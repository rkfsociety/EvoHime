//! Standalone entry point for the `evohime-hive-daemon` process.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use std::sync::Arc;

use evohime_hive::pulse::EvaPulseDaemon;
use evohime_hive_contract::{
    BoxFuture, HiveContractError, HiveCoordinator, HiveDispatchWindows, HiveEventRecord,
    HiveJournalProvider, HiveModelUsage, HiveTaskDispatchRequest, HiveWorktreeLeaseRequest,
    WorkspaceProvider,
};

struct StubJournalProvider;

impl HiveJournalProvider for StubJournalProvider {
    fn latest_sequence(&self) -> BoxFuture<'_, i64> {
        Box::pin(async { 0 })
    }

    fn replay_events(
        &self,
        _after_sequence: i64,
        _limit: usize,
    ) -> BoxFuture<'_, Result<Vec<HiveEventRecord>, HiveContractError>> {
        Box::pin(async { Ok(Vec::new()) })
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

    fn filter_hive_root_tasks(&self, _roots: Vec<String>) -> BoxFuture<'_, Vec<String>> {
        Box::pin(async { Vec::new() })
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

struct StubCoordinator;

impl HiveCoordinator for StubCoordinator {
    fn dispatch_task(
        &self,
        _request: HiveTaskDispatchRequest,
    ) -> BoxFuture<'_, Result<(), HiveContractError>> {
        Box::pin(async {
            Err(HiveContractError::Unavailable(
                "core IPC is not connected".to_owned(),
            ))
        })
    }
}

struct StubWorkspaceProvider;

impl WorkspaceProvider for StubWorkspaceProvider {
    fn eva_workspace_path(&self) -> BoxFuture<'_, Option<String>> {
        Box::pin(async { None })
    }

    fn selected_model_name(&self) -> Option<String> {
        None
    }
}

#[tokio::main]
async fn main() {
    let journal: Arc<dyn HiveJournalProvider> = Arc::new(StubJournalProvider);
    let coordinator: Arc<dyn HiveCoordinator> = Arc::new(StubCoordinator);
    let workspace_provider: Arc<dyn WorkspaceProvider> = Arc::new(StubWorkspaceProvider);

    let daemon = EvaPulseDaemon::start(journal, None, workspace_provider, coordinator, None);
    let status = daemon.status();

    tracing::info!(
        status = %status.status,
        enabled = status.enabled,
        "evohime-hive-daemon initialized; exiting without Core IPC context"
    );

    daemon.shutdown();
}
