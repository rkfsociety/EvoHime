//! EvaPulse: the Core-owned background daemon of the hive.
//!
//! The daemon wakes on a bounded interval, reads the same committed projection
//! as the operations panel, and either logs a healthy cycle or asks the
//! coordinator to investigate failed workers. It never retries a rate-limited
//! provider request and it never uses the paid DeepSeek fallback: the only
//! route allowed for a recovery task is the primary LiteRouter route.
//!
//! Budget protection is deliberately conservative. A zero daily budget, a
//! reached daily estimate, or an exhausted provider window parks the loop until
//! the exact reset moment Core can compute. When budget/usage history is
//! unavailable the daemon does not invent a spend value; it also does not gain
//! access to any paid fallback, so an unknown cost cannot turn into a charge.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use evohime_model_gateway::ModelGatewayConfig;

use crate::child_workflow::CoordinatorState;
use crate::core_agent::SelectedModel;
use crate::eva_pulse_boundary::{self, EvaPulseSignal, EvaPulseSignalKind};
use crate::hive_settings::HiveSettings;
use crate::hive_state::{
    HiveStateSnapshot, HIVE_DAY_WINDOW_MS, HIVE_PULSE_ACTIVE, HIVE_PULSE_DISABLED,
    HIVE_PULSE_SLEEPING,
};
use crate::logging::StructuredLogger;
use crate::{CoreCommand, EventJournal, TaskCoordinator};

/// Maximum number of failed workers named in one recovery task.
const MAX_REPORTED_FAILURES: usize = 8;
/// Maximum number of child ids kept in the deduplication set.
const MAX_SEEN_FAILURES: usize = 512;
/// Maximum number of uncompleted roadmap items passed to the Queen in one cycle.
const MAX_ROADMAP_ITEMS: usize = 30;
/// Fallback interval used when settings cannot be read.
const FALLBACK_INTERVAL: Duration = Duration::from_secs(1_800);
/// Keep a small response allowance so a quota-limited provider can complete a task.
const ROUTMY_MIN_TOKENS_THRESHOLD: u64 = 1_000;

/// Boxed future used by the probe/sink seams.
pub(crate) type PulseFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Bounded diagnostic context extracted for one failed child worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FailedWorkerContext {
    /// Identifier of the failed child task.
    pub child_task_id: String,
    /// Role assigned to the child worker.
    pub role: String,
    /// Machine-readable reason code from `child.workflow` or `task.failed`.
    pub reason_code: Option<String>,
    /// Last tool or command started by the worker, if recorded.
    pub last_tool: Option<String>,
    /// Target file path extracted from the last tool arguments, if present.
    pub file_path: Option<String>,
}

impl FailedWorkerContext {
    /// Builds a minimal context carrying only the child task identifier.
    pub(crate) fn from_id(id: impl Into<String>) -> Self {
        Self {
            child_task_id: id.into(),
            role: String::new(),
            reason_code: None,
            last_tool: None,
            file_path: None,
        }
    }

    /// Formats a bounded human-readable summary for the recovery prompt.
    pub(crate) fn format_summary(&self) -> String {
        let id: String = self.child_task_id.chars().take(128).collect();
        let mut details = Vec::new();
        if !self.role.trim().is_empty() {
            details.push(format!("роль={}", self.role.trim()));
        }
        if let Some(reason) = self.reason_code.as_deref().filter(|s| !s.trim().is_empty()) {
            details.push(format!("причина={}", reason.trim()));
        }
        if let Some(tool) = self.last_tool.as_deref().filter(|s| !s.trim().is_empty()) {
            details.push(format!("инструмент={}", tool.trim()));
        }
        if let Some(path) = self.file_path.as_deref().filter(|s| !s.trim().is_empty()) {
            details.push(format!("файл={}", path.trim()));
        }
        if details.is_empty() {
            id
        } else {
            format!("{id} ({})", details.join(", "))
        }
    }
}

/// Observable result of one background cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PulseCycleReport {
    /// Interval requested by the persisted settings.
    pub interval_secs: u64,
    /// Estimated spend for the current UTC day, when the tariff is known.
    pub daily_cost_nano_usd: Option<u64>,
    /// Daily ceiling from [`HiveSettings`], in nano-USD; `0` blocks all work.
    pub daily_budget_nano_usd: u64,
    /// Whether a configured provider request window is exhausted.
    pub provider_window_exhausted: bool,
    /// Exact moment the exhausted window frees a slot, when known.
    pub provider_window_wake_at_ms: Option<i64>,
    /// Explicit provider rate-limit window, when the caller observed one.
    pub rate_limited_until_ms: Option<i64>,
    /// Newly terminal child workers that still need a decision.
    pub failed_workers: Vec<String>,
    /// Enriched diagnostic contexts for `failed_workers`.
    pub failed_worker_contexts: Vec<FailedWorkerContext>,
    /// Whether the active route is the primary LiteRouter route.
    pub route_is_primary: bool,
    /// Validated isolated checkout configured for Queen self-development.
    pub eva_workspace_path: Option<String>,
    /// Whether proactive evolution is enabled in `HiveSettings`.
    pub proactive_evolution_enabled: bool,
    /// Uncompleted `[ ]` roadmap items eligible for proactive evolution.
    pub roadmap_items: Vec<RoadmapItem>,
    /// Active provider profile identifier (e.g. `"literouter"`, `"routmy"`).
    pub selected_provider: Option<String>,
    /// Configured free fallback provider transport (`OpenAICompatible` for `rout.my`).
    pub fallback_provider: Option<evohime_model_gateway::providers::ProviderKind>,
    /// Remaining daily tokens for `rout.my`, if configured.
    pub routmy_tokens_remaining: Option<u64>,
    /// Whether the selected token quota resets at UTC midnight.
    pub quota_reset_utc_midnight: bool,
    /// Whether paid fallback (e.g. DeepSeek) is explicitly allowed in settings.
    pub allow_paid_fallback: bool,
    /// Whether all free LiteRouter models are currently in cooldown.
    pub literouter_all_models_exhausted: bool,
}

/// Discovered uncompleted roadmap item and its workspace-relative file path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoadmapItem {
    /// Relative path to the roadmap/TODO file inside `eva_workspace_path`.
    pub file_path: String,
    /// 1-based line number of the item inside `file_path`.
    pub line: usize,
    /// Text of the uncompleted `[ ]` item.
    pub text: String,
}

/// Pure decision for one cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PulseDecision {
    /// Park the loop until the limit resets.
    Sleep {
        /// Last blocking reset moment, when known.
        wake_at_ms: Option<i64>,
        /// Stable reason code for logs and the snapshot.
        reason: &'static str,
    },
    /// Dispatch one bounded recovery task.
    Recover {
        /// Newly failed child ids.
        failed_workers: Vec<String>,
    },
    /// Dispatch one proactive self-development task from the roadmap.
    Evolve {
        /// Candidate uncompleted roadmap items for the Queen to choose from.
        items: Vec<RoadmapItem>,
    },
    /// Nothing needs attention.
    AllGood,
}

/// Source of one bounded background observation.
pub(crate) trait PulseProbe: Send + Sync {
    /// Reads committed state; a failure parks the loop instead of inventing work.
    fn observe(&self) -> PulseFuture<'_, Result<PulseCycleReport, String>>;

    /// Records that a recovery task was dispatched for the given failed workers.
    fn mark_recovery_dispatched(&self, _task_id: &str, _failed_workers: &[String]) {}

    /// Records that a proactive evolution task was dispatched for the given candidate items.
    fn mark_evolution_dispatched(
        &self,
        _task_id: &str,
        _eva_workspace_path: Option<&str>,
        _items: &[RoadmapItem],
    ) {
    }
}

/// Sink that asks Core to investigate failed workers or evolve proactively.
pub(crate) trait PulseTaskSink: Send + Sync {
    /// Dispatches one bounded recovery task and returns its task id.
    fn dispatch_recovery(
        &self,
        failed_workers: &[FailedWorkerContext],
        eva_workspace_path: Option<&str>,
    ) -> PulseFuture<'_, Result<String, String>>;

    /// Dispatches one bounded recovery task via the fallback free provider (e.g. `rout.my`).
    fn dispatch_recovery_fallback(
        &self,
        failed_workers: &[FailedWorkerContext],
        eva_workspace_path: Option<&str>,
        fallback_provider: evohime_model_gateway::providers::ProviderKind,
    ) -> PulseFuture<'_, Result<String, String>> {
        let _ = (failed_workers, eva_workspace_path, fallback_provider);
        Box::pin(async { Err("fallback_provider_unavailable".to_owned()) })
    }

    /// Dispatches one proactive evolution task for candidate roadmap items and returns its task id.
    fn dispatch_evolution(
        &self,
        items: &[RoadmapItem],
        eva_workspace_path: Option<&str>,
    ) -> PulseFuture<'_, Result<String, String>>;
}

/// Process-wide status projection of the daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PulseStatus {
    /// `active`, `disabled`, or `sleeping`.
    pub status: String,
    /// Unix milliseconds at which the daemon plans its next cycle.
    pub next_wake_at_ms: Option<i64>,
    /// Whether the user left the daemon enabled.
    pub enabled: bool,
}

/// Test seam for the loop interval.
#[derive(Debug, Clone, Copy, Default)]
pub struct EvaPulseOptions {
    /// Overrides the persisted interval; production leaves this unset.
    pub interval_override: Option<Duration>,
}

struct DaemonState {
    enabled: bool,
    status: &'static str,
    next_wake_at_ms: Option<i64>,
    cycles: u64,
    reason: &'static str,
}

struct Inner {
    probe: Arc<dyn PulseProbe>,
    tasks: Arc<dyn PulseTaskSink>,
    options: EvaPulseOptions,
    logger: Option<Arc<StructuredLogger>>,
    state: std::sync::Mutex<DaemonState>,
    dispatched_evolutions: std::sync::Mutex<HashSet<String>>,
    notify: tokio::sync::Notify,
    shutdown: tokio_util::sync::CancellationToken,
    started: AtomicBool,
    cycle_counter: AtomicU64,
}

/// Cloneable handle to the Core-owned background loop.
#[derive(Clone)]
pub struct EvaPulseDaemon {
    inner: Arc<Inner>,
}

impl EvaPulseDaemon {
    /// Starts the production daemon backed by the durable journal.
    ///
    /// The daemon reads `HiveSettings` before every cycle, so a changed interval
    /// or budget takes effect without restarting Core.
    pub fn start(
        journal: EventJournal,
        gateway_config: Option<ModelGatewayConfig>,
        selected_model: SelectedModel,
        coordinator: TaskCoordinator,
        logger: Option<Arc<StructuredLogger>>,
    ) -> Self {
        let probe = Arc::new(JournalPulseProbe::new(
            journal.clone(),
            gateway_config.clone(),
            selected_model,
        ));
        let sink = Arc::new(
            CoordinatorPulseTaskSink::new(
                coordinator,
                crate::hive_observation::isolated_hive_route_name(gateway_config.as_ref()),
            )
            .with_hive_settings_source(journal, gateway_config),
        );
        let daemon = Self::spawn(probe, sink, EvaPulseOptions::default(), logger);
        daemon.run();
        daemon
    }

    /// Builds a daemon from explicit probe and sink seams.
    pub(crate) fn spawn(
        probe: Arc<dyn PulseProbe>,
        tasks: Arc<dyn PulseTaskSink>,
        options: EvaPulseOptions,
        logger: Option<Arc<StructuredLogger>>,
    ) -> Self {
        let inner = Arc::new(Inner {
            probe,
            tasks,
            options,
            logger,
            state: std::sync::Mutex::new(DaemonState {
                enabled: true,
                status: HIVE_PULSE_ACTIVE,
                next_wake_at_ms: None,
                cycles: 0,
                reason: "",
            }),
            dispatched_evolutions: std::sync::Mutex::new(HashSet::new()),
            notify: tokio::sync::Notify::new(),
            shutdown: tokio_util::sync::CancellationToken::new(),
            started: AtomicBool::new(false),
            cycle_counter: AtomicU64::new(0),
        });
        Self { inner }
    }

