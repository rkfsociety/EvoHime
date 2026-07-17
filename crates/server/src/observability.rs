//! Task-pipeline observability: correlation fields + in-process metrics (P1).
//!
//! Correlation id for a task equals `task_id` (stable across pause/resume).
//! Snapshots are exposed via `GET /api/metrics` for local debugging.

use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// In-process metrics collector for the task / tool / approval pipeline.
#[derive(Debug, Default)]
pub struct PipelineMetrics {
    inner: Mutex<PipelineMetricsInner>,
}

#[derive(Debug, Default)]
struct PipelineMetricsInner {
    tasks_started: u64,
    tasks_completed: u64,
    tasks_failed: u64,
    tools_started: u64,
    tools_completed: u64,
    tools_failed: u64,
    approvals_requested: u64,
    approvals_granted: u64,
    approvals_denied: u64,
    task_retries: u64,
    plan_updates: u64,

    task_duration_ms_total: u64,
    task_duration_samples: u64,
    tool_duration_ms_total: u64,
    tool_duration_samples: u64,
    approval_latency_ms_total: u64,
    approval_latency_samples: u64,

    open_tasks: HashMap<Uuid, Instant>,
    /// FIFO starts per (task_id, tool_name) — parallel same-tool steps.
    open_tools: HashMap<(Uuid, String), VecDeque<Instant>>,
    open_approvals: HashMap<Uuid, Instant>,
    /// First plan already seen for this task (next updates count as replans).
    seen_plan: HashMap<Uuid, bool>,
}

/// Serializable metrics snapshot for HTTP.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MetricsSnapshot {
    pub tasks_started: u64,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
    pub tools_started: u64,
    pub tools_completed: u64,
    pub tools_failed: u64,
    pub approvals_requested: u64,
    pub approvals_granted: u64,
    pub approvals_denied: u64,
    pub task_retries: u64,
    pub plan_updates: u64,
    pub open_tasks: u64,
    pub open_approvals: u64,
    pub avg_task_duration_ms: f64,
    pub avg_tool_duration_ms: f64,
    pub avg_approval_latency_ms: f64,
}

