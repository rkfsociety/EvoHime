import { useEffect, useState } from "react";
import type { ApprovalRequiredEvent } from "../protocol";
import { DiffViewer } from "./DiffViewer";
import { useModalA11y } from "../hooks/useModalA11y";
import { canRememberApprovalPath, isPatchReview } from "../lib/approval-review";
import { ConfidenceAndRisk } from "./ConfidenceAndRisk";
import { ForceApproveModal } from "./ForceApproveModal";

export function ApprovalModal({
  request,
  onGrant,
  onDeny,
}: {
  request: ApprovalRequiredEvent;
  onGrant: (rememberPath: boolean) => void;
  onDeny: () => void;
}) {
  const modalRef = useModalA11y<HTMLDivElement>(true, onDeny);
  const [confidenceData, setConfidenceData] = useState<any>(null);
  const [showForceApprove, setShowForceApprove] = useState(false);

  useEffect(() => {
    if (!request?.task_id) return;
    fetch(`/api/confidence/audit/task/${request.task_id}`)
      .then((res) => res.json())
      .then((data) => {
        if (Array.isArray(data) && data.length > 0) {
          setConfidenceData(data[0]);
        }
      })
      .catch(() => {
        // Silently fail if confidence data not available
      });
  }, [request?.task_id]);

  if (!request) {
    return null;
  }

  const patchReview = isPatchReview(request);
  const canRememberPath = canRememberApprovalPath(request);
  const isHighRisk = confidenceData?.risk_level === "High";

  return (
    <div className="approvalBackdrop" role="presentation">
      <div
        ref={modalRef}
        className={`approvalModal${patchReview ? " patchReviewModal" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="approval-modal-title"
        tabIndex={-1}
      >
        <h3 id="approval-modal-title">{patchReview ? "Проверка патча" : "Требуется разрешение"}</h3>
        {patchReview ? (
          <>
            <p className="approvalScope">
              Файл: <code>{request.review.path}</code>
            </p>
            <DiffViewer diff={request.review.diff} emptyText="Пустой патч" />
          </>
        ) : (
          <>
            <p>
              Инструмент: <strong>{request.tool_name}</strong>
            </p>
            <p>
              Разрешение: <strong>{request.permission}</strong>
            </p>
            <p className="approvalScope">
              Область: <code>{request.scope}</code>
            </p>
            {confidenceData ? (
              <div style={{ marginTop: "1rem", padding: "1rem", borderTop: "1px solid var(--border)" }}>
                <ConfidenceAndRisk
                  confidence_score={confidenceData.confidence_score}
                  risk_level={confidenceData.risk_level}
                  breakdown={confidenceData.breakdown}
                  reliability={confidenceData.reliability}
                  missing_signals={confidenceData.missing_signals}
                  recommendation={confidenceData.recommendation}
                />
              </div>
            ) : null}
            {canRememberPath ? (
              <p className="approvalHint">
                Можно разрешить один раз или запомнить путь на 1 час для текущей сессии.
              </p>
            ) : (
              <p className="approvalHint">Эту область нельзя запомнить — только разовое разрешение.</p>
            )}
          </>
        )}
        <div className="approvalActions">
          <button type="button" className="approvalDenyButton" onClick={onDeny} aria-label="Отклонить действие">
            {patchReview ? "Отклонить" : "Запретить"}
          </button>
          <button
            type="button"
            className="approvalGrantButton"
            onClick={() => {
              if (isHighRisk && confidenceData?.confidence_score < 0.65) {
                setShowForceApprove(true);
              } else {
                onGrant(false);
              }
            }}
            aria-label={patchReview ? "Применить патч" : "Разрешить один раз"}
          >
            {patchReview ? "Применить" : "Один раз"}
          </button>
          {canRememberPath ? (
            <button
              type="button"
              className="approvalRememberButton"
              onClick={() => onGrant(true)}
              aria-label="Разрешить и запомнить путь на 1 час"
            >
              Запомнить путь (1 ч)
            </button>
          ) : null}
        </div>
        {showForceApprove && confidenceData ? (
          <ForceApproveModal
            riskLevel={confidenceData.risk_level}
            confidence={confidenceData.confidence_score}
            onApprove={(reason) => {
              setShowForceApprove(false);
              onGrant(false);
            }}
            onCancel={() => setShowForceApprove(false)}
          />
        ) : null}
      </div>
    </div>
  );
}
