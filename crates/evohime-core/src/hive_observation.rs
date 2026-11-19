//! Shared read-only collectors for the hive snapshot and the EvaPulse daemon.
//!
//! Both the IPC projection and the background pulse need the same committed
//! inputs: the bounded `child.workflow` window, the persistent agent roles, and
//! the dispatch/usage observations of the active model route. Keeping the
//! collection here means the panel and the daemon can never disagree about the
//! budget or about which route is primary.

use evohime_model_gateway::{ModelGatewayConfig, ProviderProfileId};

use crate::child_workflow::ChildProjection;
use crate::core_agent::SelectedModel;
use crate::hive_state::{
    custom_daily_cost_nano_usd, deepseek_daily_cost_nano_usd, HiveObservation, HIVE_DAY_WINDOW_MS,
    HIVE_HOUR_WINDOW_MS, HIVE_MINUTE_WINDOW_MS,
};
use crate::EventJournal;

/// Bounded replay window used for the hive projection.
pub(crate) const HIVE_REPLAY_WINDOW: u32 = 512;

/// Returns `"hive"` when an isolated Hive route is present and configured.
pub(crate) fn isolated_hive_route_name(config: Option<&ModelGatewayConfig>) -> Option<String> {
    let config = config?;
    config
        .routes
        .get("hive")
        .filter(|route| route.configured())
        .map(|_| "hive".to_owned())
}

/// Returns `"hive_fallback"` when an isolated Hive fallback route is present and configured.
pub(crate) fn isolated_hive_fallback_route_name(
    config: Option<&ModelGatewayConfig>,
) -> Option<String> {
    let config = config?;
    config
        .routes
        .get("hive_fallback")
        .filter(|route| route.configured())
        .map(|_| "hive_fallback".to_owned())
}

fn route_matches_profile(route: &evohime_model_gateway::ModelRouteConfig, profile: &str) -> bool {
    route
        .provider_profile_id
        .is_some_and(|id| id.as_str().eq_ignore_ascii_case(profile))
        || route.provider.as_str().eq_ignore_ascii_case(profile)
}

/// Resolves the isolated `"hive"` route when configured.
///
/// Hive never falls back to global chat routes or global provider API keys:
/// if the isolated `"hive"` route is missing or does not match the requested
/// `hive_provider_profile`, this returns `None`.
pub(crate) fn hive_route_name(
    config: Option<&ModelGatewayConfig>,
    hive_provider_profile: Option<&str>,
) -> Option<String> {
    let profile = hive_provider_profile
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let hive_route = isolated_hive_route_name(config)?;
    let matches_requested = match (profile, config.and_then(|c| c.routes.get(&hive_route))) {
        (Some(requested), Some(route)) => route_matches_profile(route, requested),
        _ => true,
    };
    matches_requested.then_some(hive_route)
}

/// Resolves the isolated `"hive_fallback"` route when configured.
pub(crate) fn hive_fallback_route_name(
    config: Option<&ModelGatewayConfig>,
    hive_fallback_provider_profile: Option<&str>,
) -> Option<String> {
    let profile = hive_fallback_provider_profile
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let fallback_route = isolated_hive_fallback_route_name(config)?;
    let matches_requested = match (profile, config.and_then(|c| c.routes.get(&fallback_route))) {
        (Some(requested), Some(route)) => route_matches_profile(route, requested),
        _ => true,
    };
    matches_requested.then_some(fallback_route)
}

