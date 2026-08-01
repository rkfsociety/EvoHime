import type { AgentReflectionEvent, ReflectionAction } from "../protocol";

const ACTION_LABELS: Record<ReflectionAction, string> = {
  proceed: "Продолжает",
  ask_user: "Нужна проверка",
  retry_tool: "Повтор инструмента",
  revise_plan: "Пересмотр плана",
  escalate: "Эскалация",
};

function formatPercentage(value: number): string {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return "—";
  }
  return `${Math.round(Math.max(0, Math.min(1, value)) * 100)}%`;
}

function formatTime(timestamp: string): string {
  const parsed = new Date(timestamp);
  return Number.isNaN(parsed.getTime())
    ? ""
    : parsed.toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

export function ReflectionTimeline({ reflections }: { reflections: AgentReflectionEvent[] }) {
  const notable = reflections.filter((reflection) => reflection.action !== "proceed");
  if (notable.length === 0) {
    return null;
  }

  return (
    <details className="reflectionTimeline" aria-label="Самопроверка агента">
      <summary>
        <span className="reflectionTimelineTitle">Самопроверка</span>
        <span className="reflectionTimelineMeta">
          {notable.length} из {reflections.length} шагов под вопросом
        </span>
      </summary>
      <div className="reflectionTimelineBody">
        {notable.map((reflection, index) => (
          <article
            className={`reflectionEntry action-${reflection.action}`}
            key={`${reflection.tool_call_id ?? "reflection"}-${reflection.timestamp}-${index}`}
          >
            <header className="reflectionEntryHeader">
              <span className="reflectionAction">{ACTION_LABELS[reflection.action] ?? reflection.action}</span>
              <span className="reflectionScore" title="Оценка успеха шага">
                {formatPercentage(reflection.analysis.success_score)}
              </span>
              <span className="reflectionTime">{formatTime(reflection.timestamp)}</span>
            </header>
            <p className="reflectionReasoning">{reflection.analysis.reasoning}</p>
            {reflection.analysis.error_patterns.length > 0 ? (
              <ul className="reflectionPatterns">
                {reflection.analysis.error_patterns.map((pattern) => (
                  <li key={pattern.pattern_id}>
                    <span className="reflectionPatternName">{pattern.pattern_name}</span>
                    <span className="reflectionPatternMeta">
                      {pattern.source === "experience_memory" ? "из памяти опыта" : "эвристика"} ·{" "}
                      {formatPercentage(pattern.confidence)}
                    </span>
                  </li>
                ))}
              </ul>
            ) : null}
            {reflection.recommendation ? (
              <p className="reflectionRecommendation">{reflection.recommendation}</p>
            ) : null}
          </article>
        ))}
      </div>
    </details>
  );
}
