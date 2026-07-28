import { useCallback, useEffect, useState } from "react";
import {
  fetchMetrics,
  fetchMetricsHistory,
  type MetricsHistoryEntry,
  type MetricsResponse,
} from "../api/metrics";

function ms(value?: number) {
  return `${Math.round(value ?? 0)} ms`;
}

type TrendField = {
  label: string;
  pick: (entry: MetricsHistoryEntry) => number;
  format: (value: number) => string;
};

const TREND_FIELDS: TrendField[] = [
  { label: "Avg task", pick: (e) => e.pipeline.avg_task_duration_ms, format: ms },
  { label: "Avg tool", pick: (e) => e.pipeline.avg_tool_duration_ms, format: ms },
  { label: "Avg LLM", pick: (e) => e.pipeline.avg_llm_duration_ms, format: ms },
  { label: "Open tasks", pick: (e) => e.pipeline.open_tasks, format: (v) => String(v) },
  { label: "Open jobs", pick: (e) => e.worker.open_jobs, format: (v) => String(v) },
  { label: "Avg job", pick: (e) => e.worker.avg_job_duration_ms, format: ms },
];

function Sparkline({ values }: { values: number[] }) {
  const width = 200;
  const height = 40;
  if (values.length < 2) {
    return <span className="metricsSparklineEmpty">нет данных</span>;
  }
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const step = width / (values.length - 1);
  const points = values
    .map((value, index) => {
      const x = index * step;
      const y = height - ((value - min) / span) * height;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  return (
    <svg
      className="metricsSparkline"
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
    >
      <polyline points={points} fill="none" strokeWidth={2} />
    </svg>
  );
}

export function MetricsSettingsSection() {
  const [metrics, setMetrics] = useState<MetricsResponse | null>(null);
  const [history, setHistory] = useState<MetricsHistoryEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [current, historyResponse] = await Promise.all([
        fetchMetrics(),
        fetchMetricsHistory(60),
      ]);
      setMetrics(current);
      setHistory([...historyResponse.entries].reverse());
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
          <strong>{pipeline?.llm_calls ?? 0}</strong>
          <span>LLM calls</span>
        </article>
        <article>
          <strong>{pipeline?.llm_calls_failed ?? 0}</strong>
          <span>LLM failed</span>
        </article>
        <article>
          <strong>{pipeline?.llm_prompt_tokens ?? 0}</strong>
          <span>Prompt tokens</span>
        </article>
        <article>
          <strong>{pipeline?.llm_completion_tokens ?? 0}</strong>
          <span>Completion tokens</span>
        </article>
        <article>
          <strong>{ms(pipeline?.avg_llm_duration_ms)}</strong>
          <span>Avg LLM</span>
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

      <h4 className="workerSubheading">
        История трендов ({history.length} снапшотов)
      </h4>
      {history.length < 2 ? (
        <p className="settingsHint">
          Недостаточно персистентных снапшотов для графика — включите
          `EVOHIME_METRICS_PERSIST_INTERVAL_SECS` и подождите несколько интервалов.
        </p>
      ) : (
        <div className="metricsTrendsGrid">
          {TREND_FIELDS.map((field) => {
            const values = history.map(field.pick);
            const latest = values[values.length - 1];
            return (
              <article key={field.label} className="metricsTrendCard">
                <div className="metricsTrendHeader">
                  <span>{field.label}</span>
                  <strong>{field.format(latest)}</strong>
                </div>
                <Sparkline values={values} />
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