    /// Starts the loop once; repeated calls are no-ops.
    pub fn run(&self) {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move { inner.run_loop().await });
    }

    /// Enables or parks the loop.
    ///
    /// Disabling wakes a sleeping loop immediately and changes the published
    /// status to `disabled`; enabling runs a cycle without waiting for the
    /// previous interval to elapse.
    pub fn set_enabled(&self, enabled: bool) {
        {
            let mut state = self.lock_state();
            state.enabled = enabled;
            state.next_wake_at_ms = None;
            state.status = if enabled {
                HIVE_PULSE_ACTIVE
            } else {
                HIVE_PULSE_DISABLED
            };
            state.reason = if enabled { "" } else { "user_disabled" };
        }
        self.inner.notify.notify_one();
    }

    /// Returns the current published status.
    pub fn status(&self) -> PulseStatus {
        let state = self.lock_state();
        PulseStatus {
            status: state.status.to_owned(),
            next_wake_at_ms: state.next_wake_at_ms,
            enabled: state.enabled,
        }
    }

    /// Number of completed cycles; used by focused tests.
    pub fn cycle_count(&self) -> u64 {
        self.inner.cycle_counter.load(Ordering::SeqCst)
    }

    /// Stops the loop and releases its task.
    pub fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.inner.notify.notify_one();
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, DaemonState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Inner {
    fn state(&self) -> std::sync::MutexGuard<'_, DaemonState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn run_loop(self: Arc<Self>) {
        let mut wait = Duration::ZERO;
        loop {
            if self.shutdown.is_cancelled() {
                return;
            }
            if !self.state().enabled {
                self.publish(HIVE_PULSE_DISABLED, None, "user_disabled");
                tokio::select! {
                    _ = self.shutdown.cancelled() => return,
                    _ = self.notify.notified() => continue,
                }
            }
            if !wait.is_zero() {
                tokio::select! {
                    _ = self.shutdown.cancelled() => return,
                    _ = self.notify.notified() => {
                        wait = Duration::ZERO;
                        continue;
                    }
                    _ = tokio::time::sleep(wait) => {}
                }
            }
            if self.shutdown.is_cancelled() {
                return;
            }
            if !self.state().enabled {
                continue;
            }
            wait = self.run_cycle().await;
        }
    }

    async fn run_cycle(&self) -> Duration {
        self.publish(HIVE_PULSE_ACTIVE, None, "");
        let report = match self.probe.observe().await {
            Ok(report) => report,
            Err(error) => {
                self.log(
                    "warning",
                    "eva_pulse.cycle_failed",
                    serde_json::json!({ "error": error }),
                );
                let interval = self.interval(None);
                self.publish(
                    HIVE_PULSE_SLEEPING,
                    next_wake_at(interval),
                    "observation_failed",
                );
                self.cycle_counter.fetch_add(1, Ordering::SeqCst);
                return interval;
            }
        };
        // A toggle that arrived while the probe was reading wins over the
        // decision: the daemon must not dispatch work after the user stopped it.
        if !self.state().enabled {
            self.publish(HIVE_PULSE_DISABLED, None, "user_disabled");
            return Duration::ZERO;
        }
        let now = crate::task_memory::now_millis() as i64;
        let interval = self.interval(Some(report.interval_secs));
        let decision = decide(&report, now);
        self.cycle_counter.fetch_add(1, Ordering::SeqCst);
        match decision {
            PulseDecision::Sleep { wake_at_ms, reason } => {
                let wait = wait_until(wake_at_ms, now, interval);
                self.publish(
                    HIVE_PULSE_SLEEPING,
                    wake_at_ms.or_else(|| next_wake_at(interval)),
                    reason,
                );
                self.log(
                    "info",
                    "eva_pulse.sleeping",
                    serde_json::json!({
                        "reason": reason,
                        "next_wake_at_ms": wake_at_ms,
                        "budget_nano_usd": report.daily_budget_nano_usd,
                        "spent_nano_usd": report.daily_cost_nano_usd,
                    }),
                );
                wait
            }
            PulseDecision::Recover { failed_workers } => {
                let contexts = if report.failed_worker_contexts.is_empty() {
                    failed_workers
                        .iter()
                        .cloned()
                        .map(FailedWorkerContext::from_id)
                        .collect::<Vec<_>>()
                } else {
                    report.failed_worker_contexts.clone()
                };
                let can_use_routmy = report.fallback_provider.is_some()
                    && report
                        .routmy_tokens_remaining
                        .is_none_or(|rem| rem >= ROUTMY_MIN_TOKENS_THRESHOLD);

                if report.literouter_all_models_exhausted {
                    if let (Some(fallback_provider), true) =
                        (report.fallback_provider, can_use_routmy)
                    {
                        self.dispatch_recovery_via_fallback(
                            &contexts,
                            &failed_workers,
                            report.eva_workspace_path.as_deref(),
                            fallback_provider,
                        )
                        .await;
                        self.publish(HIVE_PULSE_ACTIVE, next_wake_at(interval), "");
                        return interval;
                    }
                }

                if !report.route_is_primary {
                    if let (Some(fallback_provider), true) =
                        (report.fallback_provider, can_use_routmy)
                    {
                        self.dispatch_recovery_via_fallback(
                            &contexts,
                            &failed_workers,
                            report.eva_workspace_path.as_deref(),
                            fallback_provider,
                        )
                        .await;
                        self.publish(HIVE_PULSE_ACTIVE, next_wake_at(interval), "");
                        return interval;
                    }
                    self.publish(
                        HIVE_PULSE_SLEEPING,
                        next_wake_at(interval),
                        "primary_route_unavailable",
                    );
                    self.log(
                        "warning",
                        "eva_pulse.recovery_skipped",
                        serde_json::json!({
                            "reason": "primary_route_unavailable",
                            "failed_workers": failed_workers,
                        }),
                    );
                    return interval;
                }

                match self
                    .tasks
                    .dispatch_recovery(&contexts, report.eva_workspace_path.as_deref())
                    .await
                {
                    Ok(task_id) => {
                        self.probe
                            .mark_recovery_dispatched(&task_id, &failed_workers);
                        self.log(
                            "warning",
                            "eva_pulse.recovery_dispatched",
                            serde_json::json!({
                                "task_id": task_id,
                                "failed_workers": failed_workers,
                                "used_fallback": false,
                            }),
                        );
                    }
                    Err(error) if is_all_models_exhausted_error(&error) => {
                        if let Some(fallback_provider) = report.fallback_provider {
                            if report
                                .routmy_tokens_remaining
                                .is_none_or(|remaining| remaining >= ROUTMY_MIN_TOKENS_THRESHOLD)
                            {
                                self.dispatch_recovery_via_fallback(
                                    &contexts,
                                    &failed_workers,
                                    report.eva_workspace_path.as_deref(),
                                    fallback_provider,
                                )
                                .await;
                            } else if !report.allow_paid_fallback {
                                let wake_at = quota_wake_at(now, report.quota_reset_utc_midnight);
                                self.publish(
                                    HIVE_PULSE_SLEEPING,
                                    Some(wake_at),
                                    "routmy_quota_exhausted",
                                );
                                return wait_until(Some(wake_at), now, interval);
                            }
                        } else {
                            self.log(
                                "warning",
                                "eva_pulse.recovery_failed",
                                serde_json::json!({ "error": error }),
                            );
                        }
                    }
                    Err(error) => {
                        self.log(
                            "warning",
                            "eva_pulse.recovery_failed",
                            serde_json::json!({ "error": error }),
                        );
                    }
                }
                self.publish(HIVE_PULSE_ACTIVE, next_wake_at(interval), "");
                interval
            }
            PulseDecision::Evolve { items } => {
                if !report.route_is_primary {
                    self.publish(
                        HIVE_PULSE_SLEEPING,
                        next_wake_at(interval),
                        "primary_route_unavailable",
                    );
                    self.log(
                        "warning",
                        "eva_pulse.evolution_skipped",
                        serde_json::json!({
                            "reason": "primary_route_unavailable",
                            "items_count": items.len(),
                        }),
                    );
                    return interval;
                }
                let batch_key = items
                    .iter()
                    .map(|item| {
                        evolution_item_key(
                            report.eva_workspace_path.as_deref(),
                            Some(&item.file_path),
                            &item.text,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("|");
                let already_dispatched = {
                    let guard = self
                        .dispatched_evolutions
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    guard.contains(&batch_key)
                };
                if already_dispatched {
                    self.publish(HIVE_PULSE_ACTIVE, next_wake_at(interval), "");
                    return interval;
                }
                match self
                    .tasks
                    .dispatch_evolution(&items, report.eva_workspace_path.as_deref())
                    .await
                {
                    Ok(task_id) => {
                        {
                            let mut guard = self
                                .dispatched_evolutions
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            if guard.len() >= MAX_SEEN_FAILURES {
                                guard.clear();
                            }
                            guard.insert(batch_key);
                        }
                        self.probe.mark_evolution_dispatched(
                            &task_id,
                            report.eva_workspace_path.as_deref(),
                            &items,
                        );
                        self.log(
                            "info",
                            "eva_pulse.evolution_dispatched",
                            serde_json::json!({
                                "task_id": task_id,
                                "items_count": items.len(),
                            }),
                        );
                    }
                    Err(error) => {
                        self.log(
                            "warning",
                            "eva_pulse.evolution_failed",
                            serde_json::json!({
                                "error": error,
                                "items_count": items.len(),
                            }),
                        );
                    }
                }
                self.publish(HIVE_PULSE_ACTIVE, next_wake_at(interval), "");
                interval
            }
            PulseDecision::AllGood => {
                tracing::info!("EvaPulse: all good");
                self.log(
                    "info",
                    "eva_pulse.all_good",
                    serde_json::json!({ "cycles": self.cycle_counter.load(Ordering::SeqCst) }),
                );
                self.publish(HIVE_PULSE_ACTIVE, next_wake_at(interval), "");
                interval
            }
        }
    }

    fn interval(&self, settings_secs: Option<u64>) -> Duration {
        if let Some(override_interval) = self.options.interval_override {
            return override_interval.max(Duration::from_millis(1));
        }
        settings_secs
            .map(|secs| Duration::from_secs(secs.max(1)))
            .unwrap_or(FALLBACK_INTERVAL)
    }

    fn publish(&self, status: &'static str, next_wake_at_ms: Option<i64>, reason: &'static str) {
        let mut state = self.state();
        state.status = status;
        state.next_wake_at_ms = next_wake_at_ms;
        state.reason = reason;
    }

    fn log(&self, level: &str, event: &str, fields: serde_json::Value) {
        if let Some(logger) = self.logger.as_ref() {
            let _ = logger.write(level, event, fields);
        }
    }

    async fn dispatch_recovery_via_fallback(
        &self,
        contexts: &[FailedWorkerContext],
        failed_workers: &[String],
        eva_workspace_path: Option<&str>,
        fallback_provider: evohime_model_gateway::providers::ProviderKind,
    ) {
        match self
            .tasks
            .dispatch_recovery_fallback(contexts, eva_workspace_path, fallback_provider)
            .await
        {
            Ok(task_id) => {
                self.probe
                    .mark_recovery_dispatched(&task_id, failed_workers);
                self.log(
                    "warning",
                    "eva_pulse.recovery_dispatched",
                    serde_json::json!({
                        "task_id": task_id,
                        "failed_workers": failed_workers,
                        "used_fallback": true,
                        "fallback_provider": fallback_provider.as_str(),
                    }),
                );
            }
            Err(error) => {
                self.log(
                    "warning",
                    "eva_pulse.recovery_failed",
                    serde_json::json!({
                        "error": error,
                        "used_fallback": true,
                    }),
                );
            }
        }
    }
}

/// Decides what one cycle does.
///
/// The order is the safety contract: an explicit 429 window wins, then the
/// daily ceiling, then a configured provider window, and only then may a
/// recovery task be created.
pub(crate) fn decide(report: &PulseCycleReport, now_ms: i64) -> PulseDecision {
    if let Some(until) = report.rate_limited_until_ms {
        return PulseDecision::Sleep {
            wake_at_ms: Some(until),
            reason: "provider_rate_limited",
        };
    }
    let budget_exhausted = report.daily_cost_nano_usd.is_some_and(|spent| {
        report.daily_budget_nano_usd == 0 || spent >= report.daily_budget_nano_usd
    });
    if budget_exhausted {
        return PulseDecision::Sleep {
            wake_at_ms: Some(next_utc_midnight(now_ms)),
            reason: "daily_budget_exhausted",
        };
    }
    let routmy_active = report.selected_provider.as_deref() == Some("rout");
    let routmy_exhausted = report
        .routmy_tokens_remaining
        .is_some_and(|rem| rem < ROUTMY_MIN_TOKENS_THRESHOLD);
    if routmy_active && routmy_exhausted && !report.allow_paid_fallback {
        return PulseDecision::Sleep {
            wake_at_ms: Some(quota_wake_at(now_ms, report.quota_reset_utc_midnight)),
            reason: "routmy_quota_exhausted",
        };
    }
    let routmy_fallback_available = report.fallback_provider.is_some()
        && report
            .routmy_tokens_remaining
            .is_none_or(|rem| rem >= ROUTMY_MIN_TOKENS_THRESHOLD);
    if report.literouter_all_models_exhausted && !routmy_fallback_available {
        if report.fallback_provider.is_some() && routmy_exhausted && !report.allow_paid_fallback {
            return PulseDecision::Sleep {
                wake_at_ms: Some(quota_wake_at(now_ms, report.quota_reset_utc_midnight)),
                reason: "routmy_quota_exhausted",
            };
        }
        if !report.allow_paid_fallback {
            return PulseDecision::Sleep {
                wake_at_ms: report.provider_window_wake_at_ms,
                reason: "literouter_all_models_exhausted",
            };
        }
    }
    if report.provider_window_exhausted && !routmy_fallback_available {
        return PulseDecision::Sleep {
            wake_at_ms: report.provider_window_wake_at_ms,
            reason: "provider_window_exhausted",
        };
    }
    if !report.failed_workers.is_empty() {
        return PulseDecision::Recover {
            failed_workers: report.failed_workers.clone(),
        };
    }
    if report.proactive_evolution_enabled
        && report.eva_workspace_path.is_some()
        && !report.roadmap_items.is_empty()
    {
        return PulseDecision::Evolve {
            items: report.roadmap_items.clone(),
        };
    }
    PulseDecision::AllGood
}

fn is_all_models_exhausted_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("all_models_exhausted") || lower.contains("all models exhausted")
}

pub(crate) fn next_utc_midnight(now_ms: i64) -> i64 {
    (now_ms.div_euclid(HIVE_DAY_WINDOW_MS) + 1).saturating_mul(HIVE_DAY_WINDOW_MS)
}

fn quota_wake_at(now_ms: i64, reset_utc_midnight: bool) -> i64 {
    if reset_utc_midnight {
        next_utc_midnight(now_ms)
    } else {
        now_ms.saturating_add(3_600_000)
    }
}

/// Reports whether the configured token quota of one isolated Hive route is spent.
///
/// The dispatch gateway keeps the startup configuration, so Hive's durable
/// quota is checked here against the same route scope the usage rows record.
async fn hive_route_quota_exhausted(
    journal: &EventJournal,
    route: &str,
    quota: u64,
    reset_utc_midnight: bool,
) -> bool {
    if quota == 0 {
        return false;
    }
    let now_ms = crate::task_memory::now_millis() as i64;
    let start_ms = crate::hive_state::quota_window_start_ms(now_ms, reset_utc_midnight);
    journal
        .route_tokens_since(start_ms, route)
        .await
        .is_ok_and(|used| used >= quota)
}

fn next_wake_at(interval: Duration) -> Option<i64> {
    let now = crate::task_memory::now_millis() as i64;
    Some(now.saturating_add(i64::try_from(interval.as_millis()).unwrap_or(i64::MAX)))
}

fn wait_until(wake_at_ms: Option<i64>, now_ms: i64, interval: Duration) -> Duration {
    let Some(wake_at_ms) = wake_at_ms else {
        return interval;
    };
    let remaining = wake_at_ms.saturating_sub(now_ms).max(1);
    Duration::from_millis(u64::try_from(remaining).unwrap_or(u64::MAX))
}

/// Converts the persisted USD budget to the nano-USD unit used by the ledger.
pub fn usd_to_nano_usd(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let nanos = value * 1_000_000_000.0;
    if nanos >= u64::MAX as f64 {
        u64::MAX
    } else {
        nanos.round() as u64
    }
}

struct PendingRecovery {
    task_id: String,
    failed_workers: Vec<String>,
}

struct PendingEvolutionItem {
    item_key: String,
    file_path: String,
    text: String,
}

struct PendingEvolution {
    task_id: String,
    items: Vec<PendingEvolutionItem>,
}

enum EvolutionOutcome {
    Succeeded,
    Skipped,
    Failed(String),
}

/// Production probe backed by the journal and the active route configuration.
pub(crate) struct JournalPulseProbe {
    journal: EventJournal,
    gateway_config: Option<ModelGatewayConfig>,
    selected_model: SelectedModel,
    seen_failures: std::sync::Mutex<HashSet<String>>,
    pending_recoveries: std::sync::Mutex<Vec<PendingRecovery>>,
    pending_evolutions: std::sync::Mutex<Vec<PendingEvolution>>,
    handled_evolutions: std::sync::Mutex<HashSet<String>>,
    first_scan: AtomicBool,
}

impl JournalPulseProbe {
    /// Creates a probe over the shared journal and route configuration.
    pub(crate) fn new(
        journal: EventJournal,
        gateway_config: Option<ModelGatewayConfig>,
        selected_model: SelectedModel,
    ) -> Self {
        Self {
            journal,
            gateway_config,
            selected_model,
            seen_failures: std::sync::Mutex::new(HashSet::new()),
            pending_recoveries: std::sync::Mutex::new(Vec::new()),
            pending_evolutions: std::sync::Mutex::new(Vec::new()),
            handled_evolutions: std::sync::Mutex::new(HashSet::new()),
            first_scan: AtomicBool::new(true),
        }
    }

    fn record_handled_evolution(&self, item_key: &str) {
        let mut handled = self
            .handled_evolutions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if handled.len() >= MAX_SEEN_FAILURES {
            handled.clear();
        }
        handled.insert(item_key.to_owned());
    }

    fn reconcile_pending_evolutions(&self, records: &[evohime_local_storage::EventRecord]) {
        let mut pending = self
            .pending_evolutions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if pending.is_empty() {
            return;
        }
        let mut still_pending = Vec::with_capacity(pending.len());
        for entry in pending.drain(..) {
            match evolution_task_outcome(records, &entry.task_id) {
                Some(EvolutionOutcome::Succeeded) => {
                    // The Queen marked the completed item as `[x]` in the roadmap file;
                    // the remaining candidate items stay eligible for future cycles.
                }
                Some(EvolutionOutcome::Skipped) => {
                    for item in &entry.items {
                        self.record_handled_evolution(&item.item_key);
                    }
                }
                Some(EvolutionOutcome::Failed(details)) => {
                    let matched_by_text: Vec<&PendingEvolutionItem> = entry
                        .items
                        .iter()
                        .filter(|item| !item.text.is_empty() && details.contains(&item.text))
                        .collect();
                    if !matched_by_text.is_empty() {
                        for item in matched_by_text {
                            self.record_handled_evolution(&item.item_key);
                        }
                    } else {
                        let matched_by_file: Vec<&PendingEvolutionItem> = entry
                            .items
                            .iter()
                            .filter(|item| {
                                !item.file_path.is_empty() && details.contains(&item.file_path)
                            })
                            .collect();
                        if !matched_by_file.is_empty() {
                            for item in matched_by_file {
                                self.record_handled_evolution(&item.item_key);
                            }
                        } else {
                            for item in &entry.items {
                                self.record_handled_evolution(&item.item_key);
                            }
                        }
                    }
                }
                None => {
                    still_pending.push(entry);
                }
            }
        }
        *pending = still_pending;
    }

    fn collect_roadmap_items(&self, settings: &HiveSettings) -> Vec<RoadmapItem> {
        if !settings.proactive_evolution_enabled {
            return Vec::new();
        }
        let repositories = settings.all_workspace_paths();
        if repositories.is_empty() {
            return Vec::new();
        }
        let pending = self
            .pending_evolutions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !pending.is_empty() {
            return Vec::new();
        }
        drop(pending);
        let handled = self
            .handled_evolutions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut items = Vec::new();
        for workspace_path in repositories {
            let Ok(workspace) =
                evohime_tool_runtime::self_development::EvaWorkspace::open(workspace_path)
            else {
                continue;
            };
            for file in find_roadmap_files(workspace.root()) {
                let Ok(content) = std::fs::read_to_string(&file) else {
                    continue;
                };
                let relative = file
                    .strip_prefix(workspace.root())
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");
                for (line, text) in extract_uncompleted_roadmap_items(&content) {
                    let key = evolution_item_key(Some(workspace_path), Some(&relative), &text);
                    if !handled.contains(&key) {
                        items.push(RoadmapItem {
                            file_path: relative.clone(),
                            line,
                            text,
                        });
                        if items.len() >= MAX_ROADMAP_ITEMS {
                            return items;
                        }
                    }
                }
            }
        }
        items
    }

    fn reconcile_pending_recoveries(
        &self,
        records: &[evohime_local_storage::EventRecord],
    ) -> HashSet<String> {
        let mut pending = self
            .pending_recoveries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if pending.is_empty() {
            return HashSet::new();
        }
        let mut seen = self
            .seen_failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut in_flight = HashSet::new();
        let mut still_pending = Vec::with_capacity(pending.len());
        for entry in pending.drain(..) {
            match task_terminal_outcome(records, &entry.task_id) {
                Some(true) => {
                    for id in entry.failed_workers {
                        if seen.len() < MAX_SEEN_FAILURES {
                            seen.insert(id);
                        }
                    }
                }
                Some(false) => {
                    // Recovery task failed (e.g. 429 rate limit) or stopped:
                    // drop from pending without marking workers as seen so the
                    // next wake retries the incident.
                }
                None => {
                    for id in &entry.failed_workers {
                        in_flight.insert(id.clone());
                    }
                    still_pending.push(entry);
                }
            }
        }
        *pending = still_pending;
        in_flight
    }

    fn new_failures(
        &self,
        projections: &[crate::child_workflow::ChildProjection],
        in_flight: &HashSet<String>,
    ) -> Vec<String> {
        let first_scan = self.first_scan.swap(false, Ordering::SeqCst);
        let mut seen = self
            .seen_failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut failures = Vec::new();
        for projection in projections {
            if !is_terminal_failure(projection) {
                continue;
            }
            if seen.contains(&projection.child_task_id) {
                continue;
            }
            if first_scan {
                if seen.len() < MAX_SEEN_FAILURES {
                    seen.insert(projection.child_task_id.clone());
                }
                continue;
            }
            if in_flight.contains(&projection.child_task_id) {
                continue;
            }
            if failures.len() < MAX_REPORTED_FAILURES {
                failures.push(projection.child_task_id.clone());
            }
        }
        failures
    }
}

impl PulseProbe for JournalPulseProbe {
    fn mark_recovery_dispatched(&self, task_id: &str, failed_workers: &[String]) {
        let mut pending = self
            .pending_recoveries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if pending.len() >= 32 {
            pending.remove(0);
        }
        pending.push(PendingRecovery {
            task_id: task_id.to_owned(),
            failed_workers: failed_workers.to_vec(),
        });
    }

    fn mark_evolution_dispatched(
        &self,
        task_id: &str,
        eva_workspace_path: Option<&str>,
        items: &[RoadmapItem],
    ) {
        let mut pending = self
            .pending_evolutions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if pending.len() >= 32 {
            pending.remove(0);
        }
        let pending_items = items
            .iter()
            .map(|item| PendingEvolutionItem {
                item_key: evolution_item_key(eva_workspace_path, Some(&item.file_path), &item.text),
                file_path: item.file_path.clone(),
                text: item.text.clone(),
            })
            .collect();
        pending.push(PendingEvolution {
            task_id: task_id.to_owned(),
            items: pending_items,
        });
    }

    fn observe(&self) -> PulseFuture<'_, Result<PulseCycleReport, String>> {
        Box::pin(async move {
            let settings = self.journal.hive_settings().await.unwrap_or_default();
            let now_ms = crate::task_memory::now_millis() as i64;
            let selected_model = crate::hive_observation::selected_model_name(&self.selected_model);
            let mut adjusted_config = self.gateway_config.clone();
            if let Some(config) = adjusted_config.as_mut() {
                config.apply_hive_route_overrides(
                    settings.hive_provider_profile.as_deref(),
                    settings.hive_model.as_deref(),
                );
                config.apply_hive_fallback_route_overrides(
                    settings.hive_fallback_provider_profile.as_deref(),
                    settings.hive_fallback_model.as_deref(),
                );
                config.apply_hive_route_limits(
                    "hive",
                    settings.hive_provider_limits,
                    settings.hive_daily_token_quota,
                    settings.hive_quota_reset_utc_midnight,
                );
                config.apply_hive_route_limits(
                    "hive_fallback",
                    settings.hive_fallback_provider_limits,
                    settings.hive_fallback_daily_token_quota,
                    settings.hive_fallback_quota_reset_utc_midnight,
                );
            }
            let observation = crate::hive_observation::build_hive_observation_for_hive(
                &self.journal,
                adjusted_config.as_ref(),
                selected_model.as_deref(),
                settings.hive_provider_profile.as_deref(),
                settings.hive_model.as_deref(),
                now_ms,
            )
            .await;
            let latest = self.journal.latest_sequence().await;
            let after =
                latest.saturating_sub(i64::from(crate::hive_observation::HIVE_REPLAY_WINDOW) - 1);
            let records = self
                .journal
                .replay(after, crate::hive_observation::HIVE_REPLAY_WINDOW as usize)
                .await
                .unwrap_or_default();
            let projections = replay_projections_from_records(&records);
            let roles = crate::hive_observation::persistent_agent_roles(&self.journal).await;
            let snapshot =
                HiveStateSnapshot::from_observation(&settings, &projections, &roles, &observation);
            let in_flight = self.reconcile_pending_recoveries(&records);
            self.reconcile_pending_evolutions(&records);
            let failed_workers = self.new_failures(&projections, &in_flight);
            let failed_worker_contexts =
                build_failed_worker_contexts(&failed_workers, &projections, &records);
            let roadmap_items = if failed_workers.is_empty() {
                self.collect_roadmap_items(&settings)
            } else {
                Vec::new()
            };
            let rate_limit_model = observation.model.as_str();
            let rate_limited_until_ms = if rate_limit_model.is_empty() {
                None
            } else {
                evohime_model_gateway::rate_limited_until(rate_limit_model)
            };
            let allow_paid_fallback = std::env::var("MODEL_FREE_ACCESS_ALLOW_PAID_FALLBACK")
                .ok()
                .is_some_and(|value| {
                    let trimmed = value.trim();
                    trimmed.eq_ignore_ascii_case("true") || trimmed == "1"
                });
            let selected_provider =
                (!observation.provider.is_empty()).then(|| observation.provider.clone());
            let fallback_provider = observation
                .fallback_provider
                .as_deref()
                .map(|_| evohime_model_gateway::providers::ProviderKind::OpenAICompatible);
            let configured_rout_quota =
                settings.hive_daily_token_quota > 0 || settings.hive_fallback_daily_token_quota > 0;
            let quota_reset_utc_midnight = if observation.provider == "rout" {
                settings.hive_quota_reset_utc_midnight
            } else {
                settings.hive_fallback_quota_reset_utc_midnight
            };
            let routmy_tokens_remaining = if !configured_rout_quota {
                None
            } else if observation.provider == "rout" {
                snapshot.budget.daily_tokens_remaining
            } else {
                snapshot.budget.fallback_tokens_remaining
            };
            let route_is_primary = crate::hive_observation::hive_route_name(
                adjusted_config.as_ref(),
                settings.hive_provider_profile.as_deref(),
            )
            .is_some();
            let eva_workspace_path = settings.primary_workspace_path().map(str::to_owned);
            Ok(PulseCycleReport {
                interval_secs: settings.pulse_interval_secs,
                daily_cost_nano_usd: observation.daily_cost_nano_usd,
                daily_budget_nano_usd: usd_to_nano_usd(settings.daily_budget_usd),
                provider_window_exhausted: snapshot.budget.exhausted(),
                provider_window_wake_at_ms: snapshot.queen.wake_at_ms,
                rate_limited_until_ms,
                failed_workers,
                failed_worker_contexts,
                route_is_primary,
                eva_workspace_path,
                proactive_evolution_enabled: settings.proactive_evolution_enabled,
                roadmap_items,
                selected_provider,
                fallback_provider,
                routmy_tokens_remaining,
                quota_reset_utc_midnight,
                allow_paid_fallback,
                literouter_all_models_exhausted: false,
            })
        })
    }
}

