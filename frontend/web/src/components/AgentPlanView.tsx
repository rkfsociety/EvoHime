import type { PlanCandidate } from "../protocol";

function formatPercentage(value: number | undefined): string {
  if (value === undefined || value === null) {
    return "N/A";
  }
  // Clamp to [0.0, 1.0] and convert to percentage
  const clamped = Math.max(0, Math.min(1, value));
  return `${Math.round(clamped * 100)}%`;
}

function isValidScore(value: number | undefined): boolean {
  if (value === undefined || value === null) {
    return false;
  }
  return typeof value === "number" && !isNaN(value);
}

export function AgentPlanView({
  candidates,
  chosenPlanId,
  reasoning,
}: {
  candidates: PlanCandidate[];
  chosenPlanId: string;
  reasoning: string;
}) {
  if (!candidates || candidates.length === 0) {
    return null;
  }

  const chosenCandidate = candidates.find((c) => c.id === chosenPlanId);
  const missingChosenPlan = !chosenCandidate && chosenPlanId;

  return (
    <div className="agentPlanView">
      {missingChosenPlan && (
        <div className="agentPlanWarning">
          <strong>Внимание:</strong> Выбранный план с ID "{chosenPlanId}" не найден в списке кандидатов
        </div>
      )}

      {reasoning && (
        <div className="agentPlanReasoning">
          <strong>Рассуждение:</strong>
          <p>{reasoning}</p>
        </div>
      )}

      <div className="agentPlanCandidates">
        {candidates.map((candidate) => {
          const isChosen = candidate.id === chosenPlanId;
          const confidencePercent = formatPercentage(candidate.confidence);

          return (
            <article
              key={candidate.id}
              className={`agentPlanCandidate${isChosen ? " chosen" : ""}`}
            >
              <div className="agentPlanCandidateHeader">
                <div className="agentPlanCandidateInfo">
                  <strong>{candidate.description}</strong>
                  {isChosen && <span className="agentPlanChosenBadge">Выбран</span>}
                </div>
                <div className="agentPlanConfidence">{confidencePercent}</div>
              </div>

              <details className="agentPlanScoreBreakdown">
                <summary>Разбор оценок</summary>
                <table>
                  <tbody>
                    <tr>
                      <th scope="row">Сходство</th>
                      <td className="scoreValue">
                        {formatPercentage(candidate.score_breakdown.similarity_score)}
                      </td>
                    </tr>
                    <tr>
                      <th scope="row">Вероятность успеха инструмента</th>
                      <td className="scoreValue">
                        {formatPercentage(candidate.score_breakdown.tool_success_rate)}
                      </td>
                    </tr>
                    <tr>
                      <th scope="row">Штраф за сложность</th>
                      <td className="scoreValue">
                        {formatPercentage(candidate.score_breakdown.complexity_penalty)}
                      </td>
                    </tr>
                    <tr>
                      <th scope="row">Корректировка по отзывам</th>
                      <td className="scoreValue">
                        {formatPercentage(candidate.score_breakdown.feedback_adjustment)}
                      </td>
                    </tr>
                    <tr className="scoreFinal">
                      <th scope="row">Итоговая оценка</th>
                      <td className="scoreValue">
                        {formatPercentage(candidate.score_breakdown.final_score)}
                      </td>
                    </tr>
                  </tbody>
                </table>
              </details>
            </article>
          );
        })}
      </div>
    </div>
  );
}
