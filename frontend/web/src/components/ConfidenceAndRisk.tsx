import "./ConfidenceAndRisk.css";

interface SignalBreakdown {
  model_confidence?: number;
  experience_alignment?: number;
  tool_success_rate?: number;
  reflection_confidence?: number;
}

interface Reliability {
  model_confidence?: string;
  experience_alignment?: string;
  tool_success_rate?: string;
  reflection_confidence?: string;
}

export interface ConfidenceAndRiskProps {
  confidence_score: number;
  risk_level: "None" | "Low" | "Medium" | "High";
  breakdown?: SignalBreakdown;
  reliability?: Reliability;
  missing_signals?: string[];
  recommendation?: string;
}

export function ConfidenceAndRisk({
  confidence_score,
  risk_level,
  breakdown,
  reliability,
  missing_signals,
  recommendation,
}: ConfidenceAndRiskProps) {
  const confidencePercent = Math.round(confidence_score * 100);
  const riskColors = {
    None: "var(--success)",
    Low: "var(--info)",
    Medium: "var(--warning)",
    High: "var(--danger)",
  };

  return (
    <div className="confidence-and-risk">
      <div className="confidence-header">
        <div className="confidence-score">
          <div className="confidence-bar-container">
            <div
              className="confidence-bar"
              style={{
                width: `${confidencePercent}%`,
                backgroundColor: confidencePercent >= 70 ? "var(--success)" : confidencePercent >= 50 ? "var(--warning)" : "var(--danger)",
              }}
            />
          </div>
          <span className="confidence-percentage">{confidencePercent}%</span>
        </div>
        <div className="risk-badge" style={{ borderColor: riskColors[risk_level], color: riskColors[risk_level] }}>
          Risk: {risk_level}
        </div>
      </div>

      {breakdown && (
        <div className="signal-breakdown">
          <h4>Сигналы уверенности</h4>
          <div className="signals-grid">
            {breakdown.model_confidence !== undefined && (
              <div className="signal-item">
                <span className="signal-label">Модель</span>
                <span className="signal-value">{Math.round(breakdown.model_confidence * 100)}%</span>
                {reliability?.model_confidence && <span className="signal-reliability">{reliability.model_confidence}</span>}
              </div>
            )}
            {breakdown.experience_alignment !== undefined && (
              <div className="signal-item">
                <span className="signal-label">Опыт</span>
                <span className="signal-value">{Math.round(breakdown.experience_alignment * 100)}%</span>
                {reliability?.experience_alignment && <span className="signal-reliability">{reliability.experience_alignment}</span>}
              </div>
            )}
            {breakdown.tool_success_rate !== undefined && (
              <div className="signal-item">
                <span className="signal-label">Инструменты</span>
                <span className="signal-value">{Math.round(breakdown.tool_success_rate * 100)}%</span>
                {reliability?.tool_success_rate && <span className="signal-reliability">{reliability.tool_success_rate}</span>}
              </div>
            )}
            {breakdown.reflection_confidence !== undefined && (
              <div className="signal-item">
                <span className="signal-label">Рефлексия</span>
                <span className="signal-value">{Math.round(breakdown.reflection_confidence * 100)}%</span>
                {reliability?.reflection_confidence && <span className="signal-reliability">{reliability.reflection_confidence}</span>}
              </div>
            )}
          </div>
        </div>
      )}

      {missing_signals && missing_signals.length > 0 && (
        <div className="missing-signals">
          <h4>Отсутствующие сигналы</h4>
          <ul>
            {missing_signals.map((signal, i) => (
              <li key={i}>{signal}</li>
            ))}
          </ul>
        </div>
      )}

      {recommendation && (
        <div className="recommendation">
          <strong>Рекомендация:</strong> {recommendation}
        </div>
      )}
    </div>
  );
}
