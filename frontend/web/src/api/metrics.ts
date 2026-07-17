import { apiRequest } from "./client";
import type { WorkerMetricsSnapshot } from "./worker";

export type PipelineMetricsSnapshot = {
  tasks_started: number;
  tasks_completed: number;
  tasks_failed: number;
  tools_started: number;
  tools_completed: number;
  tools_failed: number;
  approvals_requested: number;
  approvals_granted: number;
  approvals_denied: number;
  task_retries: number;
  plan_updates: number;
  open_tasks: number;
  open_approvals: number;
  avg_task_duration_ms: number;
  avg_tool_duration_ms: number;
  avg_approval_latency_ms: number;
  otel_export_enabled: boolean;
};

export type MetricsResponse = {
  pipeline: PipelineMetricsSnapshot;
  worker: WorkerMetricsSnapshot;
};

export function fetchMetrics() {
  return apiRequest<MetricsResponse>("/api/metrics");
}
