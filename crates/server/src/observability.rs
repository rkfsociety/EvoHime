//! Task-pipeline observability: correlation fields + in-process metrics (P1).
//!
//! Correlation id for a task equals `task_id` (stable across pause/resume).
//! Snapshots are exposed via `GET /api/metrics` for local debugging.
//! When OTLP is enabled, open task/tool/approval spans are exported via tracing.
//!
//! Mutex poison is recovered via `into_inner` (Stage 7.20) so metrics never panic the server.

use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::Span;
use uuid::Uuid;

/// In-process metrics collector for the task / tool / approval pipeline.
#[derive(Debug, Default)]
pub struct PipelineMetrics {
    inner: Mutex<PipelineMetricsInner>,
}

#[derive(Debug)]
struct TimedSpan {
    started: Instant,
    span: Span,
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

    llm_calls: u64,
    llm_calls_failed: u64,
    llm_prompt_tokens: u64,
    llm_completion_tokens: u64,
    llm_duration_ms_total: u64,
    llm_duration_samples: u64,

    task_duration_ms_total: u64,
    task_duration_samples: u64,
    tool_duration_ms_total: u64,
    tool_duration_samples: u64,
    approval_latency_ms_total: u64,
    approval_latency_samples: u64,

    open_tasks: HashMap<Uuid, TimedSpan>,
    /// FIFO starts per (task_id, tool_name) — parallel same-tool steps.
    open_tools: HashMap<(Uuid, String), VecDeque<TimedSpan>>,
    open_approvals: HashMap<Uuid, TimedSpan>,
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
    pub llm_calls: u64,
    pub llm_calls_failed: u64,
    pub llm_prompt_tokens: u64,
    pub llm_completion_tokens: u64,
    pub avg_llm_duration_ms: f64,
    pub open_tasks: u64,
    pub open_approvals: u64,
    pub avg_task_duration_ms: f64,
    pub avg_tool_duration_ms: f64,
    pub avg_approval_latency_ms: f64,
    /// True when `OTEL_EXPORTER_OTLP_ENDPOINT` is configured and SDK is not disabled.
    pub otel_export_enabled: bool,
}

