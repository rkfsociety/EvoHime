#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! Read-only aggregated snapshot contract of the Hive for the operations panel.

use evohime_model_gateway::ProviderLimits;
use serde::{Deserialize, Serialize};

use crate::settings::{
    HiveProviderRouteSettings, HiveSettings, DEFAULT_LLM_DISPATCH_INTERVAL_SECS,
    DEFAULT_LOCAL_OBSERVATION_INTERVAL_SECS,
};

/// Version of the serialized [`HiveStateSnapshot`] contract.
pub const HIVE_STATE_SCHEMA_VERSION: u32 = 8;
/// Name shown for the single queen in the local hive.
pub const DEFAULT_QUEEN_NAME: &str = "EvoHime";
/// Maximum number of departments rendered at once.
pub const MAX_HIVE_DEPARTMENTS: usize = 16;
/// Maximum number of active agents rendered at once.
pub const MAX_HIVE_ACTIVE_AGENTS: usize = 64;

/// Rolling window the per-model minute cap is enforced over, in milliseconds.
pub const HIVE_MINUTE_WINDOW_MS: i64 = 60_000;
/// Rolling window the per-model hourly cap is enforced over, in milliseconds.
pub const HIVE_HOUR_WINDOW_MS: i64 = 3_600_000;
/// Rolling window the per-model 4-hour cap is enforced over, in milliseconds.
pub const HIVE_FOUR_HOUR_WINDOW_MS: i64 = 4 * HIVE_HOUR_WINDOW_MS;
/// Rolling window the per-model daily cap is enforced over, in milliseconds.
pub const HIVE_DAY_WINDOW_MS: i64 = 86_400_000;

/// Start of the window a token quota is measured over.
///
/// `reset_utc_midnight` selects calendar days, otherwise the window rolls over
/// the last 24 hours.
pub fn quota_window_start_ms(now_ms: i64, reset_utc_midnight: bool) -> i64 {
    if reset_utc_midnight {
        now_ms.div_euclid(HIVE_DAY_WINDOW_MS) * HIVE_DAY_WINDOW_MS
    } else {
        now_ms.saturating_sub(HIVE_DAY_WINDOW_MS)
    }
}

/// Queen status: the hive is working.
pub const HIVE_QUEEN_ACTIVE: &str = "active";
/// Queen status: no child task is running.
pub const HIVE_QUEEN_IDLE: &str = "idle";
/// Queen status: the configured rolling request cap for the model is reached.
pub const HIVE_QUEEN_BUDGET_EXHAUSTED: &str = "budget_exhausted";
/// Autonomous observation cycle status: Level 1 observation is active.
pub const HIVE_OBSERVATION_OBSERVING: &str = "observing";
/// Autonomous observation cycle status: Level 2 LLM dispatch is waiting for limits/budget.
pub const HIVE_OBSERVATION_WAITING_FOR_LIMITS: &str = "waiting_for_limits";
/// Autonomous observation cycle status: Level 1 active, waiting for tasks or proactive evolution.
pub const HIVE_OBSERVATION_WAITING_FOR_TASKS: &str = "waiting_for_tasks";
/// Autonomous observation cycle status: observation loop is disabled.
pub const HIVE_OBSERVATION_DISABLED: &str = "disabled";
/// Autonomous observation cycle status: sleeping ONLY on `observation_failed` errors.
pub const HIVE_OBSERVATION_SLEEPING: &str = "sleeping";

/// Queen status line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveQueenState {
    /// `active` while at least one child task is running, `idle` otherwise;
    /// `budget_exhausted` when the durable dispatch projection reaches a cap.
    pub status: String,
    /// Display name of the queen.
    pub name: String,
    /// Stable reason code for a non-working status, empty while working.
    pub reason: String,
    /// Unix milliseconds at which the blocking limit is known to reset.
    ///
    /// `None` when Core cannot compute the reset moment instead of guessing it.
    pub wake_at_ms: Option<i64>,
}