fn replay_projections_from_records(
    records: &[evohime_local_storage::EventRecord],
) -> Vec<crate::child_workflow::ChildProjection> {
    let mut by_child = std::collections::BTreeMap::new();
    for record in records {
        if record.event_type != "child.workflow" {
            continue;
        }
        if let Ok(projection) =
            serde_json::from_slice::<crate::child_workflow::ChildProjection>(&record.payload)
        {
            by_child.insert(projection.child_task_id.clone(), projection);
        }
    }
    by_child.into_values().collect()
}

fn task_terminal_outcome(
    records: &[evohime_local_storage::EventRecord],
    target_task_id: &str,
) -> Option<bool> {
    let mut outcome = None;
    for record in records {
        match record.event_type.as_str() {
            "task.completed"
                if extract_event_task_id(&record.payload).as_deref() == Some(target_task_id) =>
            {
                outcome = Some(true);
            }
            "task.failed" | "task.stopped"
                if extract_event_task_id(&record.payload).as_deref() == Some(target_task_id) =>
            {
                outcome = Some(false);
            }
            _ => {}
        }
    }
    outcome
}

fn evolution_task_outcome(
    records: &[evohime_local_storage::EventRecord],
    target_task_id: &str,
) -> Option<EvolutionOutcome> {
    let mut outcome = None;
    for record in records {
        if extract_event_task_id(&record.payload).as_deref() != Some(target_task_id) {
            continue;
        }
        match record.event_type.as_str() {
            "task.completed" => {
                let message =
                    extract_event_text_field(&record.payload, "TaskCompleted", "final_message");
                let trimmed = message.trim();
                let is_skip = trimmed == "SKIP"
                    || trimmed.starts_with("SKIP ")
                    || trimmed.starts_with("SKIP\n")
                    || trimmed.starts_with("SKIP\r")
                    || trimmed.starts_with("SKIP:")
                    || trimmed.starts_with("SKIP.");
                if is_skip {
                    outcome = Some(EvolutionOutcome::Skipped);
                } else if trimmed.starts_with("FAILED") || trimmed.contains("FAILED:") {
                    outcome = Some(EvolutionOutcome::Failed(message));
                } else {
                    outcome = Some(EvolutionOutcome::Succeeded);
                }
            }
            "task.failed" => {
                let error = extract_event_text_field(&record.payload, "TaskFailed", "error");
                outcome = Some(EvolutionOutcome::Failed(error));
            }
            "task.stopped" => {
                outcome = Some(EvolutionOutcome::Failed(String::new()));
            }
            _ => {}
        }
    }
    outcome
}

