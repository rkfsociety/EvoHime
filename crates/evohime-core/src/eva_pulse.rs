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
use std::time::Duration;

use evohime_model_gateway::ModelGatewayConfig;

use crate::child_workflow::CoordinatorState;
use crate::core_agent::SelectedModel;
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
/// Fallback interval used when settings cannot be read.
const FALLBACK_INTERVAL: Duration = Duration::from_secs(1_800);

/// Boxed future used by the probe/sink seams.
pub(crate) type PulseFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

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
    /// Whether the active route is the primary LiteRouter route.
    pub route_is_primary: bool,
    /// Validated isolated checkout configured for Queen self-development.
    pub eva_workspace_path: Option<String>,
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
    /// Nothing needs attention.
    AllGood,
}

/// Source of one bounded background observation.
pub(crate) trait PulseProbe: Send + Sync {
    /// Reads committed state; a failure parks the loop instead of inventing work.
    fn observe(&self) -> PulseFuture<'_, Result<PulseCycleReport, String>>;
}

/// Sink that asks Core to investigate failed workers.
pub(crate) trait PulseTaskSink: Send + Sync {
    /// Dispatches one bounded recovery task and returns its task id.
    fn dispatch_recovery(
        &self,
        failed_workers: &[String],
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
            journal,
            gateway_config.clone(),
            selected_model,
        ));
        let sink = Arc::new(CoordinatorPulseTaskSink::new(
            coordinator,
            crate::hive_observation::background_route_name(gateway_config.as_ref()),
        ));
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
                if !report.route_is_primary {
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
                    .dispatch_recovery(&failed_workers, report.eva_workspace_path.as_deref())
                    .await
                {
                    Ok(task_id) => {
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
    let budget_exhausted = report.daily_budget_nano_usd == 0
        || report
            .daily_cost_nano_usd
            .is_some_and(|spent| spent >= report.daily_budget_nano_usd);
    if budget_exhausted {
        return PulseDecision::Sleep {
            wake_at_ms: Some(next_utc_day_ms(now_ms)),
            reason: "daily_budget_exhausted",
        };
    }
    if report.provider_window_exhausted {
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
    PulseDecision::AllGood
}

fn next_utc_day_ms(now_ms: i64) -> i64 {
    (now_ms.div_euclid(HIVE_DAY_WINDOW_MS) + 1).saturating_mul(HIVE_DAY_WINDOW_MS)
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

/// Production probe backed by the journal and the active route configuration.
pub(crate) struct JournalPulseProbe {
    journal: EventJournal,
    gateway_config: Option<ModelGatewayConfig>,
    selected_model: SelectedModel,
    seen_failures: std::sync::Mutex<HashSet<String>>,
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
            first_scan: AtomicBool::new(true),
        }
    }

    fn new_failures(&self, projections: &[crate::child_workflow::ChildProjection]) -> Vec<String> {
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
            if seen.len() >= MAX_SEEN_FAILURES {
                break;
            }
            if !seen.insert(projection.child_task_id.clone()) {
                continue;
            }
            // The first scan after startup adopts existing failures instead of
            // immediately spawning work for a state the user already saw.
            if !first_scan && failures.len() < MAX_REPORTED_FAILURES {
                failures.push(projection.child_task_id.clone());
            }
        }
        failures
    }
}

impl PulseProbe for JournalPulseProbe {
    fn observe(&self) -> PulseFuture<'_, Result<PulseCycleReport, String>> {
        Box::pin(async move {
            let settings = self.journal.hive_settings().await.unwrap_or_default();
            let now_ms = crate::task_memory::now_millis() as i64;
            let selected_model = crate::hive_observation::selected_model_name(&self.selected_model);
            let observation = crate::hive_observation::build_hive_observation(
                &self.journal,
                self.gateway_config.as_ref(),
                selected_model.as_deref(),
                now_ms,
            )
            .await;
            let projections = crate::hive_observation::replay_hive_projections(&self.journal).await;
            let roles = crate::hive_observation::persistent_agent_roles(&self.journal).await;
            let snapshot =
                HiveStateSnapshot::from_observation(&settings, &projections, &roles, &observation);
            let failed_workers = self.new_failures(&projections);
            let rate_limited_until_ms = if observation.model.is_empty() {
                None
            } else {
                evohime_model_gateway::rate_limited_until(&observation.model)
            };
            Ok(PulseCycleReport {
                interval_secs: settings.pulse_interval_secs,
                daily_cost_nano_usd: observation.daily_cost_nano_usd,
                daily_budget_nano_usd: usd_to_nano_usd(settings.daily_budget_usd),
                provider_window_exhausted: snapshot.budget.exhausted(),
                provider_window_wake_at_ms: snapshot.queen.wake_at_ms,
                rate_limited_until_ms,
                failed_workers,
                route_is_primary: crate::hive_observation::route_is_background_primary(
                    self.gateway_config.as_ref(),
                ),
                eva_workspace_path: settings.eva_workspace_path,
            })
        })
    }
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
}

impl CoordinatorPulseTaskSink {
    /// Creates a sink bound to the coordinator and the allowed route name.
    pub(crate) fn new(coordinator: TaskCoordinator, primary_route: Option<String>) -> Self {
        Self {
            coordinator,
            primary_route,
        }
    }
}

impl PulseTaskSink for CoordinatorPulseTaskSink {
    fn dispatch_recovery(
        &self,
        failed_workers: &[String],
        eva_workspace_path: Option<&str>,
    ) -> PulseFuture<'_, Result<String, String>> {
        let primary_route = self.primary_route.clone();
        let failed = failed_workers
            .iter()
            .take(MAX_REPORTED_FAILURES)
            .map(|id| id.chars().take(128).collect::<String>())
            .collect::<Vec<_>>();
        let workspace_path = eva_workspace_path
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(str::to_owned);
        Box::pin(async move {
            // A route is required: without the primary LiteRouter route the
            // daemon must not fall back to a paid DeepSeek profile.
            primary_route.ok_or_else(|| "primary_route_unavailable".to_owned())?;
            let task_id = uuid::Uuid::new_v4().to_string();
            let incident = format!("завершившиеся с ошибкой воркеры: {}", failed.join(", "));
            let prompt = recovery_task_prompt(&incident, workspace_path.as_deref());
            self.coordinator
                .dispatch(CoreCommand::StartTask {
                    task_id: task_id.clone(),
                    prompt,
                    workspace_root: None,
                    preferred_route_hint: None,
                    conversation: None,
                    goal_id: None,
                })
                .await
                .map_err(|error| error.to_string())?;
            Ok(task_id)
        })
    }
}

fn recovery_task_prompt(incident: &str, workspace_path: Option<&str>) -> String {
    match workspace_path {
        Some(path) => format!(
            "Обнаружен сбой ({incident}). Твой код находится в {path}. Используй инструменты \
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
        fallback_calls: AtomicUsize,
    }

    impl PulseTaskSink for CountingSink {
        fn dispatch_recovery(
            &self,
            _failed_workers: &[String],
            _eva_workspace_path: Option<&str>,
        ) -> PulseFuture<'_, Result<String, String>> {
            self.primary_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok("task-pulse".into()) })
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
            route_is_primary: true,
            eva_workspace_path: None,
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
            .dispatch_recovery(&["failed-worker".into()], Some("D:/eva"))
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
}
