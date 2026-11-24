//! Shared read-only collectors for the hive snapshot and the EvaPulse daemon.
//!
//! Both the IPC projection and the background pulse need the same committed
//! inputs: the bounded `child.workflow` window, the persistent agent roles, and
//! the dispatch/usage observations of the active model route. Keeping the
//! collection here means the panel and the daemon can never disagree about the
//! budget or about which route is primary.

use std::collections::{BTreeMap, BTreeSet};

use evohime_hive_contract::{HiveJournalProvider, WorkspaceProvider};
use evohime_model_gateway::{ModelGatewayConfig, ProviderProfileId};
use serde::{Deserialize, Serialize};

use crate::state::{
    custom_daily_cost_nano_usd, deepseek_daily_cost_nano_usd, HiveObservation, HIVE_DAY_WINDOW_MS,
    HIVE_HOUR_WINDOW_MS, HIVE_MINUTE_WINDOW_MS,
};

/// Bounded replay window used for the hive projection.
pub const HIVE_REPLAY_WINDOW: u32 = 512;

/// Lifecycle state deserialized from `child.workflow` event payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HiveChildState {
    /// Child request was created and validated.
    Created,
    /// Child request is waiting to run.
    Queued,
    /// Child execution is in progress.
    Running,
    /// Returned report is being checked against the contract.
    Validating,
    /// Checks passed but parent approval is still pending.
    WaitingParentAcceptance,
    /// Required checks passed and the parent approved the result.
    Accepted,
    /// Parent rejected the child report.
    Rejected,
    /// Child execution or report validation failed.
    Failed,
    /// Child execution was cancelled.
    Cancelled,
    /// Child lease or execution deadline expired.
    TimedOut,
    /// Owner or runtime aborted the child.
    Aborted,
    /// A required check failed and the result needs revision.
    RevisePlan,
}

/// Minimal local projection deserialized from `HiveEventRecord.payload`
/// without depending on Core's `ChildProjection` type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MinimalChildWorkflowPayload {
    /// Identifier of the delegated child task.
    pub child_task_id: String,
    /// Identifier of the parent task.
    pub parent_task_id: String,
    /// Role assigned to the child worker.
    pub role: String,
    /// Current lifecycle state of the child task.
    pub state: HiveChildState,
    /// Whether the child task was moved to dead-letter.
    #[serde(default)]
    pub dead_letter: bool,
    /// Delegation depth of the child task.
    #[serde(default)]
    pub depth: u8,
    /// Number of sub-agents spawned by this worker.
    #[serde(default)]
    pub spawned_subagents: u32,
    /// Planned sub-agent ceiling for this worker.
    #[serde(default)]
    pub planned_ceiling: u32,
}

/// Returns `"hive"` when an isolated Hive route is present and configured.
pub fn isolated_hive_route_name(config: Option<&ModelGatewayConfig>) -> Option<String> {
    let config = config?;
    config
        .routes
        .get("hive")
        .filter(|route| route.configured())
        .map(|_| "hive".to_owned())
}