fn extract_event_text_field(payload: &[u8], variant: &str, field: &str) -> String {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(payload) else {
        return String::new();
    };
    value
        .get(field)
        .or_else(|| value.get(variant).and_then(|inner| inner.get(field)))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned()
}

fn extract_event_task_id(payload: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<serde_json::Value>(payload).ok()?;
    if let Some(id) = value.get("task_id").and_then(|v| v.as_str()) {
        return Some(id.to_owned());
    }
    for variant in ["TaskCompleted", "TaskFailed", "TaskStopped", "ToolStarted"] {
        if let Some(id) = value
            .get(variant)
            .and_then(|inner| inner.get("task_id"))
            .and_then(|v| v.as_str())
        {
            return Some(id.to_owned());
        }
    }
    None
}

fn build_failed_worker_contexts(
    failed_workers: &[String],
    projections: &[crate::child_workflow::ChildProjection],
    records: &[evohime_local_storage::EventRecord],
) -> Vec<FailedWorkerContext> {
    failed_workers
        .iter()
        .map(|worker_id| {
            let projection = projections
                .iter()
                .find(|item| item.child_task_id == *worker_id);
            let role = projection.map(|item| item.role.clone()).unwrap_or_default();
            let mut reason_code = projection.and_then(|item| item.reason_code.clone());
            if reason_code.as_deref().is_none_or(|s| s.trim().is_empty()) {
                reason_code = find_task_failure_reason(records, worker_id);
            }
            let (last_tool, file_path) = find_last_tool_and_path(records, worker_id);
            FailedWorkerContext {
                child_task_id: worker_id.clone(),
                role,
                reason_code,
                last_tool,
                file_path,
            }
        })
        .collect()
}

fn find_task_failure_reason(
    records: &[evohime_local_storage::EventRecord],
    target_task_id: &str,
) -> Option<String> {
    let mut found = None;
    for record in records {
        if record.event_type != "task.failed" {
            continue;
        }
        if extract_event_task_id(&record.payload).as_deref() != Some(target_task_id) {
            continue;
        }
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&record.payload) {
            let reason = value
                .get("reason_code")
                .or_else(|| value.get("error"))
                .and_then(|v| v.as_str())
                .map(|s| s.chars().take(128).collect::<String>());
            if reason.is_some() {
                found = reason;
            }
        }
    }
    found
}

fn find_last_tool_and_path(
    records: &[evohime_local_storage::EventRecord],
    target_task_id: &str,
) -> (Option<String>, Option<String>) {
    let mut last_tool = None;
    let mut file_path = None;
    for record in records {
        if record.event_type != "tool.started" {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&record.payload) else {
            continue;
        };
        let inner = value.get("ToolStarted").unwrap_or(&value);
        let Some(task_id) = inner.get("task_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(tool_name) = inner.get("tool_name").and_then(|v| v.as_str()) else {
            continue;
        };
        let arguments = inner
            .get("arguments")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        if task_id != target_task_id {
            continue;
        }
        let parsed_args = if arguments.trim().is_empty() {
            None
        } else {
            serde_json::from_str::<serde_json::Value>(&arguments).ok()
        };
        let command = parsed_args.as_ref().and_then(|args| {
            args.get("command")
                .or_else(|| args.get("CommandLine"))
                .or_else(|| args.get("cmd"))
                .and_then(|v| v.as_str())
                .map(|s| s.chars().take(96).collect::<String>())
        });
        last_tool = Some(match command {
            Some(cmd) if !cmd.trim().is_empty() => format!("{tool_name}: {cmd}"),
            _ => tool_name.to_owned(),
        });
        file_path = parsed_args.as_ref().and_then(|args| {
            [
                "path",
                "file_path",
                "file",
                "target_path",
                "target",
                "filename",
            ]
            .iter()
            .find_map(|key| args.get(*key).and_then(|v| v.as_str()))
            .map(|s| s.chars().take(256).collect::<String>())
        });
    }
    (last_tool, file_path)
}

fn is_terminal_failure(projection: &crate::child_workflow::ChildProjection) -> bool {
    projection.dead_letter
        || matches!(
            projection.state,
            CoordinatorState::Failed
                | CoordinatorState::Rejected
                | CoordinatorState::TimedOut
                | CoordinatorState::Aborted
        )
}

/// Production sink: one bounded Core task, never a paid provider fallback.
pub(crate) struct CoordinatorPulseTaskSink {
    coordinator: TaskCoordinator,
    primary_route: Option<String>,
    preferred_route_hint: Option<String>,
    journal: Option<EventJournal>,
    gateway_config: Option<ModelGatewayConfig>,
}

impl CoordinatorPulseTaskSink {
    /// Creates a sink bound to the coordinator and the allowed route name.
    pub(crate) fn new(coordinator: TaskCoordinator, primary_route: Option<String>) -> Self {
        Self {
            coordinator,
            primary_route,
            preferred_route_hint: None,
            journal: None,
            gateway_config: None,
        }
    }

    /// Sets an explicit `preferred_route_hint` for Hive background tasks.
    #[allow(dead_code)]
    pub(crate) fn with_preferred_route_hint(mut self, hint: Option<String>) -> Self {
        self.preferred_route_hint = hint
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        self
    }

    /// Attaches the journal and gateway config so Hive can resolve
    /// `hive_provider_profile` dynamically without restarting Core.
    pub(crate) fn with_hive_settings_source(
        mut self,
        journal: EventJournal,
        gateway_config: Option<ModelGatewayConfig>,
    ) -> Self {
        self.journal = Some(journal);
        self.gateway_config = gateway_config;
        self
    }
}

fn build_recovery_prompt(
    failed_workers: &[FailedWorkerContext],
    eva_workspace_path: Option<&str>,
) -> String {
    let failed = failed_workers
        .iter()
        .take(MAX_REPORTED_FAILURES)
        .map(FailedWorkerContext::format_summary)
        .collect::<Vec<_>>();
    let workspace_path = eva_workspace_path
        .map(str::trim)
        .filter(|path| !path.is_empty());
    let incident = format!("завершившиеся с ошибкой воркеры: {}", failed.join("; "));
    recovery_task_prompt(&incident, workspace_path)
}

/// Applies the typed EvaPulse → Hive boundary before creating a Hive task.
///
/// The model prompt is intentionally not placed in the signal envelope. The
/// envelope carries only bounded routing metadata; the accepted signal is
/// then translated into the existing Core task path below.
fn validate_hive_signal(signal_kind: EvaPulseSignalKind) -> Result<(), String> {
    let now_ms: i64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "pulse_clock_before_unix_epoch".to_owned())?
        .as_millis()
        .try_into()
        .map_err(|_| "pulse_clock_out_of_range".to_owned())?;
    let signal = EvaPulseSignal {
        schema_version: eva_pulse_boundary::CONTRACT_VERSION,
        message_id: uuid::Uuid::new_v4().to_string(),
        correlation_id: uuid::Uuid::new_v4().to_string(),
        source: "eva-pulse".to_owned(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
        kind: signal_kind,
        created_at_ms: now_ms,
        expires_at_ms: now_ms + 30 * 60 * 1_000,
        payload: Vec::new(),
    };
    eva_pulse_boundary::validate_signal(&signal, now_ms)
        .map_err(|error| format!("hive_signal_rejected:{error}"))
}

impl PulseTaskSink for CoordinatorPulseTaskSink {
    fn dispatch_recovery(
        &self,
        failed_workers: &[FailedWorkerContext],
        eva_workspace_path: Option<&str>,
    ) -> PulseFuture<'_, Result<String, String>> {
        let prompt = build_recovery_prompt(failed_workers, eva_workspace_path);
        self.dispatch_primary_task(prompt, EvaPulseSignalKind::RecoveryReview)
    }

    fn dispatch_recovery_fallback(
        &self,
        failed_workers: &[FailedWorkerContext],
        eva_workspace_path: Option<&str>,
        _fallback_provider: evohime_model_gateway::providers::ProviderKind,
    ) -> PulseFuture<'_, Result<String, String>> {
        let prompt = build_recovery_prompt(failed_workers, eva_workspace_path);
        self.dispatch_fallback_task(prompt, EvaPulseSignalKind::RecoveryReview)
    }

    fn dispatch_evolution(
        &self,
        items: &[RoadmapItem],
        _eva_workspace_path: Option<&str>,
    ) -> PulseFuture<'_, Result<String, String>> {
        let prompt = proactive_evolution_prompt(items);
        self.dispatch_primary_task(prompt, EvaPulseSignalKind::EvolutionReview)
    }
}

