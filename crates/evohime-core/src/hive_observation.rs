//! Shared read-only collectors for the hive snapshot and the EvaPulse daemon.
//!
//! Both the IPC projection and the background pulse need the same committed
//! inputs: the bounded `child.workflow` window, the persistent agent roles, and
//! the dispatch/usage observations of the active model route. Keeping the
//! collection here means the panel and the daemon can never disagree about the
//! budget or about which route is primary.

use evohime_model_gateway::providers::ProviderKind;
use evohime_model_gateway::{ModelGatewayConfig, ProviderProfileId};

use crate::child_workflow::ChildProjection;
use crate::core_agent::SelectedModel;
use crate::hive_state::{
    deepseek_daily_cost_nano_usd, HiveObservation, HIVE_DAY_WINDOW_MS, HIVE_HOUR_WINDOW_MS,
    HIVE_MINUTE_WINDOW_MS,
};
use crate::EventJournal;

/// Bounded replay window used for the hive projection.
pub(crate) const HIVE_REPLAY_WINDOW: u32 = 512;

/// Whether the active route is the free LiteRouter primary that background work
/// is allowed to use.
///
/// EvaPulse never falls back to a paid DeepSeek profile: when the route is not
/// LiteRouter (or an explicit DeepSeek profile), the daemon may still observe
/// the hive but must not dispatch a recovery task.
pub(crate) fn route_is_background_primary(config: Option<&ModelGatewayConfig>) -> bool {
    let Some(config) = config else {
        return false;
    };
    let Some(route) = config.routes.get(&config.default_route) else {
        return false;
    };
    route.provider == ProviderKind::LiteRouter
        && route.provider_profile_id != Some(ProviderProfileId::DeepSeek)
}

/// Returns the route name background work may use, if it is the primary route.
pub(crate) fn background_route_name(config: Option<&ModelGatewayConfig>) -> Option<String> {
    route_is_background_primary(config).then(|| {
        config
            .as_ref()
            .map(|config| config.default_route.clone())
            .unwrap_or_default()
    })
}

/// Collects configured provider limits and committed usage for the active route.
///
/// The projection is additive: a missing route configuration leaves caps unset;
/// an unreadable provenance row preserves known caps but marks the counters
/// unavailable so the caller cannot mistake missing history for zero.
pub(crate) async fn build_hive_observation(
    journal: &EventJournal,
    gateway_config: Option<&ModelGatewayConfig>,
    selected_model: Option<&str>,
    now_ms: i64,
) -> HiveObservation {
    let mut observation = HiveObservation {
        now_ms,
        ..HiveObservation::default()
    };
    let Some(config) = gateway_config else {
        return observation;
    };
    let Some(route) = config.routes.get(&config.default_route) else {
        return observation;
    };
    // The ledger records the transport kind, so dispatch counters must be
    // queried by that exact value. The vendor profile is the readable identity
    // and decides whether the DeepSeek tariff applies.
    let transport = route.provider.as_str();
    let profile = route.provider_profile_id;
    let provider = profile
        .map_or(transport, |profile| profile.as_str())
        .to_owned();
    // The user-selected model overrides the configured default, so the caps
    // and counters must follow the model that will actually be dispatched.
    let selected_model = selected_model
        .map(str::to_owned)
        .filter(|value| !value.trim().is_empty());
    let model =
        crate::core_agent::effective_model_name(&route.literouter.model, selected_model.as_deref());
    let limits = route.model_rate_limits.get(&model);
    observation.provider = provider;
    observation.model = model.clone();
    if profile == Some(ProviderProfileId::DeepSeek) {
        let day_start_ms = observation.now_ms.div_euclid(HIVE_DAY_WINDOW_MS) * HIVE_DAY_WINDOW_MS;
        if let Ok(usage) = journal
            .deepseek_usage_since(day_start_ms, transport, &model)
            .await
        {
            observation.daily_cost_nano_usd = deepseek_daily_cost_nano_usd(&model, &usage);
            observation.daily_cost_available = observation.daily_cost_nano_usd.is_some();
        }
    }
    observation.minute_cap = limits
        .filter(|limits| limits.enabled)
        .map_or(0, |limits| limits.rpm);
    observation.hour_cap = limits
        .filter(|limits| limits.enabled)
        .map_or(0, |limits| limits.rph);
    observation.day_cap = limits
        .filter(|limits| limits.enabled)
        .map_or(0, |limits| limits.rpd);
    if let Ok(windows) = journal
        .model_dispatch_windows(
            transport,
            &model,
            observation.now_ms,
            HIVE_MINUTE_WINDOW_MS,
            HIVE_HOUR_WINDOW_MS,
            HIVE_DAY_WINDOW_MS,
        )
        .await
    {
        observation.history_available = true;
        observation.requests_last_minute = windows.last_minute;
        observation.requests_last_hour = windows.last_hour;
        observation.requests_last_day = windows.last_day;
        observation.oldest_minute_dispatch_ms = windows.oldest_minute_ms;
        observation.oldest_hour_dispatch_ms = windows.oldest_hour_ms;
        observation.oldest_day_dispatch_ms = windows.oldest_day_ms;
    }
    observation
}

/// Replays the bounded child projection window.
///
/// Later records for the same child replace earlier ones; terminal projections
/// are retained so the pulse can report a failure that just happened.
pub(crate) async fn replay_hive_projections(journal: &EventJournal) -> Vec<ChildProjection> {
    let latest = journal.latest_sequence().await;
    let after = latest.saturating_sub(i64::from(HIVE_REPLAY_WINDOW) - 1);
    let records = journal
        .replay(after, HIVE_REPLAY_WINDOW as usize)
        .await
        .unwrap_or_default();
    let mut by_child = std::collections::BTreeMap::new();
    for record in records {
        if record.event_type != "child.workflow" {
            continue;
        }
        if let Ok(projection) = serde_json::from_slice::<ChildProjection>(&record.payload) {
            by_child.insert(projection.child_task_id.clone(), projection);
        }
    }
    by_child.into_values().collect()
}

/// Lists active role profiles from the persistent agent registry.
///
/// Raw agent records never leave Core; only the bounded role identifier is used
/// to keep an idle department visible in the snapshot.
pub(crate) async fn persistent_agent_roles(journal: &EventJournal) -> Vec<String> {
    let database = journal.database().lock().await;
    let Ok(agents) =
        evohime_local_storage::domains::agents::load_agents(database.connection(), 256)
    else {
        return Vec::new();
    };
    let mut roles: Vec<String> = agents
        .iter()
        .filter_map(|bytes| {
            let agent: crate::persistent_agent_registry::PersistentAgent =
                serde_json::from_slice(bytes).ok()?;
            if agent.status != crate::persistent_agent_registry::LifecycleStatus::Active {
                return None;
            }
            Some(agent.role_profile_ref.id)
        })
        .collect();
    roles.sort();
    roles.dedup();
    roles
}

/// Reads the model identity the shell selected, if any.
pub(crate) fn selected_model_name(selected: &SelectedModel) -> Option<String> {
    selected.get().map(|value| value.to_string())
}
