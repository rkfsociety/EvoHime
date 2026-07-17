import { useCallback, useEffect, useState } from "react";
import {
  fetchWorkerStatus,
  listWorkerJobs,
  retryWorkerJob,
  type WorkerJob,
  type WorkerStatusResponse,
} from "../api/worker";

function canRetry(job: WorkerJob) {
  if (job.attempts >= job.max_attempts) return false;
  return job.status === "failed" || job.status === "stalled";
}

function formatCheckedAt(ms?: number) {
  if (!ms) return "—";
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return "—";
  }
}

export function WorkerSettingsSection() {
  const [status, setStatus] = useState<WorkerStatusResponse | null>(null);
  const [jobs, setJobs] = useState<WorkerJob[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [retryingId, setRetryingId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextStatus, nextJobs] = await Promise.all([
        fetchWorkerStatus(),
        listWorkerJobs(50),
      ]);
      setStatus(nextStatus);
      setJobs(nextJobs);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const onRetry = async (jobId: string) => {
    setRetryingId(jobId);
    setError(null);
    try {
      await retryWorkerJob(jobId);
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setRetryingId(null);
    }
  };

  const metrics = status?.metrics;
  const health = metrics?.last_health;
  const dbCounts = status?.db_status_counts ?? {};

  return (
    <section className="settingsSection workerSettings">
      <div className="settingsInlineBar">
        <div>
          <h3>Python worker</h3>
          <p className="settingsHint">Статус bridge и недавние durable jobs</p>
        </div>
        <button type="button" onClick={() => void refresh()} disabled={loading}>
          {loading ? "Обновление…" : "Обновить"}
        </button>
      </div>

      {error ? <p className="settingsError">{error}</p> : null}

      <div className="workerHealthRow">
        <span
          className={
            health?.healthy ? "workerHealthBadge ok" : "workerHealthBadge bad"
          }
        >
          {health
            ? health.healthy
              ? "Healthy"
              : "Unhealthy"
            : "Нет health snapshot"}
        </span>
        <span>Проверка: {formatCheckedAt(health?.checked_at_ms)}</span>
        {health?.pid != null ? <span>PID {health.pid}</span> : null}
        {health?.queue_depth != null ? (
          <span>Queue {health.queue_depth}</span>
        ) : null}
        {health?.active_jobs != null ? (
          <span>Active {health.active_jobs}</span>
        ) : null}
      </div>

      <div className="workerMetricsGrid">
        <article>
          <strong>{metrics?.jobs_submitted ?? 0}</strong>
          <span>Submitted</span>
        </article>
        <article>
          <strong>{metrics?.jobs_completed ?? 0}</strong>
          <span>Completed</span>
        </article>
        <article>
          <strong>{metrics?.jobs_failed ?? 0}</strong>
          <span>Failed</span>
        </article>
        <article>
          <strong>{metrics?.jobs_retried ?? 0}</strong>
          <span>Retried</span>
        </article>
        <article>
          <strong>{metrics?.jobs_stalled ?? 0}</strong>
          <span>Stalled</span>
        </article>
        <article>
          <strong>{metrics?.open_jobs ?? 0}</strong>
          <span>Open</span>
        </article>
        <article>
          <strong>{metrics?.recoveries ?? 0}</strong>
          <span>Recoveries</span>
        </article>
        <article>
          <strong>
            {metrics ? Math.round(metrics.avg_job_duration_ms) : 0} ms
          </strong>
          <span>Avg duration</span>
        </article>
        <article>
          <strong>
            {metrics
              ? `${metrics.health_checks_ok}/${metrics.health_checks_failed}`
              : "0/0"}
          </strong>
          <span>Health ok/fail</span>
        </article>
      </div>

      <h4 className="workerSubheading">DB status counts</h4>
      <div className="workerDbCounts">
        {Object.keys(dbCounts).length === 0 ? (
          <span className="settingsHint">Пока нет строк в worker_jobs</span>
        ) : (
          Object.entries(dbCounts).map(([statusName, count]) => (
            <span key={statusName}>
              <code>{statusName}</code> {count}
            </span>
          ))
        )}
      </div>

      <h4 className="workerSubheading">Недавние jobs</h4>
      {jobs.length === 0 ? (
        <div className="emptyState">
          <strong>Jobs пока нет</strong>
          <p>Задания появятся после submit через `/api/worker/jobs`.</p>
        </div>
      ) : (
        <div className="workerJobList">
          {jobs.map((job) => (
            <article className="workerJobCard" key={job.id}>
              <div className="workerJobHeader">
                <strong>{job.task}</strong>
                <span className={`workerJobStatus status-${job.status}`}>
                  {job.status}
                </span>
              </div>
              <div className="workerJobMeta">
                <code>{job.id.slice(0, 8)}</code>
                <span>
                  attempts {job.attempts}/{job.max_attempts}
                </span>
                <time dateTime={job.updated_at}>{job.updated_at}</time>
              </div>
              {job.error ? <p className="workerJobError">{job.error}</p> : null}
              {canRetry(job) ? (
                <button
                  type="button"
                  className="workerRetryButton"
                  disabled={retryingId === job.id}
                  onClick={() => void onRetry(job.id)}
                >
                  {retryingId === job.id ? "Retry…" : "Retry"}
                </button>
              ) : null}
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