impl CoordinatorPulseTaskSink {
    fn dispatch_primary_task(
        &self,
        prompt: String,
        signal_kind: EvaPulseSignalKind,
    ) -> PulseFuture<'_, Result<String, String>> {
        let primary_route = self.primary_route.clone();
        let explicit_hint = self.preferred_route_hint.clone();
        let journal = self.journal.clone();
        let gateway_config = self.gateway_config.clone();
        Box::pin(async move {
            validate_hive_signal(signal_kind)?;
            if let Some(journal) = journal.as_ref() {
                let settings = journal.hive_settings().await.unwrap_or_default();
                if hive_route_quota_exhausted(
                    journal,
                    "hive",
                    settings.hive_daily_token_quota,
                    settings.hive_quota_reset_utc_midnight,
                )
                .await
                {
                    return Err("hive_quota_exhausted".to_owned());
                }
            }
            let mut preferred_route_hint = explicit_hint;
            if preferred_route_hint.is_none() {
                if let Some(mut config) = gateway_config {
                    let settings = if let Some(journal) = journal.as_ref() {
                        journal.hive_settings().await.ok()
                    } else {
                        None
                    };
                    let hive_profile = settings
                        .as_ref()
                        .and_then(|s| s.hive_provider_profile.as_deref())
                        .map(str::trim)
                        .filter(|value| !value.is_empty());
                    let hive_model = settings.as_ref().and_then(|s| s.hive_model.as_deref());
                    config.apply_hive_route_overrides(hive_profile, hive_model);
                    if let Some(settings) = settings.as_ref() {
                        config.apply_hive_route_limits(
                            "hive",
                            settings.hive_provider_limits,
                            settings.hive_daily_token_quota,
                            settings.hive_quota_reset_utc_midnight,
                        );
                    }
                    preferred_route_hint =
                        crate::hive_observation::hive_route_name(Some(&config), hive_profile);
                } else {
                    preferred_route_hint = primary_route;
                }
            }
            let preferred_route_hint =
                preferred_route_hint.ok_or_else(|| "primary_route_unavailable".to_owned())?;
            let task_id = uuid::Uuid::new_v4().to_string();
            self.coordinator
                .dispatch(CoreCommand::StartTask {
                    task_id: task_id.clone(),
                    prompt,
                    workspace_root: None,
                    preferred_route_hint: Some(preferred_route_hint),
                    conversation: None,
                    goal_id: None,
                    session_binding: None,
                })
                .await
                .map_err(|error| error.to_string())?;
            Ok(task_id)
        })
    }

    fn dispatch_fallback_task(
        &self,
        prompt: String,
        signal_kind: EvaPulseSignalKind,
    ) -> PulseFuture<'_, Result<String, String>> {
        let journal = self.journal.clone();
        let gateway_config = self.gateway_config.clone();
        Box::pin(async move {
            validate_hive_signal(signal_kind)?;
            if let Some(journal) = journal.as_ref() {
                let settings = journal.hive_settings().await.unwrap_or_default();
                if hive_route_quota_exhausted(
                    journal,
                    "hive_fallback",
                    settings.hive_fallback_daily_token_quota,
                    settings.hive_fallback_quota_reset_utc_midnight,
                )
                .await
                {
                    return Err("hive_quota_exhausted".to_owned());
                }
            }
            let mut config =
                gateway_config.ok_or_else(|| "fallback_route_unavailable".to_owned())?;
            let settings = if let Some(journal) = journal.as_ref() {
                journal.hive_settings().await.ok()
            } else {
                None
            };
            let fallback_profile = settings
                .as_ref()
                .and_then(|s| s.hive_fallback_provider_profile.as_deref())
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let fallback_model = settings
                .as_ref()
                .and_then(|s| s.hive_fallback_model.as_deref());
            config.apply_hive_fallback_route_overrides(fallback_profile, fallback_model);
            if let Some(settings) = settings.as_ref() {
                config.apply_hive_route_limits(
                    "hive_fallback",
                    settings.hive_fallback_provider_limits,
                    settings.hive_fallback_daily_token_quota,
                    settings.hive_fallback_quota_reset_utc_midnight,
                );
            }
            let route_name =
                crate::hive_observation::hive_fallback_route_name(Some(&config), fallback_profile)
                    .ok_or_else(|| "fallback_route_unavailable".to_owned())?;
            let task_id = uuid::Uuid::new_v4().to_string();
            self.coordinator
                .dispatch(CoreCommand::StartTask {
                    task_id: task_id.clone(),
                    prompt,
                    workspace_root: None,
                    preferred_route_hint: Some(route_name),
                    conversation: None,
                    goal_id: None,
                    session_binding: None,
                })
                .await
                .map_err(|error| error.to_string())?;
            Ok(task_id)
        })
    }
}

const PROJECT_CONTEXT_PREAMBLE: &str = "\
ПЕРЕД ТЕМ КАК ПИСАТЬ КОД, ты обязана изучить контекст проекта. Выполни следующие шаги:\n\
1. Прочитай файл `AGENTS.md` в корне проекта (если он есть) с помощью `eva_workspace_read_file`. Строго соблюдай правила из него (например, правила коммитов, ветвления и запреты).\n\
2. Прочитай `docs/architecture.md` (если он есть), чтобы понять, как устроен проект.\n\
3. Выполни команду `git log -n 5` в песочнице, чтобы увидеть, что делалось в проекте до тебя.\n\
Только после этого приступай к изменениям.\n\n";

fn format_roadmap_items_list(items: &[RoadmapItem]) -> String {
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            format!(
                "{}. [{}:{}] {}",
                index + 1,
                item.file_path,
                item.line,
                item.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn proactive_evolution_prompt(items: &[RoadmapItem]) -> String {
    let items_list = format_roadmap_items_list(items);
    format!(
        "Ты — Королева Ева. Ты работаешь в автономном режиме. Вот список невыполненных задач из твоих файлов планов:\n\
{items_list}\n\n\
Проанализируй этот список. Выбери РОВНО ОДНУ задачу, которую ты можешь безопасно и качественно реализовать прямо сейчас (учитывая твои инструменты `eva_workspace_*` и контекст проекта из `AGENTS.md`).\n\n\
Если задача требует слишком много ресурсов или ты не уверена в её безопасности, выбери другую. Если ВСЕ задачи слишком сложные, верни ответ \"SKIP\".\n\n\
Если ты выбрала задачу, твои действия:\n\
1. Прочитай `AGENTS.md` и `docs/architecture.md`.\n\
2. Выполни `git log -n 5`.\n\
3. Реализуй выбранную задачу, используя `eva_workspace_write_file`.\n\
4. Запусти тесты и линтеры.\n\
5. Если успешно — сделай коммит, пуш и отметь пункт как `[x]` в файле планов.\n\
6. Если не получилось — опиши проблему и верни \"FAILED: [причина]\"."
    )
}

fn evolution_item_key(
    eva_workspace_path: Option<&str>,
    eva_roadmap_path: Option<&str>,
    item: &str,
) -> String {
    format!(
        "{}::{}::{}",
        eva_workspace_path.unwrap_or("").trim(),
        eva_roadmap_path.unwrap_or("").trim(),
        item.trim()
    )
}

fn is_excluded_roadmap_dir(name: &std::ffi::OsStr) -> bool {
    matches!(name.to_str(), Some(".git" | "target" | "node_modules"))
}

fn push_if_workspace_file(
    workspace_root: &std::path::Path,
    candidate: std::path::PathBuf,
    out: &mut Vec<std::path::PathBuf>,
) {
    let Ok(canonical) = candidate.canonicalize() else {
        return;
    };
    if canonical.starts_with(workspace_root) && canonical.is_file() {
        out.push(canonical);
    }
}

/// Discovers roadmap and TODO markdown files inside `workspace_path` in priority order.
///
/// Priority order:
/// 1. `docs/development-plan.md`
/// 2. `docs/plans/*.md` (sorted by file name)
/// 3. `TODO.md` and `ROADMAP.md` in the workspace root
/// 4. `docs/current-state.md`
pub(crate) fn find_roadmap_files(workspace_path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(root) = workspace_path.canonicalize() else {
        return Vec::new();
    };
    let mut files = Vec::new();
    push_if_workspace_file(&root, root.join("docs/development-plan.md"), &mut files);

    let plans_dir = root.join("docs/plans");
    if plans_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&plans_dir) {
            let mut plan_files = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.file_name().is_some_and(is_excluded_roadmap_dir) {
                    continue;
                }
                let is_md = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
                if is_md && path.is_file() {
                    plan_files.push(path);
                }
            }
            plan_files.sort();
            for path in plan_files {
                push_if_workspace_file(&root, path, &mut files);
            }
        }
    }

    push_if_workspace_file(&root, root.join("TODO.md"), &mut files);
    push_if_workspace_file(&root, root.join("ROADMAP.md"), &mut files);
    push_if_workspace_file(&root, root.join("docs/current-state.md"), &mut files);
    files
}

fn extract_uncompleted_roadmap_items(content: &str) -> Vec<(usize, String)> {
    let mut items = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let rest = trimmed
            .strip_prefix("- [ ] ")
            .or_else(|| trimmed.strip_prefix("* [ ] "))
            .or_else(|| trimmed.strip_prefix("[ ] "))
            .or_else(|| {
                let (prefix, tail) = trimmed.split_once("[ ] ")?;
                let prefix_trimmed = prefix.trim();
                let digits = prefix_trimmed
                    .strip_suffix('.')
                    .or_else(|| prefix_trimmed.strip_suffix(')'))?;
                (!digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())).then_some(tail)
            });
        if let Some(text) = rest {
            let clean = text.trim();
            if !clean.is_empty() {
                items.push((index + 1, clean.to_owned()));
            }
        }
    }
    items
}

