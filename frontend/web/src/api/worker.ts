import { apiRequest } from "./client";

export type WorkerHealthView = {
  healthy: boolean;
  started_at?: string | null;
  pid?: number | null;
  queue_depth?: number | null;
  active_jobs?: number | null;
  checked_at_ms: number;
};

export type WorkerMetricsSnapshot = {
  jobs_submitted: number;
  jobs_completed: number;
  jobs_failed: number;
  jobs_retried: number;
  jobs_stalled: number;
  health_checks_ok: number;
  health_checks_failed: number;
  recoveries: number;
  open_jobs: number;
  avg_job_duration_ms: number;
  last_health?: WorkerHealthView | null;
};

export type WorkerStatusResponse = {
  metrics: WorkerMetricsSnapshot;
  db_status_counts: Record<string, number>;
};

export type WorkerJob = {
  id: string;
  worker_job_id?: string | null;
  task: string;
  payload_json: unknown;
  status: string;
  attempts: number;
  max_attempts: number;
  result_json?: unknown;
  error?: string | null;
  claim_token?: string | null;
  created_at: string;
  updated_at: string;
  completed_at?: string | null;
};

export function fetchWorkerStatus() {
  return apiRequest<WorkerStatusResponse>("/api/worker/status");
}

export function listWorkerJobs(limit = 50) {
  return apiRequest<WorkerJob[]>(`/api/worker/jobs?limit=${limit}`);
}

export function retryWorkerJob(jobId: string) {
  return apiRequest<WorkerJob>(`/api/worker/jobs/${jobId}/retry`, {
    method: "POST",
  });
}