/// Aggregate counters shown above the department grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveMetrics {
    /// Number of active agents in the snapshot.
    pub total_agents_online: u16,
    /// Number of child tasks currently in flight.
    pub active_tasks: u16,
    /// Number of tasks started during the current UTC day.
    pub tasks_today: u32,
    /// Mean wall-clock time of today's completed tasks, in seconds.
    pub average_task_time_sec: u64,
    /// Share of the queen-authored ceilings already used by spawned sub-agents.
    pub efficiency_percent: u8,
}

/// One role department with its current load and ceiling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveDepartment {
    /// Role identifier aggregated across child projections.
    pub role: String,
    /// Agents of this role currently active.
    pub active_count: u16,
    /// Per-role ceiling derived from [`HiveSettings`].
    pub max_capacity: u16,
}

/// One active agent rendered in the inspector list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveActiveAgent {
    /// Stable child-task identifier.
    pub id: String,
    /// Human-readable Russian name shown in the Hive UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Delegated role.
    pub role: String,
    /// Bounded description of the current lifecycle step.
    pub current_action: String,
    /// Nesting depth: 1 for a worker, 2 for a delegated sub-agent.
    pub depth: u8,
    /// Sub-agents this agent already spawned.
    pub spawned_subagents: u16,
    /// Queen-authored ceiling in force for this agent.
    pub ceiling: u16,
}

/// Task-flow counters rendered as progress bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveTaskFlow {
    /// Active tasks overall.
    pub processing: u8,
    /// Tasks in analysis-oriented roles.
    pub analyzing: u8,
    /// Tasks in planning-oriented roles.
    pub generating_plan: u8,
}

/// Active fan-out limits from [`HiveSettings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveLimits {
    /// Per-worker sub-agent ceiling.
    pub max_subagents_per_worker: u16,
    /// Maximum sub-agents a worker keeps in flight.
    pub max_parallel_subagents: u16,
}

/// Request budget actually enforced for the active model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveBudget {
    /// Provider profile identifier shown to the user, without credentials.
    pub provider: String,
    /// Whether durable dispatch history was read successfully.
    pub history_available: bool,
    /// Model identifier the counters belong to.
    pub model: String,
    /// Dispatches observed in the rolling minute window.
    pub requests_last_minute: u32,
    /// Dispatches observed in the rolling hour window.
    pub requests_last_hour: u32,
    /// Dispatches observed in the rolling 4-hour window.
    #[serde(default)]
    pub requests_last_4_hours: u32,
    /// Dispatches observed in the rolling day window.
    pub requests_last_day: u32,
    /// Configured minute cap; `0` means no minute cap.
    pub minute_cap: u32,
    /// Configured hourly cap; `0` means no hourly cap.
    pub hour_cap: u32,
    /// Configured 4-hour cap; `0` means no 4-hour cap.
    #[serde(default)]
    pub four_hour_cap: u32,
    /// Configured daily cap; `0` means no daily cap.
    pub day_cap: u32,
    /// Unified effective provider request limits across rolling windows.
    #[serde(default)]
    pub limits: ProviderLimits,
    /// Whether the minute cap is reached.
    pub minute_exhausted: bool,
    /// Whether the hourly cap is reached.
    pub hour_exhausted: bool,
    /// Whether the 4-hour cap is reached.
    #[serde(default)]
    pub four_hour_exhausted: bool,
    /// Whether the daily cap is reached.
    pub day_exhausted: bool,
    /// Whether the daily DeepSeek cost projection was read and priced completely.
    pub daily_cost_available: bool,
    /// Estimated DeepSeek API cost for the current UTC day, in nano-USD.
    pub daily_cost_nano_usd: Option<u64>,
    /// Optional daily token quota for token-pooled providers (e.g. rout.my).
    #[serde(default)]
    pub daily_token_quota: Option<u64>,
    /// Remaining tokens in the current UTC day for `daily_token_quota`.
    #[serde(default)]
    pub daily_tokens_remaining: Option<u64>,
    /// Whether `daily_token_quota` resets at 00:00 UTC.
    #[serde(default)]
    pub quota_reset_utc_midnight: bool,
    /// Configured fallback token quota, if any.
    #[serde(default)]
    pub fallback_daily_token_quota: Option<u64>,
    /// Whether the fallback token quota resets at UTC midnight.
    #[serde(default)]
    pub fallback_quota_reset_utc_midnight: bool,
    /// Configured free fallback provider identifier (e.g. `routmy`), if any.
    #[serde(default)]
    pub fallback_provider: Option<String>,
    /// Configured model for the fallback provider route, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_model: Option<String>,
    /// Remaining daily tokens for the configured fallback provider, if known.
    #[serde(default)]
    pub fallback_tokens_remaining: Option<u64>,
    /// Total tokens recorded for the configured fallback provider during its reset window.
    #[serde(default)]
    pub fallback_tokens_used: u64,
}