fn recovery_task_prompt(incident: &str, workspace_path: Option<&str>) -> String {
    match workspace_path {
        Some(path) => format!(
            "{PROJECT_CONTEXT_PREAMBLE}\
             Обнаружен сбой ({incident}). Твой код находится в {path}. Используй инструменты \
             `eva_workspace_*`, чтобы найти причину, исправить код, запустить тесты и сделать \
             коммит. Если для исправления нужна платная модель (DeepSeek), запроси подтверждение \
             у пользователя."
        ),
        None => format!(
            "EvaPulse: обнаружен сбой ({incident}). Изучи журнал child.workflow, определи причину \
             и предложи безопасное исправление. Не изменяй файлы без подтверждения."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct FakeProbe {
        report: PulseCycleReport,
        cycles: AtomicUsize,
    }

    impl FakeProbe {
        fn new(report: PulseCycleReport) -> Self {
            Self {
                report,
                cycles: AtomicUsize::new(0),
            }
        }
    }

    impl PulseProbe for FakeProbe {
        fn observe(&self) -> PulseFuture<'_, Result<PulseCycleReport, String>> {
            self.cycles.fetch_add(1, Ordering::SeqCst);
            let report = self.report.clone();
            Box::pin(async move { Ok(report) })
        }
    }

    #[derive(Default)]
    struct CountingSink {
        primary_calls: AtomicUsize,
        evolution_calls: AtomicUsize,
        fallback_calls: AtomicUsize,
    }

    impl PulseTaskSink for CountingSink {
        fn dispatch_recovery(
            &self,
            _failed_workers: &[FailedWorkerContext],
            _eva_workspace_path: Option<&str>,
        ) -> PulseFuture<'_, Result<String, String>> {
            self.primary_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok("task-pulse".into()) })
        }

        fn dispatch_recovery_fallback(
            &self,
            _failed_workers: &[FailedWorkerContext],
            _eva_workspace_path: Option<&str>,
            _fallback_provider: evohime_model_gateway::providers::ProviderKind,
        ) -> PulseFuture<'_, Result<String, String>> {
            self.fallback_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok("task-pulse-routmy".into()) })
        }

        fn dispatch_evolution(
            &self,
            _items: &[RoadmapItem],
            _eva_workspace_path: Option<&str>,
        ) -> PulseFuture<'_, Result<String, String>> {
            self.evolution_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok("task-evolution".into()) })
        }
    }

    fn healthy_report() -> PulseCycleReport {
        PulseCycleReport {
            interval_secs: 1,
            daily_cost_nano_usd: Some(1),
            daily_budget_nano_usd: 1_000_000,
            provider_window_exhausted: false,
            provider_window_wake_at_ms: None,
            rate_limited_until_ms: None,
            failed_workers: Vec::new(),
            failed_worker_contexts: Vec::new(),
            route_is_primary: true,
            eva_workspace_path: None,
            proactive_evolution_enabled: false,
            roadmap_items: Vec::new(),
            selected_provider: Some("literouter".to_owned()),
            fallback_provider: None,
            routmy_tokens_remaining: None,
            quota_reset_utc_midnight: false,
            allow_paid_fallback: false,
            literouter_all_models_exhausted: false,
        }
    }

    fn options() -> EvaPulseOptions {
        EvaPulseOptions {
            interval_override: Some(Duration::from_millis(5)),
        }
    }

    #[tokio::test]
    async fn pulse_respects_budget_exhaustion() {
        let mut report = healthy_report();
        report.daily_budget_nano_usd = 0;
        report.failed_workers = vec!["child-failed".into()];
        let sink = Arc::new(CountingSink::default());
        let daemon = EvaPulseDaemon::spawn(
            Arc::new(FakeProbe::new(report)),
            Arc::clone(&sink) as Arc<dyn PulseTaskSink>,
            options(),
            None,
        );
        daemon.run();
        tokio::time::sleep(Duration::from_millis(50)).await;
        daemon.shutdown();

        assert_eq!(sink.primary_calls.load(Ordering::SeqCst), 0);
        assert_eq!(sink.fallback_calls.load(Ordering::SeqCst), 0);
        let status = daemon.status();
        assert_eq!(status.status, HIVE_PULSE_SLEEPING);
        assert!(status.next_wake_at_ms.is_some());
    }

    #[test]
    fn decide_ignores_zero_usd_budget_when_provider_has_no_monetary_billing() {
        let mut report = healthy_report();
        report.daily_cost_nano_usd = None;
        report.daily_budget_nano_usd = 0;
        report.failed_workers = vec!["child-failed".into()];

        let decision = decide(&report, 1_700_000_000_000);
        assert!(
            matches!(decision, PulseDecision::Recover { .. }),
            "expected Recover for free/quota provider with 0 USD budget, got {decision:?}"
        );
    }

    #[tokio::test]
    async fn background_task_does_not_use_fallback_provider() {
        let mut report = healthy_report();
        report.failed_workers = vec!["child-failed".into()];
        report.rate_limited_until_ms = Some(crate::task_memory::now_millis() as i64 + 60_000);
        let sink = Arc::new(CountingSink::default());
        let daemon = EvaPulseDaemon::spawn(
            Arc::new(FakeProbe::new(report)),
            Arc::clone(&sink) as Arc<dyn PulseTaskSink>,
            options(),
            None,
        );
        daemon.run();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let status = daemon.status();
        daemon.shutdown();

        // A 429 parks the loop: neither the primary nor the paid fallback is
        // contacted again until the rate-limit window ends.
        assert_eq!(sink.primary_calls.load(Ordering::SeqCst), 0);
        assert_eq!(sink.fallback_calls.load(Ordering::SeqCst), 0);
        assert_eq!(status.status, HIVE_PULSE_SLEEPING);
        assert!(status.next_wake_at_ms.is_some());
    }

    #[tokio::test]
    async fn disabled_daemon_runs_no_cycles_until_enabled() {
        let probe = Arc::new(FakeProbe::new(healthy_report()));
        let sink = Arc::new(CountingSink::default());
        let daemon = EvaPulseDaemon::spawn(
            Arc::clone(&probe) as Arc<dyn PulseProbe>,
            Arc::clone(&sink) as Arc<dyn PulseTaskSink>,
            options(),
            None,
        );
        daemon.set_enabled(false);
        daemon.run();
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert_eq!(probe.cycles.load(Ordering::SeqCst), 0);
        assert_eq!(daemon.status().status, HIVE_PULSE_DISABLED);
        daemon.set_enabled(true);
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(daemon.status().enabled);
        assert!(probe.cycles.load(Ordering::SeqCst) >= 1);
        daemon.shutdown();
    }

    #[tokio::test]
    async fn recovery_is_skipped_when_route_is_not_the_primary_literouter() {
        let mut report = healthy_report();
        report.failed_workers = vec!["child-failed".into()];
        report.route_is_primary = false;
        let sink = Arc::new(CountingSink::default());
        let daemon = EvaPulseDaemon::spawn(
            Arc::new(FakeProbe::new(report)),
            Arc::clone(&sink) as Arc<dyn PulseTaskSink>,
            options(),
            None,
        );
        daemon.run();
        tokio::time::sleep(Duration::from_millis(40)).await;
        daemon.shutdown();
        assert_eq!(sink.primary_calls.load(Ordering::SeqCst), 0);
        assert_eq!(sink.fallback_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn recovery_sink_starts_task_with_self_development_instruction() {
        let (coordinator, mut events) = TaskCoordinator::new(8);
        let sink = CoordinatorPulseTaskSink::new(coordinator, Some("primary".into()));
        let task_id = sink
            .dispatch_recovery(
                &[FailedWorkerContext::from_id("failed-worker")],
                Some("D:/eva"),
            )
            .await
            .expect("recovery dispatches");
        let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("task start arrives")
            .expect("event received");
        assert!(matches!(
            &event,
            crate::CoreEvent::TaskStarted { task_id: started, .. } if started == &task_id
        ));
        let prompt = recovery_task_prompt("failed-worker", Some("D:/eva"));
        assert!(prompt.contains("eva_workspace_*"));
        assert!(prompt.contains("D:/eva"));
    }

    #[test]
    fn decision_order_blocks_budget_before_work() {
        let mut report = healthy_report();
        report.failed_workers = vec!["child-1".into()];
        report.daily_cost_nano_usd = Some(2_000_000);
        report.daily_budget_nano_usd = 1_000_000;
        assert!(matches!(
            decide(&report, 1_000),
            PulseDecision::Sleep {
                reason: "daily_budget_exhausted",
                ..
            }
        ));

        report.daily_budget_nano_usd = 1_000_000_000;
        report.provider_window_exhausted = true;
        report.provider_window_wake_at_ms = Some(2_000);
        assert_eq!(
            decide(&report, 1_000),
            PulseDecision::Sleep {
                wake_at_ms: Some(2_000),
                reason: "provider_window_exhausted"
            }
        );

        report.provider_window_exhausted = false;
        assert_eq!(
            decide(&report, 1_000),
            PulseDecision::Recover {
                failed_workers: vec!["child-1".into()]
            }
        );
    }

    #[test]
    fn zero_budget_is_not_unlimited() {
        assert_eq!(usd_to_nano_usd(0.0), 0);
        assert_eq!(usd_to_nano_usd(-1.0), 0);
        assert_eq!(usd_to_nano_usd(f64::NAN), 0);
        assert_eq!(usd_to_nano_usd(0.5), 500_000_000);
    }

    #[test]
    fn pulse_creates_self_dev_task_when_workspace_is_set() {
        let prompt = recovery_task_prompt("worker task-1 завершился с ошибкой", Some("D:/eva"));
        assert!(prompt.contains("Обнаружен сбой"));
        assert!(prompt.contains("D:/eva"));
        assert!(prompt.contains("`eva_workspace_*`"));
        assert!(prompt.contains("запустить тесты и сделать коммит"));
        assert!(prompt.contains("платная модель (DeepSeek)"));
        assert_eq!(
            crate::core_prelude::SELF_DEVELOPMENT_TOOL_IDS,
            [
                evohime_tool_runtime::self_development::TOOL_READ_FILE,
                evohime_tool_runtime::self_development::TOOL_WRITE_FILE,
                evohime_tool_runtime::self_development::TOOL_RUN_COMMAND,
                evohime_tool_runtime::self_development::TOOL_GIT,
            ]
        );
        assert_eq!(
            evohime_tool_runtime::self_development::SELF_DEVELOPMENT_CAPABILITY,
            "eva_self_development"
        );
    }

    #[test]
    fn pulse_creates_investigation_task_when_workspace_is_empty() {
        let prompt = recovery_task_prompt("worker task-1 завершился с ошибкой", None);
        assert!(prompt.contains("Изучи журнал child.workflow"));
        assert!(!prompt.contains("eva_workspace_*"));
        assert!(!prompt.contains("D:/eva"));
    }

    fn failed_projection(
        child_task_id: &str,
        role: &str,
        reason_code: Option<&str>,
    ) -> crate::child_workflow::ChildProjection {
        crate::child_workflow::ChildProjection {
            event_id: format!("evt-{child_task_id}"),
            parent_task_id: "queen-1".into(),
            child_task_id: child_task_id.into(),
            role: role.into(),
            revision: 1,
            state: CoordinatorState::Failed,
            reason_code: reason_code.map(str::to_owned),
            parent_sequence: 1,
            budget: None,
            lease_live: false,
            dead_letter: false,
            depth: 1,
            report_status: None,
            report_summary: None,
            spawned_subagents: 0,
            planned_ceiling: 0,
            extensions_granted: 0,
            lazy_waiting: 0,
            in_flight_subagents: 0,
            extension_requested: false,
        }
    }

    #[tokio::test]
    async fn eva_pulse_passes_error_context_to_recovery_task() {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal =
            EventJournal::open(directory.path().join("pulse-context.db")).expect("journal opens");
        let selected = SelectedModel::default();
        selected.set("primary");
        let probe = JournalPulseProbe::new(journal.clone(), None, selected);
        // Initial baseline scan before the worker failure arrives.
        let _ = probe.observe().await.expect("initial observe");

        journal
            .record(&crate::CoreEvent::ToolStarted {
                task_id: "worker-ctx-1".into(),
                tool_name: "eva_workspace_write_file".into(),
                arguments: r#"{"path":"crates/evohime-core/src/eva_pulse.rs"}"#.into(),
            })
            .await
            .expect("tool event recorded");
        journal
            .record(&crate::CoreEvent::ChildWorkflowProjection {
                task_id: "queen-1".into(),
                projection: failed_projection("worker-ctx-1", "coder", Some("compile_error")),
            })
            .await
            .expect("projection recorded");

        let report = probe.observe().await.expect("observe after failure");
        assert_eq!(report.failed_workers, vec!["worker-ctx-1".to_owned()]);
        assert_eq!(report.failed_worker_contexts.len(), 1);
        assert_eq!(
            report.failed_worker_contexts[0].reason_code.as_deref(),
            Some("compile_error")
        );
        assert_eq!(
            report.failed_worker_contexts[0].last_tool.as_deref(),
            Some("eva_workspace_write_file")
        );
        assert_eq!(
            report.failed_worker_contexts[0].file_path.as_deref(),
            Some("crates/evohime-core/src/eva_pulse.rs")
        );

        let (coordinator, mut events) = TaskCoordinator::new(8);
        let sink = CoordinatorPulseTaskSink::new(coordinator, Some("primary".into()));
        let task_id = sink
            .dispatch_recovery(&report.failed_worker_contexts, Some("D:/eva"))
            .await
            .expect("recovery dispatches");
        let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("task start arrives")
            .expect("event received");
        assert!(matches!(
            &event,
            crate::CoreEvent::TaskStarted { task_id: started, .. } if started == &task_id
        ));
        let prompt = build_recovery_prompt(&report.failed_worker_contexts, Some("D:/eva"));
        assert!(prompt.contains("worker-ctx-1"));
        assert!(prompt.contains("compile_error"));
        assert!(prompt.contains("eva_workspace_write_file"));
        assert!(prompt.contains("crates/evohime-core/src/eva_pulse.rs"));
        assert!(prompt.contains("D:/eva"));
    }

    #[tokio::test]
    async fn eva_pulse_retries_incident_after_recovery_429_cooldown() {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal =
            EventJournal::open(directory.path().join("pulse-429.db")).expect("journal opens");
        let model = "pulse-429-test-model";
        evohime_model_gateway::clear_rate_limit(model);
        let selected = SelectedModel::default();
        selected.set(model);
        let gateway_config = Some(ModelGatewayConfig {
            default_route: "primary".into(),
            routes: std::collections::HashMap::from([(
                "hive".into(),
                evohime_model_gateway::ModelRouteConfig::literouter(
                    "hive-test-key",
                    "http://localhost:1234",
                    model,
                ),
            )]),
        });
        let probe = JournalPulseProbe::new(journal.clone(), gateway_config, selected);
        let _ = probe.observe().await.expect("initial observe");

        journal
            .record(&crate::CoreEvent::ChildWorkflowProjection {
                task_id: "queen-1".into(),
                projection: failed_projection("worker-429-1", "coder", Some("worker_failed")),
            })
            .await
            .expect("projection recorded");

        // Cycle 1: incident detected and recovery-1 is dispatched.
        let report_1 = probe.observe().await.expect("observe 1");
        assert_eq!(report_1.failed_workers, vec!["worker-429-1".to_owned()]);
        probe.mark_recovery_dispatched("recovery-1", &report_1.failed_workers);

        // Background recovery-1 hits 429 and fails, recording rate-limit cooldown.
        let wake_at = crate::task_memory::now_millis() as i64 + 60_000;
        evohime_model_gateway::record_rate_limit(model, wake_at);
        journal
            .record(&crate::CoreEvent::TaskFailed {
                task_id: "recovery-1".into(),
                error: "429 Too Many Requests: provider_rate_limited".into(),
            })
            .await
            .expect("task failure recorded");

        // Cycle 2 (during cooldown): incident is NOT lost; daemon sleeps until wake_at_ms.
        let report_2 = probe.observe().await.expect("observe 2");
        assert_eq!(report_2.failed_workers, vec!["worker-429-1".to_owned()]);
        assert!(report_2.rate_limited_until_ms.is_some());
        let decision_2 = decide(&report_2, crate::task_memory::now_millis() as i64);
        assert!(matches!(
            decision_2,
            PulseDecision::Sleep {
                wake_at_ms: Some(_),
                reason: "provider_rate_limited"
            }
        ));

        // Cycle 3 (after cooldown expires): incident is retried.
        evohime_model_gateway::clear_rate_limit(model);
        let report_3 = probe.observe().await.expect("observe 3");
        assert_eq!(
            decide(&report_3, crate::task_memory::now_millis() as i64),
            PulseDecision::Recover {
                failed_workers: vec!["worker-429-1".to_owned()]
            }
        );

        // Once recovery-2 completes, the incident is retired and not repeated.
        probe.mark_recovery_dispatched("recovery-2", &report_3.failed_workers);
        journal
            .record(&crate::CoreEvent::TaskCompleted {
                task_id: "recovery-2".into(),
                final_message: "fixed".into(),
            })
            .await
            .expect("task completed recorded");
        let report_4 = probe.observe().await.expect("observe 4");
        assert_eq!(
            decide(&report_4, crate::task_memory::now_millis() as i64),
            PulseDecision::AllGood
        );
    }

    fn init_git_repo(dir: &std::path::Path) {
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir)
            .status()
            .expect("git init runs");
        assert!(status.success());
    }

    #[tokio::test]
    async fn pulse_creates_proactive_task_from_roadmap() {
        let directory = tempfile::tempdir().expect("temp dir");
        let repo_dir = directory.path().join("eva-repo");
        std::fs::create_dir_all(repo_dir.join("docs")).expect("docs dir");
        init_git_repo(&repo_dir);
        std::fs::write(
            repo_dir.join("docs/development-plan.md"),
            "# Plan\n- [x] Already done\n- [ ] Add proactive metrics endpoint\n",
        )
        .expect("write roadmap");

        let journal =
            EventJournal::open(directory.path().join("pulse-evolve.db")).expect("journal opens");
        let settings = HiveSettings {
            watched_repositories: vec![repo_dir.to_string_lossy().into_owned()],
            proactive_evolution_enabled: true,
            ..HiveSettings::default()
        };
        journal
            .save_hive_settings(&settings)
            .await
            .expect("save settings");

        let selected = SelectedModel::default();
        selected.set("primary");
        let gateway_config = Some(ModelGatewayConfig {
            default_route: "primary".into(),
            routes: std::collections::HashMap::from([(
                "hive".into(),
                evohime_model_gateway::ModelRouteConfig::literouter(
                    "hive-test-key",
                    "http://localhost:1234",
                    "hive-model",
                ),
            )]),
        });
        let probe = Arc::new(JournalPulseProbe::new(
            journal.clone(),
            gateway_config,
            selected,
        ));
        let (coordinator, mut events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let sink = Arc::new(CoordinatorPulseTaskSink::new(
            coordinator,
            Some("hive".into()),
        ));

        let report_1 = probe.observe().await.expect("observe 1");
        assert_eq!(
            decide(&report_1, crate::task_memory::now_millis() as i64),
            PulseDecision::Evolve {
                items: vec![RoadmapItem {
                    file_path: "docs/development-plan.md".into(),
                    line: 3,
                    text: "Add proactive metrics endpoint".into(),
                }],
            }
        );

        let daemon = EvaPulseDaemon::spawn(
            Arc::clone(&probe) as Arc<dyn PulseProbe>,
            Arc::clone(&sink) as Arc<dyn PulseTaskSink>,
            options(),
            None,
        );
        daemon.run();

        let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("evolution task starts")
            .expect("event received");
        let dispatched_task_id = match &event {
            crate::CoreEvent::TaskStarted { task_id, .. } => task_id.clone(),
            other => panic!("unexpected event: {other:?}"),
        };

        // Allow a couple more cycles to run while the task is in-flight and after failure.
        tokio::time::sleep(Duration::from_millis(30)).await;
        journal
            .record(&crate::CoreEvent::TaskFailed {
                task_id: dispatched_task_id,
                error: "simulated failure".into(),
            })
            .await
            .expect("record failure");
        tokio::time::sleep(Duration::from_millis(30)).await;
        daemon.shutdown();

        // Anti-loop protection: the same item must not be re-dispatched after failure.
        assert!(events.try_recv().is_err());

        let prompt = proactive_evolution_prompt(&[RoadmapItem {
            file_path: "docs/development-plan.md".into(),
            line: 3,
            text: "Add proactive metrics endpoint".into(),
        }]);
        assert!(prompt.contains("Add proactive metrics endpoint"));
        assert!(prompt.contains("docs/development-plan.md:3"));
        assert!(prompt.contains("eva_workspace_write_file"));
        assert!(prompt.contains("[x]"));
    }

    #[tokio::test]
    async fn pulse_ignores_roadmap_if_disabled() {
        let directory = tempfile::tempdir().expect("temp dir");
        let repo_dir = directory.path().join("eva-repo");
        std::fs::create_dir_all(repo_dir.join("docs")).expect("docs dir");
        init_git_repo(&repo_dir);
        std::fs::write(
            repo_dir.join("docs/development-plan.md"),
            "- [ ] Uncompleted item\n",
        )
        .expect("write roadmap");

        let journal =
            EventJournal::open(directory.path().join("pulse-disabled.db")).expect("journal opens");
        let settings = HiveSettings {
            watched_repositories: vec![repo_dir.to_string_lossy().into_owned()],
            proactive_evolution_enabled: false,
            ..HiveSettings::default()
        };
        journal
            .save_hive_settings(&settings)
            .await
            .expect("save settings");

        let probe = JournalPulseProbe::new(journal, None, SelectedModel::default());
        let report = probe.observe().await.expect("observe");
        assert!(!report.proactive_evolution_enabled);
        assert!(report.roadmap_items.is_empty());
        assert_eq!(
            decide(&report, crate::task_memory::now_millis() as i64),
            PulseDecision::AllGood
        );

        let mut forced_report = healthy_report();
        forced_report.proactive_evolution_enabled = false;
        forced_report.eva_workspace_path = Some("D:/eva".into());
        forced_report.roadmap_items = vec![RoadmapItem {
            file_path: "docs/development-plan.md".into(),
            line: 1,
            text: "Uncompleted item".into(),
        }];
        assert_eq!(decide(&forced_report, 1_000), PulseDecision::AllGood);
    }

    #[tokio::test]
    async fn pulse_finds_roadmap_automatically() {
        let directory = tempfile::tempdir().expect("temp dir");
        let repo_dir = directory.path().join("eva-repo");
        std::fs::create_dir_all(&repo_dir).expect("repo dir");
        init_git_repo(&repo_dir);
        std::fs::write(repo_dir.join("TODO.md"), "# TODO\n- [ ] Fix bug\n").expect("write TODO");

        let discovered = find_roadmap_files(&repo_dir);
        assert_eq!(discovered.len(), 1);
        assert!(discovered[0].ends_with("TODO.md"));

        let journal =
            EventJournal::open(directory.path().join("pulse-auto.db")).expect("journal opens");
        let settings = HiveSettings {
            watched_repositories: vec![repo_dir.to_string_lossy().into_owned()],
            proactive_evolution_enabled: true,
            ..HiveSettings::default()
        };
        journal
            .save_hive_settings(&settings)
            .await
            .expect("save settings");

        let selected = SelectedModel::default();
        selected.set("primary");
        let gateway_config = Some(ModelGatewayConfig {
            default_route: "primary".into(),
            routes: std::collections::HashMap::from([(
                "hive".into(),
                evohime_model_gateway::ModelRouteConfig::literouter(
                    "hive-test-key",
                    "http://localhost:1234",
                    "hive-model",
                ),
            )]),
        });
        let probe = Arc::new(JournalPulseProbe::new(
            journal.clone(),
            gateway_config,
            selected,
        ));
        let (coordinator, mut events) = TaskCoordinator::new_with_journal(8, None, journal);
        let sink = Arc::new(CoordinatorPulseTaskSink::new(
            coordinator,
            Some("hive".into()),
        ));

        let report = probe.observe().await.expect("observe");
        assert_eq!(
            decide(&report, crate::task_memory::now_millis() as i64),
            PulseDecision::Evolve {
                items: vec![RoadmapItem {
                    file_path: "TODO.md".into(),
                    line: 2,
                    text: "Fix bug".into(),
                }],
            }
        );

        let daemon = EvaPulseDaemon::spawn(
            Arc::clone(&probe) as Arc<dyn PulseProbe>,
            Arc::clone(&sink) as Arc<dyn PulseTaskSink>,
            options(),
            None,
        );
        daemon.run();

        let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("evolution task starts")
            .expect("event received");
        assert!(matches!(event, crate::CoreEvent::TaskStarted { .. }));
        daemon.shutdown();
    }

    #[tokio::test]
    async fn pulse_ignores_roadmap_if_no_todo() {
        let directory = tempfile::tempdir().expect("temp dir");
        let repo_dir = directory.path().join("eva-repo");
        std::fs::create_dir_all(repo_dir.join("docs/plans")).expect("plans dir");
        init_git_repo(&repo_dir);
        std::fs::write(repo_dir.join("TODO.md"), "# TODO\n- [x] Done\n").expect("write TODO");
        std::fs::write(
            repo_dir.join("docs/plans/01.md"),
            "# Plan\nNo checkboxes here.\n",
        )
        .expect("write plan");

        let journal =
            EventJournal::open(directory.path().join("pulse-empty.db")).expect("journal opens");
        let settings = HiveSettings {
            watched_repositories: vec![repo_dir.to_string_lossy().into_owned()],
            proactive_evolution_enabled: true,
            ..HiveSettings::default()
        };
        journal
            .save_hive_settings(&settings)
            .await
            .expect("save settings");

        let probe = JournalPulseProbe::new(journal, None, SelectedModel::default());
        let report = probe.observe().await.expect("observe");
        assert!(report.roadmap_items.is_empty());
        assert_eq!(
            decide(&report, crate::task_memory::now_millis() as i64),
            PulseDecision::AllGood
        );
    }

    #[test]
    fn pulse_includes_project_context_in_prompt() {
        let evolution = proactive_evolution_prompt(&[RoadmapItem {
            file_path: "docs/development-plan.md".into(),
            line: 1,
            text: "Add metrics".into(),
        }]);
        let recovery = recovery_task_prompt("worker crashed", Some("D:/eva"));

        assert!(recovery.starts_with("ПЕРЕД ТЕМ КАК ПИСАТЬ КОД"));
        assert!(recovery.contains("eva_workspace_read_file"));
        for prompt in [&evolution, &recovery] {
            assert!(prompt.contains("AGENTS.md"));
            assert!(prompt.contains("docs/architecture.md"));
            assert!(prompt.contains("git log -n 5"));
        }
        assert!(evolution.contains("eva_workspace_*"));
    }

    #[tokio::test]
    async fn pulse_collects_all_roadmap_items() {
        let directory = tempfile::tempdir().expect("temp dir");
        let repo_dir = directory.path().join("eva-repo");
        std::fs::create_dir_all(repo_dir.join("docs/plans")).expect("plans dir");
        init_git_repo(&repo_dir);
        std::fs::write(
            repo_dir.join("docs/development-plan.md"),
            "# Plan\n- [x] Done item\n- [ ] First task\n",
        )
        .expect("write dev plan");
        std::fs::write(
            repo_dir.join("docs/plans/01-stage.md"),
            "# Stage 1\n* [ ] Second task\n",
        )
        .expect("write stage 1");

        let mut todo_lines = String::from("# TODO\n");
        for idx in 1..=35 {
            todo_lines.push_str(&format!("- [ ] Todo task {idx}\n"));
        }
        std::fs::write(repo_dir.join("TODO.md"), todo_lines).expect("write TODO");

        let journal =
            EventJournal::open(directory.path().join("pulse-collect.db")).expect("journal opens");
        let settings = HiveSettings {
            watched_repositories: vec![repo_dir.to_string_lossy().into_owned()],
            proactive_evolution_enabled: true,
            ..HiveSettings::default()
        };
        journal
            .save_hive_settings(&settings)
            .await
            .expect("save settings");

        let probe = JournalPulseProbe::new(journal, None, SelectedModel::default());
        let report = probe.observe().await.expect("observe");

        assert_eq!(report.roadmap_items.len(), MAX_ROADMAP_ITEMS);
        assert_eq!(
            report.roadmap_items[0],
            RoadmapItem {
                file_path: "docs/development-plan.md".into(),
                line: 3,
                text: "First task".into(),
            }
        );
        assert_eq!(
            report.roadmap_items[1],
            RoadmapItem {
                file_path: "docs/plans/01-stage.md".into(),
                line: 2,
                text: "Second task".into(),
            }
        );
        assert_eq!(
            report.roadmap_items[2],
            RoadmapItem {
                file_path: "TODO.md".into(),
                line: 2,
                text: "Todo task 1".into(),
            }
        );
        assert_eq!(
            report.roadmap_items[29],
            RoadmapItem {
                file_path: "TODO.md".into(),
                line: 29,
                text: "Todo task 28".into(),
            }
        );
    }

    #[test]
    fn pulse_prompt_asks_queen_to_choose() {
        let items = vec![
            RoadmapItem {
                file_path: "docs/development-plan.md".into(),
                line: 4,
                text: "Implement pulse task selection".into(),
            },
            RoadmapItem {
                file_path: "TODO.md".into(),
                line: 2,
                text: "Refactor logging".into(),
            },
        ];
        let prompt = proactive_evolution_prompt(&items);

        assert!(prompt.contains("Ты — Королева Ева. Ты работаешь в автономном режиме."));
        assert!(prompt.contains("1. [docs/development-plan.md:4] Implement pulse task selection"));
        assert!(prompt.contains("2. [TODO.md:2] Refactor logging"));
        assert!(prompt.contains("Выбери РОВНО ОДНУ задачу"));
        assert!(prompt.contains("верни ответ \"SKIP\""));
        assert!(prompt.contains("eva_workspace_write_file"));
        assert!(prompt.contains("FAILED: [причина]"));
    }

    #[tokio::test]
    async fn pulse_blocks_failed_and_skipped_items_without_dropping_others() {
        let directory = tempfile::tempdir().expect("temp dir");
        let repo_dir = directory.path().join("eva-repo");
        std::fs::create_dir_all(repo_dir.join("docs")).expect("docs dir");
        init_git_repo(&repo_dir);
        std::fs::write(
            repo_dir.join("TODO.md"),
            "- [ ] Easy task\n- [ ] Hard task\n",
        )
        .expect("write TODO");

        let journal = EventJournal::open(directory.path().join("pulse-loop-guard.db"))
            .expect("journal opens");
        let settings = HiveSettings {
            watched_repositories: vec![repo_dir.to_string_lossy().into_owned()],
            proactive_evolution_enabled: true,
            ..HiveSettings::default()
        };
        journal
            .save_hive_settings(&settings)
            .await
            .expect("save settings");

        let probe = JournalPulseProbe::new(journal.clone(), None, SelectedModel::default());
        let report_1 = probe.observe().await.expect("observe 1");
        assert_eq!(report_1.roadmap_items.len(), 2);

        probe.mark_evolution_dispatched(
            "evo-1",
            settings.primary_workspace_path(),
            &report_1.roadmap_items,
        );
        // While in-flight, no new evolution items are offered.
        let report_inflight = probe.observe().await.expect("observe inflight");
        assert!(report_inflight.roadmap_items.is_empty());

        // Queen reports failure on "Hard task": only "Hard task" is blocked, "Easy task" stays eligible.
        journal
            .record(&crate::CoreEvent::TaskCompleted {
                task_id: "evo-1".into(),
                final_message: "FAILED: Hard task requires unavailable credentials".into(),
            })
            .await
            .expect("record failed message");

        let report_2 = probe.observe().await.expect("observe 2");
        assert_eq!(report_2.roadmap_items.len(), 1);
        assert_eq!(report_2.roadmap_items[0].text, "Easy task");

        // Queen returns SKIP for remaining item: it is also added to handled_evolutions.
        probe.mark_evolution_dispatched(
            "evo-2",
            settings.primary_workspace_path(),
            &report_2.roadmap_items,
        );
        journal
            .record(&crate::CoreEvent::TaskCompleted {
                task_id: "evo-2".into(),
                final_message: "SKIP".into(),
            })
            .await
            .expect("record skip message");

        let report_3 = probe.observe().await.expect("observe 3");
        assert!(report_3.roadmap_items.is_empty());
    }

    struct ExhaustedLiteRouterSink {
        primary_calls: AtomicUsize,
        fallback_calls: AtomicUsize,
    }

    impl Default for ExhaustedLiteRouterSink {
        fn default() -> Self {
            Self {
                primary_calls: AtomicUsize::new(0),
                fallback_calls: AtomicUsize::new(0),
            }
        }
    }

    impl PulseTaskSink for ExhaustedLiteRouterSink {
        fn dispatch_recovery(
            &self,
            _failed_workers: &[FailedWorkerContext],
            _eva_workspace_path: Option<&str>,
        ) -> PulseFuture<'_, Result<String, String>> {
            self.primary_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Err("all_models_exhausted".to_owned()) })
        }

        fn dispatch_recovery_fallback(
            &self,
            _failed_workers: &[FailedWorkerContext],
            _eva_workspace_path: Option<&str>,
            _fallback_provider: evohime_model_gateway::providers::ProviderKind,
        ) -> PulseFuture<'_, Result<String, String>> {
            self.fallback_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok("task-routmy-fallback".to_owned()) })
        }

        fn dispatch_evolution(
            &self,
            _items: &[RoadmapItem],
            _eva_workspace_path: Option<&str>,
        ) -> PulseFuture<'_, Result<String, String>> {
            Box::pin(async move { Ok("task-evo".to_owned()) })
        }
    }

    #[tokio::test]
    async fn eva_pulse_uses_literouter_first() {
        let mut report = healthy_report();
        report.selected_provider = Some("literouter".to_owned());
        report.fallback_provider =
            Some(evohime_model_gateway::providers::ProviderKind::OpenAICompatible);
        report.routmy_tokens_remaining = Some(650_000);
        report.failed_workers = vec!["child-failed".into()];

        let sink = Arc::new(CountingSink::default());
        let daemon = EvaPulseDaemon::spawn(
            Arc::new(FakeProbe::new(report)),
            sink.clone(),
            options(),
            None,
        );
        daemon.run();

        tokio::time::timeout(Duration::from_secs(2), async {
            while daemon.cycle_count() == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("cycle completes");
        daemon.shutdown();

        assert!(sink.primary_calls.load(Ordering::SeqCst) >= 1);
        assert_eq!(sink.fallback_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn eva_pulse_falls_back_to_rout_when_literouter_exhausted() {
        let mut report = healthy_report();
        report.selected_provider = Some("literouter".to_owned());
        report.fallback_provider =
            Some(evohime_model_gateway::providers::ProviderKind::OpenAICompatible);
        report.routmy_tokens_remaining = Some(650_000);
        report.failed_workers = vec!["child-failed".into()];

        // Pure decision must not sleep when LiteRouter is exhausted and rout.my has tokens.
        let mut decide_report = report.clone();
        decide_report.literouter_all_models_exhausted = true;
        assert_eq!(
            decide(&decide_report, 10_000),
            PulseDecision::Recover {
                failed_workers: vec!["child-failed".into()],
            }
        );

        let sink = Arc::new(ExhaustedLiteRouterSink::default());
        let daemon = EvaPulseDaemon::spawn(
            Arc::new(FakeProbe::new(report)),
            sink.clone(),
            options(),
            None,
        );
        daemon.run();

        tokio::time::timeout(Duration::from_secs(2), async {
            while daemon.cycle_count() == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("cycle completes");
        daemon.shutdown();

        assert!(sink.primary_calls.load(Ordering::SeqCst) >= 1);
        assert!(sink.fallback_calls.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn eva_pulse_sleeps_when_routmy_quota_exhausted() {
        let now_ms = 1_700_000_000_000_i64;
        let mut report = healthy_report();
        report.selected_provider = Some("rout".to_owned());
        report.routmy_tokens_remaining = Some(500);
        report.allow_paid_fallback = false;
        report.failed_workers = vec!["child-failed".into()];

        assert_eq!(
            decide(&report, now_ms),
            PulseDecision::Sleep {
                wake_at_ms: Some(quota_wake_at(now_ms, report.quota_reset_utc_midnight)),
                reason: "routmy_quota_exhausted",
            }
        );

        // Also when LiteRouter is exhausted and fallback rout.my has < 1000 tokens.
        let mut fallback_exhausted = healthy_report();
        fallback_exhausted.selected_provider = Some("literouter".to_owned());
        fallback_exhausted.fallback_provider =
            Some(evohime_model_gateway::providers::ProviderKind::OpenAICompatible);
        fallback_exhausted.routmy_tokens_remaining = Some(999);
        fallback_exhausted.literouter_all_models_exhausted = true;
        fallback_exhausted.allow_paid_fallback = false;
        fallback_exhausted.failed_workers = vec!["child-failed".into()];

        assert_eq!(
            decide(&fallback_exhausted, now_ms),
            PulseDecision::Sleep {
                wake_at_ms: Some(quota_wake_at(
                    now_ms,
                    fallback_exhausted.quota_reset_utc_midnight,
                )),
                reason: "routmy_quota_exhausted",
            }
        );

        let sink = Arc::new(CountingSink::default());
        let daemon = EvaPulseDaemon::spawn(
            Arc::new(FakeProbe::new(report)),
            sink.clone(),
            options(),
            None,
        );
        daemon.run();

        tokio::time::timeout(Duration::from_secs(2), async {
            while daemon.cycle_count() == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("cycle completes");
        let status = daemon.status();
        daemon.shutdown();

        assert_eq!(status.status, HIVE_PULSE_SLEEPING);
        assert_eq!(sink.primary_calls.load(Ordering::SeqCst), 0);
        assert_eq!(sink.fallback_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn hive_scans_all_watched_repositories() {
        let directory = tempfile::tempdir().expect("temp dir");
        let repo_one = directory.path().join("repo-one");
        let repo_two = directory.path().join("repo-two");
        std::fs::create_dir_all(&repo_one).expect("repo one dir");
        std::fs::create_dir_all(&repo_two).expect("repo two dir");
        for repo in [&repo_one, &repo_two] {
            let status = std::process::Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(repo)
                .status()
                .expect("git init");
            assert!(status.success());
        }
        std::fs::write(repo_one.join("TODO.md"), "- [ ] Task from repo one\n")
            .expect("write repo one TODO");
        std::fs::write(repo_two.join("TODO.md"), "- [ ] Task from repo two\n")
            .expect("write repo two TODO");

        let journal = EventJournal::open(directory.path().join("pulse-multi-repo.db"))
            .expect("journal opens");
        let settings = HiveSettings {
            watched_repositories: vec![
                repo_one.to_string_lossy().into_owned(),
                repo_two.to_string_lossy().into_owned(),
            ],
            proactive_evolution_enabled: true,
            ..HiveSettings::default()
        };
        journal
            .save_hive_settings(&settings)
            .await
            .expect("save settings");

        let probe = JournalPulseProbe::new(journal, None, SelectedModel::default());
        let report = probe.observe().await.expect("observe");
        let texts: Vec<&str> = report
            .roadmap_items
            .iter()
            .map(|item| item.text.as_str())
            .collect();
        assert_eq!(texts, vec!["Task from repo one", "Task from repo two"]);
    }

    #[tokio::test]
    async fn hive_never_uses_global_providers_when_unconfigured() {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal = EventJournal::open(directory.path().join("pulse-unconfigured-hive.db"))
            .expect("journal opens");
        let settings = HiveSettings {
            hive_provider_profile: Some("literouter".into()),
            hive_model: Some("hive-special-model".into()),
            ..HiveSettings::default()
        };
        journal
            .save_hive_settings(&settings)
            .await
            .expect("save settings");

        let literouter_route = evohime_model_gateway::ModelRouteConfig::literouter(
            "global-free-key",
            "http://localhost:1234",
            "free-default",
        );
        let rout_route = evohime_model_gateway::ModelRouteConfig::openai_compatible(
            "global-rout-key",
            "https://api.rout.my/v1",
            "",
        )
        .with_provider_profile(evohime_model_gateway::ProviderProfileId::Rout, None);

        let gateway_config = ModelGatewayConfig {
            default_route: "default".into(),
            routes: std::collections::HashMap::from([
                ("default".into(), literouter_route),
                ("rout".into(), rout_route),
            ]),
        };

        let selected = SelectedModel::default();
        selected.set("free-default");
        let probe = JournalPulseProbe::new(journal.clone(), Some(gateway_config.clone()), selected);
        let report = probe.observe().await.expect("observe");
        assert!(!report.route_is_primary);
        assert!(report.selected_provider.is_none());
        assert!(report.fallback_provider.is_none());

        let (mut coordinator, _events) = TaskCoordinator::new(8);
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        coordinator.set_command_sender_for_test(tx);
        let sink = CoordinatorPulseTaskSink::new(coordinator, None)
            .with_hive_settings_source(journal, Some(gateway_config));
        let err = sink
            .dispatch_recovery(&[], None)
            .await
            .expect_err("unconfigured hive must not dispatch on global providers");
        assert_eq!(err, "primary_route_unavailable");
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn coordinator_pulse_sink_prefers_isolated_hive_route() {
        let dir = tempfile::tempdir().expect("tempdir");
        let journal =
            crate::EventJournal::open(dir.path().join("pulse-isolated-hive.db")).expect("journal");

        let deepseek_route = evohime_model_gateway::ModelRouteConfig::openai_compatible(
            "sk-paid",
            "https://api.deepseek.com/v1",
            "deepseek-chat",
        )
        .with_provider_profile(evohime_model_gateway::ProviderProfileId::DeepSeek, None);
        let hive_route = evohime_model_gateway::ModelRouteConfig::openai_compatible(
            "sk-hive",
            "https://api.rout.my/v1",
            "",
        )
        .with_provider_profile(evohime_model_gateway::ProviderProfileId::Rout, None);
        let gateway_config = evohime_model_gateway::ModelGatewayConfig {
            default_route: "deepseek".into(),
            routes: std::collections::HashMap::from([
                ("deepseek".into(), deepseek_route),
                ("hive".into(), hive_route),
            ]),
        };

        let (mut coordinator, _events) = TaskCoordinator::new(8);
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        coordinator.set_command_sender_for_test(tx);
        let sink = CoordinatorPulseTaskSink::new(coordinator, None)
            .with_hive_settings_source(journal, Some(gateway_config));
        let task_id = sink
            .dispatch_recovery(&[], None)
            .await
            .expect("dispatches with isolated hive route");
        assert!(!task_id.is_empty());
        let command = rx.recv().await.expect("command sent");
        match command {
            CoreCommand::StartTask {
                preferred_route_hint,
                ..
            } => {
                assert_eq!(preferred_route_hint.as_deref(), Some("hive"));
            }
            _ => panic!("unexpected command sent to coordinator"),
        }
    }
}
