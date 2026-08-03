import React from 'react';
import './ConfidenceAndRisk.css';

export interface SignalBreakdown {
  score: number;
  reliability: string;
  source?: string;
  details?: string;
}

export interface ConfidenceAndRiskProps {
  confidenceScore: number; // [0.0, 1.0]
  riskLevel: 'none' | 'low' | 'medium' | 'high';
  breakdown: {
    model: SignalBreakdown;
    experience: SignalBreakdown;
    tools: SignalBreakdown;
    reflection: SignalBreakdown;
  };
  reliability: Record<string, string>;
  missingSignals: string[];
  recommendation: 'proceed' | 'ask' | 'require_approval';
}

export const ConfidenceAndRisk: React.FC<ConfidenceAndRiskProps> = ({
  confidenceScore,
  riskLevel,
  breakdown,
  reliability,
  missingSignals,
  recommendation,
}) => {
  const confidencePercent = Math.round(confidenceScore * 100);
  const getRiskColor = (risk: string) => {
    switch (risk) {
      case 'none': return '#4CAF50';
      case 'low': return '#8BC34A';
      case 'medium': return '#FF9800';
      case 'high': return '#F44336';
      default: return '#999';
    }
  };

  const getRiskLabel = (risk: string) => {
    switch (risk) {
      case 'none': return '✓ Safe';
      case 'low': return '⚠ Low Risk';
      case 'medium': return '⚠ Medium Risk';
      case 'high': return '🔴 High Risk';
      default: return 'Unknown';
    }
  };

  return (
    <div className="confidence-and-risk">
      <div className="confidence-section">
        <div className="confidence-bar-container">
          <div className="confidence-label">Confidence: {confidencePercent}%</div>
          <div className="confidence-bar">
            <div
              className="confidence-fill"
              style={{
                width: `${confidencePercent}%`,
                backgroundColor: confidencePercent >= 75 ? '#4CAF50' : confidencePercent >= 50 ? '#FF9800' : '#F44336',
              }}
            />
          </div>
        </div>

        <div className="breakdown-grid">
          <div className="breakdown-item">
            <div className="signal-name">Model</div>
            <div className="signal-score">{Math.round(breakdown.model.score * 100)}%</div>
            <div className="signal-reliability">({breakdown.model.reliability})</div>
          </div>
          <div className="breakdown-item">
            <div className="signal-name">Experience</div>
            <div className="signal-score">{Math.round(breakdown.experience.score * 100)}%</div>
            <div className="signal-reliability">({breakdown.experience.reliability})</div>
          </div>
          <div className="breakdown-item">
            <div className="signal-name">Tools</div>
            <div className="signal-score">{Math.round(breakdown.tools.score * 100)}%</div>
            <div className="signal-reliability">({breakdown.tools.reliability})</div>
          </div>
          <div className="breakdown-item">
            <div className="signal-name">Reflection</div>
            <div className="signal-score">{Math.round(breakdown.reflection.score * 100)}%</div>
            <div className="signal-reliability">({breakdown.reflection.reliability})</div>
          </div>
        </div>
      </div>

      <div className="risk-section">
        <div className="risk-badge" style={{ borderColor: getRiskColor(riskLevel) }}>
          {getRiskLabel(riskLevel)}
        </div>
      </div>

      {missingSignals.length > 0 && (
        <div className="missing-signals">
          <div className="missing-label">⚠ Missing signals:</div>
          <div className="missing-list">
            {missingSignals.map((signal) => (
              <span key={signal} className="missing-item">
                {signal}
              </span>
            ))}
          </div>
        </div>
      )}

      <div className="recommendation">
        <div className="rec-label">Recommendation:</div>
        <div className={`rec-value rec-${recommendation}`}>{recommendation.toUpperCase()}</div>
      </div>
    </div>
  );
};