impl HiveBudget {
    /// Whether any configured cap is reached.
    pub fn exhausted(&self) -> bool {
        self.minute_exhausted
            || self.hour_exhausted
            || self.four_hour_exhausted
            || self.day_exhausted
    }
}

/// Bounded, read-only observations published with the hive snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HiveObservation {
    /// Provider profile identifier of the active route.
    pub provider: String,
    /// Whether durable dispatch history was read successfully.
    pub history_available: bool,
    /// Model identifier of the active route.
    pub model: String,
    /// Dispatches inside [`HIVE_MINUTE_WINDOW_MS`].
    pub requests_last_minute: u64,
    /// Dispatches inside [`HIVE_HOUR_WINDOW_MS`].
    pub requests_last_hour: u64,
    /// Dispatches inside [`HIVE_FOUR_HOUR_WINDOW_MS`].
    pub requests_last_4_hours: u64,
    /// Dispatches inside [`HIVE_DAY_WINDOW_MS`].
    pub requests_last_day: u64,
    /// Oldest dispatch still inside the minute window.
    pub oldest_minute_dispatch_ms: Option<i64>,
    /// Oldest dispatch still inside the hour window.
    pub oldest_hour_dispatch_ms: Option<i64>,
    /// Oldest dispatch still inside the 4-hour window.
    pub oldest_4_hours_dispatch_ms: Option<i64>,
    /// Oldest dispatch still inside the day window.
    pub oldest_day_dispatch_ms: Option<i64>,
    /// Configured hourly cap of the active model; `0` means unlimited.
    pub hour_cap: u32,
    /// Configured 4-hour cap of the active model; `0` means unlimited.
    pub four_hour_cap: u32,
    /// Configured minute cap of the active model; `0` means unlimited.
    pub minute_cap: u32,
    /// Configured daily cap of the active model; `0` means unlimited.
    pub day_cap: u32,
    /// Reference time used to compute rolling window boundaries.
    pub now_ms: i64,
    /// Whether current UTC-day DeepSeek usage was read and priced completely.
    pub daily_cost_available: bool,
    /// Estimated DeepSeek API cost for the current UTC day, in nano-USD.
    pub daily_cost_nano_usd: Option<u64>,
    /// Autonomous observation cycle status.
    pub observation_status: String,
    /// Unix milliseconds at which the daemon plans its next observation cycle.
    pub next_observation_at_ms: Option<i64>,
    /// Tasks started during the current UTC day according to durable Core events.
    pub tasks_today: u32,
    /// Mean duration of today's completed tasks, in seconds.
    pub average_task_time_sec: u64,
    /// Optional daily token quota of the active route.
    pub daily_token_quota: Option<u64>,
    /// Total tokens recorded for the active route during the current UTC day.
    pub daily_tokens_used: u64,
    /// Whether the active route's daily token quota resets at 00:00 UTC.
    pub quota_reset_utc_midnight: bool,
    /// Configured fallback token quota, if any.
    pub fallback_daily_token_quota: Option<u64>,
    /// Whether the fallback token quota resets at UTC midnight.
    pub fallback_quota_reset_utc_midnight: bool,
    /// Configured free fallback provider identifier, if any.
    pub fallback_provider: Option<String>,
    /// Configured model for the fallback provider route, if any.
    pub fallback_model: Option<String>,
    /// Remaining daily tokens for the configured fallback provider, if known.
    pub fallback_tokens_remaining: Option<u64>,
    /// Total tokens recorded for the configured fallback provider during its reset window.
    pub fallback_tokens_used: u64,
}

