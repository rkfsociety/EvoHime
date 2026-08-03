import React, { useState } from 'react';
import './ForceApproveModal.css';

export interface ForceApproveModalProps {
  isOpen: boolean;
  riskLevel: 'none' | 'low' | 'medium' | 'high';
  confidenceScore: number;
  reason?: string;
  onApprove: (reason: string) => void;
  onCancel: () => void;
}

export const ForceApproveModal: React.FC<ForceApproveModalProps> = ({
  isOpen,
  riskLevel,
  confidenceScore,
  reason,
  onApprove,
  onCancel,
}) => {
  const [comment, setComment] = useState('');
  const [confirmed, setConfirmed] = useState(false);

  if (!isOpen) return null;

  const handleApprove = () => {
    if (!confirmed || !comment.trim()) return;
    onApprove(comment);
    setComment('');
    setConfirmed(false);
  };

  const handleCancel = () => {
    setComment('');
    setConfirmed(false);
    onCancel();
  };

  const confidencePercent = Math.round(confidenceScore * 100);

  return (
    <div className="force-approve-modal-overlay">
      <div className="force-approve-modal">
        <div className="modal-header">
          <h3>High-Risk Operation - Force Approve</h3>
          <button className="close-btn" onClick={handleCancel}>×</button>
        </div>

        <div className="modal-content">
          <div className="risk-info">
            <div className="risk-item">
              <span className="risk-label">Risk Level:</span>
              <span className={`risk-badge risk-${riskLevel}`}>
                {riskLevel.toUpperCase()}
              </span>
            </div>
            <div className="risk-item">
              <span className="risk-label">Confidence:</span>
              <span className="confidence-value">{confidencePercent}%</span>
            </div>
            {reason && (
              <div className="risk-reason">
                <span className="reason-label">Reason for ask:</span>
                <p>{reason}</p>
              </div>
            )}
          </div>

          <div className="warning-box">
            <p>
              ⚠️ This operation involves high-risk changes (e.g., git push, destructive shell commands, or risky file modifications).
              The agent has lower confidence in the plan. Forcing approval bypasses safety checks.
            </p>
          </div>

          <div className="comment-section">
            <label htmlFor="approve-reason">
              Reason for force approval <span className="required">*</span>
            </label>
            <textarea
              id="approve-reason"
              value={comment}
              onChange={(e) => setComment(e.target.value)}
              placeholder="Explain why you're overriding the safety checks..."
              rows={4}
              className="comment-input"
            />
            <div className="char-count">
              {comment.length} / 500 characters
            </div>
          </div>

          <div className="confirmation-section">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={confirmed}
                onChange={(e) => setConfirmed(e.target.checked)}
                className="checkbox-input"
              />
              <span>I understand the risks and take full responsibility for this action</span>
            </label>
          </div>
        </div>

        <div className="modal-footer">
          <button
            className="btn btn-cancel"
            onClick={handleCancel}
          >
            Cancel
          </button>
          <button
            className="btn btn-force-approve"
            onClick={handleApprove}
            disabled={!confirmed || !comment.trim()}
            title={!confirmed || !comment.trim() ? 'Confirm and provide reason' : 'Force approve operation'}
          >
            Force Approve
          </button>
        </div>
      </div>
    </div>
  );
};