/// Returns `"hive_fallback"` when an isolated Hive fallback route is present and configured.
pub fn isolated_hive_fallback_route_name(config: Option<&ModelGatewayConfig>) -> Option<String> {
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
pub fn hive_route_name(
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
pub fn hive_fallback_route_name(
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

fn effective_model_name(route_default: &str, override_model: Option<&str>) -> String {
    override_model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| route_default.trim())
        .to_owned()
}

/// Collects configured provider limits and committed usage strictly for Hive's
/// own isolated `"hive"` route.
///
/// When Hive is not configured, global routes and global chat models are never
/// substituted: the returned observation leaves provider, model, and caps unset.
pub async fn build_hive_observation_for_hive(
    journal: &dyn HiveJournalProvider,
    gateway_config: Option<&ModelGatewayConfig>,
    workspace_provider: Option<&dyn WorkspaceProvider>,
    hive_provider_profile: Option<&str>,
    hive_model: Option<&str>,
    now_ms: i64,
) -> HiveObservation {
    let mut observation = HiveObservation {
        now_ms,
        ..HiveObservation::default()
    };
    let _shell_selected_model =
        workspace_provider.and_then(|provider| provider.selected_model_name());
    let (tasks_today, average_task_time_sec) = journal.task_metrics(now_ms).await;
    observation.tasks_today = tasks_today;
    observation.average_task_time_sec = average_task_time_sec;
    let Some(config) = gateway_config else {
        return observation;
    };
    let hive_profile = hive_provider_profile
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let route_name = hive_route_name(Some(config), hive_profile);
    let Some(route_name) = route_name else {
        return observation;
    };
    let Some(route) = config.routes.get(&route_name) else {
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
    let model = effective_model_name(&route.literouter.model, effective_selected.as_deref());
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
            .model_usage_since(day_start_ms, transport, &model)
            .await
        {
            observation.daily_cost_nano_usd = custom_daily_cost_nano_usd(&custom_pricing, &usage);
            observation.daily_cost_available = observation.daily_cost_nano_usd.is_some();
        }
    } else if profile == Some(ProviderProfileId::DeepSeek) {
        if let Ok(usage) = journal
            .model_usage_since(day_start_ms, transport, &model)
            .await
        {
            observation.daily_cost_nano_usd = deepseek_daily_cost_nano_usd(&model, &usage);
            observation.daily_cost_available = observation.daily_cost_nano_usd.is_some();
        }
    }
    if let Some(quota) = route.daily_token_quota {
        observation.daily_token_quota = Some(quota);
        observation.quota_reset_utc_midnight = route.quota_reset_utc_midnight;
        if let Ok(used) = journal
            .route_tokens_since(
                route
                    .effective_reset_schedule()
                    .window_start_ms(observation.now_ms),
                &route_name,
            )
            .await
        {
            observation.daily_tokens_used = used;
        }
    }
    observation.minute_cap = limits.rpm;
    observation.hour_cap = limits.rph;
    observation.day_cap = limits.rpd;

    let per_model_limits = route
        .model_rate_limits
        .get(&model)
        .is_some_and(|entry| entry.enabled);
    let dispatch_model_scope = if per_model_limits { model.as_str() } else { "" };

    if let Ok(windows) = journal
        .model_dispatch_windows(
            transport,
            dispatch_model_scope,
            observation.now_ms,
            HIVE_MINUTE_WINDOW_MS,
            HIVE_HOUR_WINDOW_MS,
            HIVE_DAY_WINDOW_MS,
        )
        .await
    {
        observation.history_available = true;
        observation.requests_last_minute = u64::from(windows.last_minute);
        observation.requests_last_hour = u64::from(windows.last_hour);
        observation.requests_last_day = u64::from(windows.last_day);
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
            let fb_model = fallback_route.literouter.model.trim();
            if !fb_model.is_empty() {
                observation.fallback_model = Some(fb_model.to_owned());
            }
            if let Some(quota) = fallback_route.daily_token_quota {
                observation.fallback_daily_token_quota = Some(quota);
                observation.fallback_quota_reset_utc_midnight =
                    fallback_route.quota_reset_utc_midnight;
                let used = journal
                    .route_tokens_since(
                        fallback_route
                            .effective_reset_schedule()
                            .window_start_ms(observation.now_ms),
                        "hive_fallback",
                    )
                    .await
                    .unwrap_or(0);
                observation.fallback_tokens_remaining = Some(quota.saturating_sub(used));
            }
        }
    }

    observation
}

/// Replays the bounded child projection window.
///
/// Later records for the same child replace earlier ones; terminal projections
/// are retained so the pulse can report a failure that just happened.
pub async fn replay_hive_projections(
    journal: &dyn HiveJournalProvider,
) -> Vec<MinimalChildWorkflowPayload> {
    let latest = journal.latest_sequence().await;
    let after = latest.saturating_sub(i64::from(HIVE_REPLAY_WINDOW) - 1);
    let records = journal
        .replay_events(after, HIVE_REPLAY_WINDOW as usize)
        .await
        .unwrap_or_default();
    let mut by_child = BTreeMap::new();
    for record in records {
        if record.event_type != "child.workflow" {
            continue;
        }
        if let Ok(projection) =
            serde_json::from_slice::<MinimalChildWorkflowPayload>(&record.payload)
        {
            by_child.insert(projection.child_task_id.clone(), projection);
        }
    }
    let parents: BTreeMap<String, String> = by_child
        .values()
        .map(|projection| {
            (
                projection.child_task_id.clone(),
                projection.parent_task_id.clone(),
            )
        })
        .collect();
    let roots: BTreeSet<String> = by_child
        .keys()
        .map(|child| projection_root_task(child, &parents))
        .collect();
    let hive_roots: BTreeSet<String> = journal
        .filter_hive_root_tasks(roots.into_iter().collect())
        .await
        .into_iter()
        .collect();
    by_child
        .into_values()
        .filter(|projection| {
            hive_roots.contains(&projection_root_task(&projection.child_task_id, &parents))
        })
        .collect()
}

/// Resolves the task that started the whole child subtree.
fn projection_root_task(child_task_id: &str, parents: &BTreeMap<String, String>) -> String {
    let mut current = child_task_id;
    for _ in 0..8 {
        let Some(parent) = parents.get(current) else {
            return current.to_string();
        };
        if !parents.contains_key(parent) {
            return parent.clone();
        }
        current = parent;
    }
    current.to_string()
}

/// Lists active role profiles from the persistent agent registry.
pub async fn persistent_agent_roles(journal: &dyn HiveJournalProvider) -> Vec<String> {
    journal.persistent_agent_roles().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_hive_contract::{
        BoxFuture, HiveContractError, HiveDispatchWindows, HiveEventRecord, HiveModelUsage,
        HiveWorktreeLeaseRequest,
    };
    use std::collections::HashMap;

    struct MockJournal {
        events: Vec<HiveEventRecord>,
        hive_roots: Vec<String>,
    }

    impl HiveJournalProvider for MockJournal {
        fn latest_sequence(&self) -> BoxFuture<'_, i64> {
            Box::pin(async move { self.events.last().map_or(0, |e| e.sequence_id) })
        }

        fn replay_events(
            &self,
            _after_sequence: i64,
            _limit: usize,
        ) -> BoxFuture<'_, Result<Vec<HiveEventRecord>, HiveContractError>> {
            Box::pin(async move { Ok(self.events.clone()) })
        }

        fn record_audit<'a>(
            &'a self,
            _task_id: &'a str,
            _event_type: &'a str,
            _payload: &'a [u8],
        ) -> BoxFuture<'a, Result<(), HiveContractError>> {
            Box::pin(async { Ok(()) })
        }

        fn prune_signal_records<'a>(
            &'a self,
            _event_types: &'a [&'a str],
            _keep_limit: usize,
        ) -> BoxFuture<'a, ()> {
            Box::pin(async {})
        }

        fn load_hive_settings_json(
            &self,
        ) -> BoxFuture<'_, Result<Option<serde_json::Value>, HiveContractError>> {
            Box::pin(async { Ok(None) })
        }

        fn save_hive_settings_json<'a>(
            &'a self,
            _settings: &'a serde_json::Value,
        ) -> BoxFuture<'a, Result<(), HiveContractError>> {
            Box::pin(async { Ok(()) })
        }

        fn persistent_agent_roles(&self) -> BoxFuture<'_, Vec<String>> {
            Box::pin(async { Vec::new() })
        }

        fn task_metrics(&self, _now_ms: i64) -> BoxFuture<'_, (u32, u64)> {
            Box::pin(async { (0, 0) })
        }

        fn filter_hive_root_tasks(&self, roots: Vec<String>) -> BoxFuture<'_, Vec<String>> {
            Box::pin(async move {
                roots
                    .into_iter()
                    .filter(|root| self.hive_roots.contains(root))
                    .collect()
            })
        }

        fn model_dispatch_windows<'a>(
            &'a self,
            _provider: &'a str,
            _model: &'a str,
            _now_ms: i64,
            _minute_window_ms: i64,
            _hour_window_ms: i64,
            _day_window_ms: i64,
        ) -> BoxFuture<'a, Result<HiveDispatchWindows, HiveContractError>> {
            Box::pin(async { Ok(HiveDispatchWindows::default()) })
        }

        fn model_usage_since<'a>(
            &'a self,
            _start_ms: i64,
            _provider: &'a str,
            _model: &'a str,
        ) -> BoxFuture<'a, Result<Vec<HiveModelUsage>, HiveContractError>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn route_tokens_since<'a>(
            &'a self,
            _start_ms: i64,
            _route_scope: &'a str,
        ) -> BoxFuture<'a, Result<u64, HiveContractError>> {
            Box::pin(async { Ok(0) })
        }

        fn register_worktree_lease(&self, _request: HiveWorktreeLeaseRequest) -> BoxFuture<'_, ()> {
            Box::pin(async {})
        }

        fn has_active_workflow_run<'a>(&'a self, _task_id: &'a str) -> BoxFuture<'a, bool> {
            Box::pin(async { false })
        }

        fn mark_worktree_cleanup_pending<'a>(&'a self, _worktree_id: &'a str) -> BoxFuture<'a, ()> {
            Box::pin(async {})
        }
    }

    #[tokio::test]
    async fn replay_hive_projections_deserializes_minimal_payload_and_filters_roots() {
        let hive_payload = serde_json::json!({
            "child_task_id": "child-1",
            "parent_task_id": "eva-pulse-root-1",
            "role": "researcher",
            "state": "running",
            "depth": 1,
            "spawned_subagents": 1,
            "planned_ceiling": 2
        });
        let chat_payload = serde_json::json!({
            "child_task_id": "child-2",
            "parent_task_id": "chat-root-1",
            "role": "reviewer",
            "state": "running"
        });
        let journal = MockJournal {
            events: vec![
                HiveEventRecord {
                    sequence_id: 1,
                    task_id: "child-1".into(),
                    event_type: "child.workflow".into(),
                    payload: serde_json::to_vec(&hive_payload).expect("json"),
                    created_at_ms: 1000,
                },
                HiveEventRecord {
                    sequence_id: 2,
                    task_id: "child-2".into(),
                    event_type: "child.workflow".into(),
                    payload: serde_json::to_vec(&chat_payload).expect("json"),
                    created_at_ms: 1001,
                },
            ],
            hive_roots: vec!["eva-pulse-root-1".into()],
        };

        let projections = replay_hive_projections(&journal).await;
        assert_eq!(projections.len(), 1);
        assert_eq!(projections[0].child_task_id, "child-1");
        assert_eq!(projections[0].state, HiveChildState::Running);
    }

    #[test]
    fn hive_route_name_uses_only_isolated_hive_route_never_global_routes() {
        let mut routes = HashMap::new();
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

        assert_eq!(hive_route_name(Some(&config), Some("literouter")), None);
        assert_eq!(
            hive_route_name(Some(&config), Some("rout")).as_deref(),
            Some("hive")
        );
        assert_eq!(
            hive_route_name(Some(&config), None).as_deref(),
            Some("hive")
        );
    }
}
