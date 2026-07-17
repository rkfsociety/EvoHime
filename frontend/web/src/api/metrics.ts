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
  llm_calls: number;
  llm_calls_failed: number;
  llm_prompt_tokens: number;
  llm_completion_tokens: number;
  avg_llm_duration_ms: number;
  open_tasks: number;
  open_approvals: number;
  avg_task_duration_ms: number;
  avg_tool_duration_ms: number;
  avg_approval_latency_ms: number;
  otel_export_enabled: boolean;
};

export type MetricsPersistStatus = {
  enabled: boolean;
  interval_secs: number;
  history_limit: number;
  last_persisted_at?: string | null;
};

export type MetricsResponse = {
  pipeline: PipelineMetricsSnapshot;
  worker: WorkerMetricsSnapshot;
  persist?: MetricsPersistStatus;
};

export type MetricsHistoryEntry = {
  id: number;
  captured_at: string;
  pipeline: PipelineMetricsSnapshot;
  worker: WorkerMetricsSnapshot;
};

export function fetchMetrics() {
  return apiRequest<MetricsResponse>("/api/metrics");
}

export function fetchMetricsHistory(limit = 60) {
  return apiRequest<{ entries: MetricsHistoryEntry[] }>(
    `/api/metrics/history?limit=${limit}`,
  );
}