impl PipelineMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let inner = self.inner.lock().expect("pipeline metrics lock");
        MetricsSnapshot {
            tasks_started: inner.tasks_started,
            tasks_completed: inner.tasks_completed,
            tasks_failed: inner.tasks_failed,
            tools_started: inner.tools_started,
            tools_completed: inner.tools_completed,
            tools_failed: inner.tools_failed,
            approvals_requested: inner.approvals_requested,
            approvals_granted: inner.approvals_granted,
            approvals_denied: inner.approvals_denied,
            task_retries: inner.task_retries,
            plan_updates: inner.plan_updates,
            open_tasks: inner.open_tasks.len() as u64,
            open_approvals: inner.open_approvals.len() as u64,
            avg_task_duration_ms: avg_ms(inner.task_duration_ms_total, inner.task_duration_samples),
            avg_tool_duration_ms: avg_ms(inner.tool_duration_ms_total, inner.tool_duration_samples),
            avg_approval_latency_ms: avg_ms(
                inner.approval_latency_ms_total,
                inner.approval_latency_samples,
            ),
        }
    }

    pub fn task_started(&self, session_id: Uuid, task_id: Uuid) {
        let mut inner = self.inner.lock().expect("pipeline metrics lock");
        inner.tasks_started += 1;
        inner.open_tasks.insert(task_id, Instant::now());
        inner.seen_plan.insert(task_id, false);
        tracing::info!(
            correlation_id = %task_id,
            session_id = %session_id,
            task_id = %task_id,
            "task.pipeline.started"
        );
    }

    /// Re-attach timing for a resumed/paused task without bumping `tasks_started`.
    pub fn task_resumed(&self, session_id: Uuid, task_id: Uuid) {
        let mut inner = self.inner.lock().expect("pipeline metrics lock");
        inner.open_tasks.entry(task_id).or_insert_with(Instant::now);
        inner.seen_plan.entry(task_id).or_insert(true);
        tracing::info!(
            correlation_id = %task_id,
            session_id = %session_id,
            task_id = %task_id,
            "task.pipeline.resumed"
        );
    }

    pub fn task_finished(&self, session_id: Uuid, task_id: Uuid, ok: bool) {
        let mut inner = self.inner.lock().expect("pipeline metrics lock");
        let elapsed = inner
            .open_tasks
            .remove(&task_id)
            .map(|started| started.elapsed());
        inner.seen_plan.remove(&task_id);
        // Drop dangling tool timers for this task.
        inner.open_tools.retain(|(tid, _), _| *tid != task_id);
        if ok {
            inner.tasks_completed += 1;
        } else {
            inner.tasks_failed += 1;
        }
        if let Some(elapsed) = elapsed {
            let ms = duration_ms(elapsed);
            inner.task_duration_ms_total = inner.task_duration_ms_total.saturating_add(ms);
            inner.task_duration_samples = inner.task_duration_samples.saturating_add(1);
        }
        tracing::info!(
            correlation_id = %task_id,
            session_id = %session_id,
            task_id = %task_id,
            ok,
            duration_ms = elapsed.map(duration_ms).unwrap_or(0),
            "task.pipeline.finished"
        );
    }

    pub fn plan_updated(&self, session_id: Uuid, task_id: Uuid, step_count: usize) {
        let mut inner = self.inner.lock().expect("pipeline metrics lock");
        let first = inner.seen_plan.get(&task_id).copied().unwrap_or(false);
        if first {
            inner.plan_updates += 1;
        } else {
            inner.seen_plan.insert(task_id, true);
        }
        tracing::info!(
            correlation_id = %task_id,
            session_id = %session_id,
            task_id = %task_id,
            step_count,
            replan = first,
            "task.pipeline.plan_updated"
        );
    }

    pub fn tool_started(&self, session_id: Uuid, task_id: Uuid, tool_name: &str) {
        let mut inner = self.inner.lock().expect("pipeline metrics lock");
        inner.tools_started += 1;
        inner
            .open_tools
            .entry((task_id, tool_name.to_string()))
            .or_default()
            .push_back(Instant::now());
        tracing::info!(
            correlation_id = %task_id,
            session_id = %session_id,
            task_id = %task_id,
            tool_name,
            "task.pipeline.tool_started"
        );
    }

    pub fn tool_completed(
        &self,
        session_id: Uuid,
        task_id: Uuid,
        tool_name: &str,
        success: bool,
    ) {
        let mut inner = self.inner.lock().expect("pipeline metrics lock");
        let key = (task_id, tool_name.to_string());
        let elapsed = {
            let queue = inner.open_tools.get_mut(&key);
            let started = queue.and_then(|q| q.pop_front());
            if inner
                .open_tools
                .get(&key)
                .map(|q| q.is_empty())
                .unwrap_or(false)
            {
                inner.open_tools.remove(&key);
            }
            started.map(|started| started.elapsed())
        };
        if success {
            inner.tools_completed += 1;
        } else {
            inner.tools_failed += 1;
        }
        if let Some(elapsed) = elapsed {
            let ms = duration_ms(elapsed);
            inner.tool_duration_ms_total = inner.tool_duration_ms_total.saturating_add(ms);
            inner.tool_duration_samples = inner.tool_duration_samples.saturating_add(1);
        }
        tracing::info!(
            correlation_id = %task_id,
            session_id = %session_id,
            task_id = %task_id,
            tool_name,
            success,
            duration_ms = elapsed.map(duration_ms).unwrap_or(0),
            "task.pipeline.tool_completed"
        );
    }

    pub fn approval_requested(&self, session_id: Uuid, task_id: Uuid, approval_id: Uuid, tool: &str) {
        let mut inner = self.inner.lock().expect("pipeline metrics lock");
        inner.approvals_requested += 1;
        inner.open_approvals.insert(approval_id, Instant::now());
        tracing::info!(
            correlation_id = %task_id,
            session_id = %session_id,
            task_id = %task_id,
            approval_id = %approval_id,
            tool_name = tool,
            "task.pipeline.approval_requested"
        );
    }

    pub fn approval_resolved(
        &self,
        session_id: Uuid,
        task_id: Uuid,
        approval_id: Uuid,
        granted: bool,
    ) {
        let mut inner = self.inner.lock().expect("pipeline metrics lock");
        let elapsed = inner
            .open_approvals
            .remove(&approval_id)
            .map(|started| started.elapsed());
        if granted {
            inner.approvals_granted += 1;
        } else {
            inner.approvals_denied += 1;
        }
        if let Some(elapsed) = elapsed {
            let ms = duration_ms(elapsed);
            inner.approval_latency_ms_total = inner.approval_latency_ms_total.saturating_add(ms);
            inner.approval_latency_samples = inner.approval_latency_samples.saturating_add(1);
        }
        tracing::info!(
            correlation_id = %task_id,
            session_id = %session_id,
            task_id = %task_id,
            approval_id = %approval_id,
            granted,
            duration_ms = elapsed.map(duration_ms).unwrap_or(0),
            "task.pipeline.approval_resolved"
        );
    }

    pub fn task_retry(&self, session_id: Uuid, task_id: Uuid) {
        let mut inner = self.inner.lock().expect("pipeline metrics lock");
        inner.task_retries += 1;
        tracing::info!(
            correlation_id = %task_id,
            session_id = %session_id,
            task_id = %task_id,
            "task.pipeline.retry"
        );
    }
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn avg_ms(total: u64, samples: u64) -> f64 {
    if samples == 0 {
        0.0
    } else {
        total as f64 / samples as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn records_task_tool_and_approval_latency() {
        let metrics = PipelineMetrics::new();
        let session = Uuid::new_v4();
        let task = Uuid::new_v4();
        let approval = Uuid::new_v4();

        metrics.task_started(session, task);
        metrics.plan_updated(session, task, 2);
        metrics.plan_updated(session, task, 1); // replan
        metrics.tool_started(session, task, "filesystem.read");
        thread::sleep(Duration::from_millis(5));
        metrics.tool_completed(session, task, "filesystem.read", true);
        metrics.approval_requested(session, task, approval, "filesystem.write");
        thread::sleep(Duration::from_millis(5));
        metrics.approval_resolved(session, task, approval, true);
        metrics.task_finished(session, task, true);
        metrics.task_retry(session, task);

        let snap = metrics.snapshot();
        assert_eq!(snap.tasks_started, 1);
        assert_eq!(snap.tasks_completed, 1);
        assert_eq!(snap.tools_started, 1);
        assert_eq!(snap.tools_completed, 1);
        assert_eq!(snap.approvals_requested, 1);
        assert_eq!(snap.approvals_granted, 1);
        assert_eq!(snap.plan_updates, 1);
        assert_eq!(snap.task_retries, 1);
        assert_eq!(snap.open_tasks, 0);
        assert_eq!(snap.open_approvals, 0);
        assert!(snap.avg_tool_duration_ms >= 1.0);
        assert!(snap.avg_approval_latency_ms >= 1.0);
        assert!(snap.avg_task_duration_ms >= 1.0);
    }

    #[test]
    fn parallel_same_tool_uses_fifo_timing() {
        let metrics = PipelineMetrics::new();
        let session = Uuid::new_v4();
        let task = Uuid::new_v4();
        metrics.tool_started(session, task, "shell.execute");
        metrics.tool_started(session, task, "shell.execute");
        metrics.tool_completed(session, task, "shell.execute", true);
        metrics.tool_completed(session, task, "shell.execute", false);
        let snap = metrics.snapshot();
        assert_eq!(snap.tools_started, 2);
        assert_eq!(snap.tools_completed, 1);
        assert_eq!(snap.tools_failed, 1);
    }
}