impl PipelineMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
            llm_calls: inner.llm_calls,
            llm_calls_failed: inner.llm_calls_failed,
            llm_prompt_tokens: inner.llm_prompt_tokens,
            llm_completion_tokens: inner.llm_completion_tokens,
            avg_llm_duration_ms: avg_ms(inner.llm_duration_ms_total, inner.llm_duration_samples),
            open_tasks: inner.open_tasks.len() as u64,
            open_approvals: inner.open_approvals.len() as u64,
            avg_task_duration_ms: avg_ms(inner.task_duration_ms_total, inner.task_duration_samples),
            avg_tool_duration_ms: avg_ms(inner.tool_duration_ms_total, inner.tool_duration_samples),
            avg_approval_latency_ms: avg_ms(
                inner.approval_latency_ms_total,
                inner.approval_latency_samples,
            ),
            otel_export_enabled: crate::otel::export_enabled(),
        }
    }

    pub fn task_started(&self, session_id: Uuid, task_id: Uuid) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.tasks_started += 1;
        let span = tracing::info_span!(
            "task.pipeline",
            otel.name = "task.pipeline",
            correlation_id = %task_id,
            session_id = %session_id,
            task_id = %task_id,
        );
        let _guard = span.enter();
        tracing::info!("task.pipeline.started");
        drop(_guard);
        inner.open_tasks.insert(
            task_id,
            TimedSpan {
                started: Instant::now(),
                span,
            },
        );
        inner.seen_plan.insert(task_id, false);
    }

    /// Re-attach timing for a resumed/paused task without bumping `tasks_started`.
    pub fn task_resumed(&self, session_id: Uuid, task_id: Uuid) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let std::collections::hash_map::Entry::Vacant(entry) = inner.open_tasks.entry(task_id) {
            let span = tracing::info_span!(
                "task.pipeline",
                otel.name = "task.pipeline",
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                resumed = true,
            );
            let _guard = span.enter();
            tracing::info!("task.pipeline.resumed");
            drop(_guard);
            entry.insert(TimedSpan {
                started: Instant::now(),
                span,
            });
        } else {
            tracing::info!(
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                "task.pipeline.resumed"
            );
        }
        inner.seen_plan.entry(task_id).or_insert(true);
    }

    pub fn task_finished(&self, session_id: Uuid, task_id: Uuid, ok: bool) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let timed = inner.open_tasks.remove(&task_id);
        let elapsed = timed.as_ref().map(|t| t.started.elapsed());
        if let Some(timed) = timed {
            timed.span.record("ok", ok);
            if let Some(elapsed) = elapsed {
                timed.span.record("duration_ms", duration_ms(elapsed));
            }
            let _guard = timed.span.enter();
            tracing::info!(
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                ok,
                duration_ms = elapsed.map(duration_ms).unwrap_or(0),
                "task.pipeline.finished"
            );
            drop(_guard);
            drop(timed.span);
        } else {
            tracing::info!(
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                ok,
                duration_ms = 0u64,
                "task.pipeline.finished"
            );
        }
        inner.seen_plan.remove(&task_id);
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
    }

    pub fn plan_updated(&self, session_id: Uuid, task_id: Uuid, step_count: usize) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let first = inner.seen_plan.get(&task_id).copied().unwrap_or(false);
        if first {
            inner.plan_updates += 1;
        } else {
            inner.seen_plan.insert(task_id, true);
        }
        if let Some(timed) = inner.open_tasks.get(&task_id) {
            let _guard = timed.span.enter();
            tracing::info!(
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                step_count,
                replan = first,
                "task.pipeline.plan_updated"
            );
        } else {
            tracing::info!(
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                step_count,
                replan = first,
                "task.pipeline.plan_updated"
            );
        }
    }

    pub fn tool_started(&self, session_id: Uuid, task_id: Uuid, tool_name: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.tools_started += 1;
        let span = match inner.open_tasks.get(&task_id) {
            Some(parent) => tracing::info_span!(
                parent: &parent.span,
                "task.pipeline.tool",
                otel.name = "task.pipeline.tool",
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                tool_name,
            ),
            None => tracing::info_span!(
                "task.pipeline.tool",
                otel.name = "task.pipeline.tool",
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                tool_name,
            ),
        };
        let _guard = span.enter();
        tracing::info!("task.pipeline.tool_started");
        drop(_guard);
        inner
            .open_tools
            .entry((task_id, tool_name.to_string()))
            .or_default()
            .push_back(TimedSpan {
                started: Instant::now(),
                span,
            });
    }

    pub fn tool_completed(
        &self,
        session_id: Uuid,
        task_id: Uuid,
        tool_name: &str,
        success: bool,
    ) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let key = (task_id, tool_name.to_string());
        let timed = {
            let queue = inner.open_tools.get_mut(&key);
            let timed = queue.and_then(|q| q.pop_front());
            if inner
                .open_tools
                .get(&key)
                .map(|q| q.is_empty())
                .unwrap_or(false)
            {
                inner.open_tools.remove(&key);
            }
            timed
        };
        let elapsed = timed.as_ref().map(|t| t.started.elapsed());
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
        if let Some(timed) = timed {
            timed.span.record("success", success);
            if let Some(elapsed) = elapsed {
                timed.span.record("duration_ms", duration_ms(elapsed));
            }
            let _guard = timed.span.enter();
            tracing::info!(
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                tool_name,
                success,
                duration_ms = elapsed.map(duration_ms).unwrap_or(0),
                "task.pipeline.tool_completed"
            );
            drop(_guard);
            drop(timed.span);
        } else {
            tracing::info!(
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                tool_name,
                success,
                duration_ms = 0u64,
                "task.pipeline.tool_completed"
            );
        }
    }

    pub fn approval_requested(&self, session_id: Uuid, task_id: Uuid, approval_id: Uuid, tool: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.approvals_requested += 1;
        let span = match inner.open_tasks.get(&task_id) {
            Some(parent) => tracing::info_span!(
                parent: &parent.span,
                "task.pipeline.approval",
                otel.name = "task.pipeline.approval",
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                approval_id = %approval_id,
                tool_name = tool,
            ),
            None => tracing::info_span!(
                "task.pipeline.approval",
                otel.name = "task.pipeline.approval",
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                approval_id = %approval_id,
                tool_name = tool,
            ),
        };
        let _guard = span.enter();
        tracing::info!("task.pipeline.approval_requested");
        drop(_guard);
        inner.open_approvals.insert(
            approval_id,
            TimedSpan {
                started: Instant::now(),
                span,
            },
        );
    }

    pub fn approval_resolved(
        &self,
        session_id: Uuid,
        task_id: Uuid,
        approval_id: Uuid,
        granted: bool,
    ) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let timed = inner.open_approvals.remove(&approval_id);
        let elapsed = timed.as_ref().map(|t| t.started.elapsed());
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
        if let Some(timed) = timed {
            timed.span.record("granted", granted);
            if let Some(elapsed) = elapsed {
                timed.span.record("duration_ms", duration_ms(elapsed));
            }
            let _guard = timed.span.enter();
            tracing::info!(
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                approval_id = %approval_id,
                granted,
                duration_ms = elapsed.map(duration_ms).unwrap_or(0),
                "task.pipeline.approval_resolved"
            );
            drop(_guard);
            drop(timed.span);
        } else {
            tracing::info!(
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                approval_id = %approval_id,
                granted,
                duration_ms = 0u64,
                "task.pipeline.approval_resolved"
            );
        }
    }

    pub fn task_retry(&self, session_id: Uuid, task_id: Uuid) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.task_retries += 1;
        if let Some(timed) = inner.open_tasks.get(&task_id) {
            let _guard = timed.span.enter();
            tracing::info!(
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                "task.pipeline.retry"
            );
        } else {
            tracing::info!(
                correlation_id = %task_id,
                session_id = %session_id,
                task_id = %task_id,
                "task.pipeline.retry"
            );
        }
    }

    /// Record one LLM completion (plan / replan / respond / extract).
    pub fn llm_call(
        &self,
        phase: &str,
        model: &str,
        usage: Option<(u32, u32)>,
        duration_ms: u64,
        ok: bool,
    ) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.llm_calls += 1;
        if !ok {
            inner.llm_calls_failed += 1;
        }
        if let Some((prompt, completion)) = usage {
            inner.llm_prompt_tokens += u64::from(prompt);
            inner.llm_completion_tokens += u64::from(completion);
        }
        inner.llm_duration_ms_total += duration_ms;
        inner.llm_duration_samples += 1;
        tracing::debug!(
            phase,
            model,
            ok,
            duration_ms,
            prompt_tokens = usage.map(|(p, _)| p).unwrap_or(0),
            completion_tokens = usage.map(|(_, c)| c).unwrap_or(0),
            "task.pipeline.llm_call"
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
        assert!(!snap.otel_export_enabled);
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