impl Default for HiveObservation {
    fn default() -> Self {
        Self {
            provider: String::new(),
            history_available: false,
            model: String::new(),
            requests_last_minute: 0,
            requests_last_hour: 0,
            requests_last_4_hours: 0,
            requests_last_day: 0,
            oldest_minute_dispatch_ms: None,
            oldest_hour_dispatch_ms: None,
            oldest_4_hours_dispatch_ms: None,
            oldest_day_dispatch_ms: None,
            hour_cap: 0,
            four_hour_cap: 0,
            minute_cap: 0,
            day_cap: 0,
            now_ms: 0,
            daily_cost_available: false,
            daily_cost_nano_usd: None,
            observation_status: HIVE_OBSERVATION_DISABLED.into(),
            next_observation_at_ms: None,
            tasks_today: 0,
            average_task_time_sec: 0,
            daily_token_quota: None,
            daily_tokens_used: 0,
            quota_reset_utc_midnight: false,
            fallback_daily_token_quota: None,
            fallback_quota_reset_utc_midnight: false,
            fallback_provider: None,
            fallback_model: None,
            fallback_tokens_remaining: None,
            fallback_tokens_used: 0,
        }
    }
}

/// One repository currently watched by Eva and the Hive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchedRepoProject {
    /// Absolute repository path configured in [`HiveSettings`].
    pub path: String,
    /// Current repository activity: `idle`, `scanning`, `working`, or `error`.
    pub status: String,
    /// Last successful scan time, when Core has one.
    pub last_scanned_at_ms: Option<i64>,
    /// Bounded description of the task currently working on this repository.
    pub current_task: Option<String>,
}

/// Aggregated, serializable hive state for the operations panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveStateSnapshot {
    /// Version of this projection contract.
    pub schema_version: u32,
    /// Queen status line.
    pub queen: HiveQueenState,
    /// Aggregate counters.
    pub metrics: HiveMetrics,
    /// Departments aggregated by role.
    pub departments: Vec<HiveDepartment>,
    /// Active agents with their fan-out progress.
    pub active_agents: Vec<HiveActiveAgent>,
    /// Task-flow counters.
    pub task_flow: HiveTaskFlow,
    /// Active fan-out limits.
    pub limits: HiveLimits,
    /// Active request budget for the current model.
    pub budget: HiveBudget,
    /// Configured daily background-work budget in USD.
    pub daily_budget_usd: f64,
    /// Validated isolated Git checkout used for self-development, if configured.
    pub eva_workspace_path: Option<String>,
    /// Validated list of Git repositories watched by the hive.
    #[serde(default)]
    pub watched_repositories: Vec<WatchedRepoProject>,
    /// Isolated provider profile configured for the hive, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hive_provider_profile: Option<String>,
    /// Isolated model override configured for the hive, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hive_model: Option<String>,
    /// Isolated fallback provider profile configured for the hive, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hive_fallback_provider_profile: Option<String>,
    /// Isolated fallback model override configured for the hive, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hive_fallback_model: Option<String>,
    /// Configured global RPM/RPH/RPD request caps for the Hive provider.
    #[serde(default)]
    pub hive_provider_limits: ProviderLimits,
    /// Configured global RPM/RPH/RPD caps for the Hive fallback provider.
    #[serde(default)]
    pub hive_fallback_provider_limits: ProviderLimits,
    /// Daily token quota configured for the primary Hive route (`0` means unlimited).
    #[serde(default)]
    pub hive_daily_token_quota: u64,
    /// Daily token quota configured for the fallback Hive route (`0` means unlimited).
    #[serde(default)]
    pub hive_fallback_daily_token_quota: u64,
    /// Whether the primary route quota was explicitly configured.
    #[serde(default)]
    pub hive_token_quota_configured: bool,
    /// Whether the fallback route quota was explicitly configured.
    #[serde(default)]
    pub hive_fallback_token_quota_configured: bool,
    /// Whether the primary route token quota resets at UTC midnight.
    #[serde(default)]
    pub hive_quota_reset_utc_midnight: bool,
    /// Whether the fallback route token quota resets at UTC midnight.
    #[serde(default)]
    pub hive_fallback_quota_reset_utc_midnight: bool,
    /// Per-provider saved settings for the primary Hive route.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub hive_provider_configs: std::collections::BTreeMap<String, HiveProviderRouteSettings>,
    /// Per-provider saved settings for the fallback Hive route.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub hive_fallback_provider_configs:
        std::collections::BTreeMap<String, HiveProviderRouteSettings>,
    /// Whether proactive evolution via the autonomous observation cycle is enabled.
    #[serde(default)]
    pub proactive_evolution_enabled: bool,
    /// Interval between Level 1 local observation cycles, in seconds.
    #[serde(default = "default_snapshot_local_observation_interval_secs")]
    pub local_observation_interval_secs: u64,
    /// Interval between Level 2 LLM dispatch cycles, in seconds.
    #[serde(default = "default_snapshot_llm_dispatch_interval_secs")]
    pub llm_dispatch_interval_secs: u64,
    /// Autonomous observation cycle status (`observing`, `waiting_for_limits`, `waiting_for_tasks`, `disabled`, or `sleeping` on `observation_failed`).
    #[serde(default, skip_serializing)]
    pub observation_status: String,
    /// Unix milliseconds at which the daemon plans its next observation cycle.
    #[serde(default, skip_serializing)]
    pub next_wake_at_ms: Option<i64>,
}

