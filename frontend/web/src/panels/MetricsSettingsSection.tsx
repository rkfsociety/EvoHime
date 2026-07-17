import { useCallback, useEffect, useState } from "react";
import { fetchMetrics, type MetricsResponse } from "../api/metrics";

function ms(value?: number) {
  return `${Math.round(value ?? 0)} ms`;
}

export function MetricsSettingsSection() {
  const [metrics, setMetrics] = useState<MetricsResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setMetrics(await fetchMetrics());
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const pipeline = metrics?.pipeline;
  const worker = metrics?.worker;

  return (
    <section className="settingsSection metricsSettings">
      <div className="settingsInlineBar">
        <div>
          <h3>Pipeline metrics</h3>
          <p className="settingsHint">
            Live snapshot `/api/metrics`, history `/api/metrics/history`, Prometheus
            `/metrics`
          </p>
        </div>
        <button type="button" onClick={() => void refresh()} disabled={loading}>
          {loading ? "Обновление…" : "Обновить"}
        </button>
      </div>

      {error ? <p className="settingsError">{error}</p> : null}

      <div className="workerHealthRow">
        <span
          className={
            pipeline?.otel_export_enabled
              ? "workerHealthBadge ok"
              : "workerHealthBadge"
          }
        >
          OTLP {pipeline?.otel_export_enabled ? "enabled" : "off"}
        </span>
        <span className="settingsHint">
          Включается через `OTEL_EXPORTER_OTLP_ENDPOINT`
        </span>
      </div>

      <div className="workerHealthRow">
        <span
          className={
            metrics?.persist?.enabled
              ? "workerHealthBadge ok"
              : "workerHealthBadge"
          }
        >
          Persist {metrics?.persist?.enabled ? "on" : "off"}
        </span>
        <span className="settingsHint">
          {metrics?.persist?.enabled
            ? `каждые ${metrics.persist.interval_secs}s → PG; scrape: /metrics`
            : "EVOHIME_METRICS_PERSIST_INTERVAL_SECS=0; scrape всё равно на /metrics"}
          {metrics?.persist?.last_persisted_at
            ? ` · last ${metrics.persist.last_persisted_at}`
            : ""}
        </span>
      </div>

      <h4 className="workerSubheading">Tasks / tools / approvals</h4>
      <div className="workerMetricsGrid">
        <article>
          <strong>{pipeline?.tasks_started ?? 0}</strong>
          <span>Tasks started</span>
        </article>
        <article>
          <strong>{pipeline?.tasks_completed ?? 0}</strong>
          <span>Completed</span>
        </article>
        <article>
          <strong>{pipeline?.tasks_failed ?? 0}</strong>
          <span>Failed</span>
        </article>
        <article>
          <strong>{pipeline?.open_tasks ?? 0}</strong>
          <span>Open tasks</span>
        </article>
        <article>
          <strong>{pipeline?.task_retries ?? 0}</strong>
          <span>Retries</span>
        </article>
        <article>
          <strong>{pipeline?.plan_updates ?? 0}</strong>
          <span>Plan updates</span>
        </article>
        <article>
          <strong>{pipeline?.tools_started ?? 0}</strong>
          <span>Tools started</span>
        </article>
        <article>
          <strong>{pipeline?.tools_completed ?? 0}</strong>
          <span>Tools ok</span>
        </article>
        <article>
          <strong>{pipeline?.tools_failed ?? 0}</strong>
          <span>Tools failed</span>
        </article>
        <article>
          <strong>{pipeline?.approvals_requested ?? 0}</strong>
          <span>Approvals req</span>
        </article>
        <article>
          <strong>{pipeline?.approvals_granted ?? 0}</strong>
          <span>Granted</span>
        </article>
        <article>
          <strong>{pipeline?.approvals_denied ?? 0}</strong>
          <span>Denied</span>
        </article>
        <article>
          <strong>{pipeline?.open_approvals ?? 0}</strong>
          <span>Open approvals</span>
        </article>
        <article>
          <strong>{ms(pipeline?.avg_task_duration_ms)}</strong>
          <span>Avg task</span>
        </article>
        <article>
          <strong>{ms(pipeline?.avg_tool_duration_ms)}</strong>
          <span>Avg tool</span>
        </article>
        <article>
          <strong>{ms(pipeline?.avg_approval_latency_ms)}</strong>
          <span>Avg approval</span>
        </article>
      </div>

      <h4 className="workerSubheading">Worker (from same snapshot)</h4>
      <div className="workerMetricsGrid">
        <article>
          <strong>{worker?.jobs_submitted ?? 0}</strong>
          <span>Submitted</span>
        </article>
        <article>
          <strong>{worker?.jobs_completed ?? 0}</strong>
          <span>Completed</span>
        </article>
        <article>
          <strong>{worker?.jobs_failed ?? 0}</strong>
          <span>Failed</span>
        </article>
        <article>
          <strong>{worker?.open_jobs ?? 0}</strong>
          <span>Open</span>
        </article>
        <article>
          <strong>{ms(worker?.avg_job_duration_ms)}</strong>
          <span>Avg job</span>
        </article>
        <article>
          <strong>{worker?.recoveries ?? 0}</strong>
          <span>Recoveries</span>
        </article>
      </div>
    </section>
  );
}