/// Collects configured provider limits and committed usage strictly for Hive's
/// own isolated `"hive"` route.
///
/// When Hive is not configured, global routes and global chat models are never
/// substituted: the returned observation leaves provider, model, and caps unset.
pub(crate) async fn build_hive_observation_for_hive(
    journal: &EventJournal,
    gateway_config: Option<&ModelGatewayConfig>,
    _selected_model: Option<&str>,
    hive_provider_profile: Option<&str>,
    hive_model: Option<&str>,
    now_ms: i64,
) -> HiveObservation {
    let mut observation = HiveObservation {
        now_ms,
        ..HiveObservation::default()
    };
    let (tasks_today, average_task_time_sec) = task_metrics(journal, now_ms).await;
    observation.tasks_today = tasks_today;
    observation.average_task_time_sec = average_task_time_sec;
    let Some(config) = gateway_config else {
        return observation;
    };
    let hive_profile = hive_provider_profile
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let route_name = hive_route_name(Some(config), hive_profile);
    let Some(route) = route_name
        .as_deref()
        .and_then(|name| config.routes.get(name))
    else {
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
    // Hive uses only its own model override or the isolated `"hive"` route's
    // configured model; global chat model selection never leaks into Hive.
    let effective_selected = hive_model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let model = crate::core_agent::effective_model_name(
        &route.literouter.model,
        effective_selected.as_deref(),
    );
    let limits = route.effective_limits(&model);
    observation.provider = provider;
    observation.model = model.clone();
    let day_start_ms = observation.now_ms.div_euclid(HIVE_DAY_WINDOW_MS) * HIVE_DAY_WINDOW_MS;
    if let Some(custom_pricing) = route
        .model_rate_limits
        .get(&model)
        .and_then(|cfg| cfg.pricing)
    {
        if let Ok(usage) = journal
            .deepseek_usage_since(day_start_ms, transport, &model)
            .await
        {
            observation.daily_cost_nano_usd = custom_daily_cost_nano_usd(&custom_pricing, &usage);
            observation.daily_cost_available = observation.daily_cost_nano_usd.is_some();
        }
    } else if profile == Some(ProviderProfileId::DeepSeek) {
        if let Ok(usage) = journal
            .deepseek_usage_since(day_start_ms, transport, &model)
            .await
        {
            observation.daily_cost_nano_usd = deepseek_daily_cost_nano_usd(&model, &usage);
            observation.daily_cost_available = observation.daily_cost_nano_usd.is_some();
        }
    }
    if let Some(quota) = route.daily_token_quota {
        observation.daily_token_quota = Some(quota);
        observation.quota_reset_utc_midnight = route.quota_reset_utc_midnight;
        if let Ok(used) = journal.provider_tokens_since(day_start_ms, transport).await {
            observation.daily_tokens_used = used;
        }
    }
    observation.minute_cap = limits.rpm;

    observation.hour_cap = limits.rph;

    observation.day_cap = limits.rpd;

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

    if let Some(fallback_route) = config
        .routes
        .get("hive_fallback")
        .filter(|r| r.configured())
    {
        let fallback_transport = fallback_route.provider.as_str();
        let fallback_identity = fallback_route
            .provider_profile_id
            .map_or(fallback_transport, |p| p.as_str())
            .to_owned();
        if fallback_identity != observation.provider {
            observation.fallback_provider = Some(fallback_identity);
            if let Some(quota) = fallback_route.daily_token_quota {
                let used = journal
                    .provider_tokens_since(day_start_ms, fallback_transport)
                    .await
                    .unwrap_or(0);
                observation.fallback_tokens_remaining = Some(quota.saturating_sub(used));
            }
        }
    }

    observation
}

async fn task_metrics(journal: &EventJournal, now_ms: i64) -> (u32, u64) {
    let day_start_ms = now_ms.div_euclid(HIVE_DAY_WINDOW_MS) * HIVE_DAY_WINDOW_MS;
    let Some(day_start) = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(day_start_ms)
        .map(|timestamp| timestamp.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    else {
        return (0, 0);
    };
    let Ok(lease) = journal.checkout_read_database() else {
        return (0, 0);
    };
    let Some(database) = lease.database() else {
        return (0, 0);
    };
    let result = database.connection().query_row(
        "WITH today_tasks AS (
             SELECT task_id, MIN(sequence_id) AS start_sequence, MIN(created_at) AS started_at
             FROM events WHERE event_type = 'task.started' AND created_at >= ?1
             GROUP BY task_id
         ), completed AS (
             SELECT tasks.task_id, tasks.started_at, MIN(done.created_at) AS completed_at
             FROM today_tasks AS tasks
             JOIN events AS done ON done.task_id = tasks.task_id
                 AND done.event_type = 'task.completed'
                 AND done.sequence_id > tasks.start_sequence
             GROUP BY tasks.task_id, tasks.started_at
         )
         SELECT (SELECT COUNT(*) FROM today_tasks),
                (SELECT AVG((julianday(completed_at) - julianday(started_at)) * 86400.0)
                 FROM completed)",
        [day_start],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<f64>>(1)?)),
    );
    match result {
        Ok((count, average)) => (
            u32::try_from(count.max(0)).unwrap_or(u32::MAX),
            average
                .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
                .map_or(0, |seconds| seconds.round().min(u64::MAX as f64) as u64),
        ),
        Err(_) => (0, 0),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn task_metrics_come_from_durable_coordinator_events() {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal = EventJournal::open(directory.path().join("hive-task-metrics.db"))
            .expect("journal opens");
        let now_ms = crate::task_memory::now_millis() as i64;
        journal
            .record(&crate::CoreEvent::TaskStarted {
                task_id: "task-metrics-1".into(),
                prompt: "<redacted>".into(),
            })
            .await
            .expect("task start journals");
        journal
            .record(&crate::CoreEvent::TaskCompleted {
                task_id: "task-metrics-1".into(),
                final_message: "done".into(),
            })
            .await
            .expect("task completion journals");

        let (count, average) = task_metrics(&journal, now_ms).await;
        assert_eq!(count, 1);
        assert!(average <= 86_400);
    }

    #[test]
    fn hive_route_name_uses_only_isolated_hive_route_never_global_routes() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "hive".to_owned(),
            evohime_model_gateway::ModelRouteConfig::openai_compatible(
                "sk-hive",
                "https://api.rout.my/v1",
                "",
            )
            .with_provider_profile(ProviderProfileId::Rout, None),
        );
        routes.insert(
            "literouter".to_owned(),
            evohime_model_gateway::ModelRouteConfig::literouter(
                "lr-key",
                "https://api.literouter.com/v1",
                "glm-5.3-flash-cheap:free",
            ),
        );
        let config = ModelGatewayConfig {
            default_route: "literouter".to_owned(),
            routes,
        };

        // Must never fall back to the global "literouter" route when "hive" is a different profile.
        assert_eq!(hive_route_name(Some(&config), Some("literouter")), None);
        assert_eq!(
            hive_route_name(Some(&config), Some("rout")).as_deref(),
            Some("hive")
        );
        assert_eq!(
            hive_route_name(Some(&config), None).as_deref(),
            Some("hive")
        );

        // Without an isolated "hive" route, Hive stays unconfigured even when global routes exist.
        let mut global_only_routes = std::collections::HashMap::new();
        global_only_routes.insert(
            "default".to_owned(),
            evohime_model_gateway::ModelRouteConfig::literouter(
                "lr-global-key",
                "https://api.literouter.com/v1",
                "glm-5.3-flash-cheap:free",
            ),
        );
        global_only_routes.insert(
            "rout".to_owned(),
            evohime_model_gateway::ModelRouteConfig::openai_compatible(
                "sk-global-rout",
                "https://api.rout.my/v1",
                "",
            )
            .with_provider_profile(ProviderProfileId::Rout, None),
        );
        let global_only = ModelGatewayConfig {
            default_route: "default".to_owned(),
            routes: global_only_routes,
        };
        assert_eq!(hive_route_name(Some(&global_only), None), None);
        assert_eq!(
            hive_route_name(Some(&global_only), Some("literouter")),
            None
        );
        assert_eq!(hive_route_name(Some(&global_only), Some("rout")), None);
    }

    #[tokio::test]
    async fn hive_observation_does_not_leak_global_routes_or_models_when_unconfigured() {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal = EventJournal::open(directory.path().join("hive-provider-isolation.db"))
            .expect("journal opens");
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "default".to_owned(),
            evohime_model_gateway::ModelRouteConfig::literouter(
                "sk-global-lr",
                "https://api.literouter.com/v1",
                "global-lr-model",
            ),
        );
        routes.insert(
            "rout".to_owned(),
            evohime_model_gateway::ModelRouteConfig::openai_compatible(
                "sk-global-rout",
                "https://api.rout.my/v1",
                "openai/gpt-6-luna",
            )
            .with_provider_profile(ProviderProfileId::Rout, None),
        );
        let unconfigured = ModelGatewayConfig {
            default_route: "default".to_owned(),
            routes: routes.clone(),
        };

        let empty_obs = build_hive_observation_for_hive(
            &journal,
            Some(&unconfigured),
            Some("global-lr-model"),
            None,
            None,
            crate::task_memory::now_millis() as i64,
        )
        .await;
        assert!(empty_obs.provider.is_empty());
        assert!(empty_obs.model.is_empty());
        assert!(empty_obs.fallback_provider.is_none());

        routes.insert(
            "hive".to_owned(),
            evohime_model_gateway::ModelRouteConfig::literouter(
                "sk-hive-lr",
                "https://api.literouter.com/v1",
                "llama-3.3-70b-instruct-turbo-free",
            ),
        );
        let configured = ModelGatewayConfig {
            default_route: "default".to_owned(),
            routes,
        };

        let observation = build_hive_observation_for_hive(
            &journal,
            Some(&configured),
            Some("global-lr-model"),
            Some("literouter"),
            None,
            crate::task_memory::now_millis() as i64,
        )
        .await;

        assert_eq!(observation.provider, "literouter");
        assert_eq!(observation.model, "llama-3.3-70b-instruct-turbo-free");
        assert!(observation.fallback_provider.is_none());

        let mut routes_with_fallback = configured.routes.clone();
        routes_with_fallback.insert(
            "hive_fallback".to_owned(),
            evohime_model_gateway::ModelRouteConfig::openai_compatible(
                "sk-hive-fallback-rout",
                "https://api.rout.my/v1",
                "openai/gpt-6-luna",
            )
            .with_provider_profile(ProviderProfileId::Rout, None),
        );
        let configured_with_fallback = ModelGatewayConfig {
            default_route: "default".to_owned(),
            routes: routes_with_fallback,
        };
        let obs_with_fallback = build_hive_observation_for_hive(
            &journal,
            Some(&configured_with_fallback),
            Some("global-lr-model"),
            Some("literouter"),
            None,
            crate::task_memory::now_millis() as i64,
        )
        .await;
        assert_eq!(obs_with_fallback.fallback_provider.as_deref(), Some("rout"));
    }
}