fn default_snapshot_local_observation_interval_secs() -> u64 {
    DEFAULT_LOCAL_OBSERVATION_INTERVAL_SECS
}

fn default_snapshot_llm_dispatch_interval_secs() -> u64 {
    DEFAULT_LLM_DISPATCH_INTERVAL_SECS
}

impl Default for HiveStateSnapshot {
    fn default() -> Self {
        let settings = HiveSettings::default();
        Self {
            schema_version: HIVE_STATE_SCHEMA_VERSION,
            queen: HiveQueenState {
                status: HIVE_QUEEN_IDLE.into(),
                name: DEFAULT_QUEEN_NAME.into(),
                reason: String::new(),
                wake_at_ms: None,
            },
            metrics: HiveMetrics::default(),
            departments: Vec::new(),
            active_agents: Vec::new(),
            task_flow: HiveTaskFlow::default(),
            limits: HiveLimits {
                max_subagents_per_worker: settings.effective_worker_ceiling().min(u16::MAX as u32)
                    as u16,
                max_parallel_subagents: settings.effective_parallel_limit().min(u16::MAX as u32)
                    as u16,
            },
            budget: HiveBudget::default(),
            daily_budget_usd: settings.daily_budget_usd,
            eva_workspace_path: None,
            watched_repositories: Vec::new(),
            hive_provider_profile: None,
            hive_model: None,
            hive_fallback_provider_profile: None,
            hive_fallback_model: None,
            hive_provider_limits: ProviderLimits::default(),
            hive_fallback_provider_limits: ProviderLimits::default(),
            hive_daily_token_quota: 0,
            hive_fallback_daily_token_quota: 0,
            hive_token_quota_configured: false,
            hive_fallback_token_quota_configured: false,
            hive_quota_reset_utc_midnight: false,
            hive_fallback_quota_reset_utc_midnight: false,
            hive_provider_configs: settings.effective_provider_configs(false),
            hive_fallback_provider_configs: settings.effective_provider_configs(true),
            proactive_evolution_enabled: false,
            local_observation_interval_secs: settings.local_observation_interval_secs,
            llm_dispatch_interval_secs: settings.llm_dispatch_interval_secs,
            observation_status: HIVE_OBSERVATION_DISABLED.into(),
            next_wake_at_ms: None,
        }
    }
}
