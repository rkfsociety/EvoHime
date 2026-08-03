import { useState } from "react";
import { useModalA11y } from "../hooks/useModalA11y";
import "./ForceApproveModal.css";

export interface ForceApproveModalProps {
  riskLevel: "None" | "Low" | "Medium" | "High";
  confidence: number;
  onApprove: (reason: string) => void;
  onCancel: () => void;
}

export function ForceApproveModal({ riskLevel, confidence, onApprove, onCancel }: ForceApproveModalProps) {
  const [reason, setReason] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const modalRef = useModalA11y<HTMLDivElement>(true, onCancel);

  const handleApprove = () => {
    if (confirmed && reason.trim()) {
      onApprove(reason);
    }
  };

  return (
    <div className="force-approve-backdrop" role="presentation">
      <div ref={modalRef} className="force-approve-modal" role="dialog" aria-modal="true" aria-labelledby="force-approve-title" tabIndex={-1}>
        <h3 id="force-approve-title">⚠️ Подтверждение высокорискового действия</h3>

        <div className="warning-box">
          <p>
            Уровень риска: <strong>{riskLevel}</strong>
          </p>
          <p>
            Уверенность: <strong>{Math.round(confidence * 100)}%</strong>
          </p>
          <p>Это действие требует явного подтверждения из-за низкой уверенности системы и высокого уровня риска.</p>
        </div>

        <div className="reason-section">
          <label htmlFor="force-approve-reason">Причина для переопределения:</label>
          <textarea
            id="force-approve-reason"
            className="reason-input"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            placeholder="Объясните, почему вы переопределяете рекомендацию системы..."
            rows={4}
          />
        </div>

        <div className="confirmation-section">
          <label>
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(e) => setConfirmed(e.target.checked)}
              aria-label="Я понимаю риски и хочу продолжить"
            />
            Я понимаю риски и хочу продолжить
          </label>
        </div>

        <div className="modal-actions">
          <button type="button" className="cancel-button" onClick={onCancel} aria-label="Отменить">
            Отменить
          </button>
          <button
            type="button"
            className="force-approve-button"
            onClick={handleApprove}
            disabled={!confirmed || !reason.trim()}
            aria-label="Переопределить и продолжить"
          >
            Переопределить
          </button>
        </div>
      </div>
    </div>
  );
}
