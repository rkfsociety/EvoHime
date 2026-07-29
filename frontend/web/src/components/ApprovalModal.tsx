import type { ApprovalRequiredEvent } from "../protocol";
import { DiffViewer } from "./DiffViewer";
import { useModalA11y } from "../hooks/useModalA11y";
import { canRememberApprovalPath, isPatchReview } from "../lib/approval-review";

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

  if (!request) {
    return null;
  }

  const patchReview = isPatchReview(request);
  const canRememberPath = canRememberApprovalPath(request);

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
            onClick={() => onGrant(false)}
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
      </div>
    </div>
  );
}
