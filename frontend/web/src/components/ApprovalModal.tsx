import type { ApprovalRequiredEvent } from "../protocol";
import { useModalA11y } from "../hooks/useModalA11y";

export function ApprovalModal({
  request,
  onGrant,
  onDeny,
}: {
  request: ApprovalRequiredEvent;
  onGrant: () => void;
  onDeny: () => void;
}) {
  const modalRef = useModalA11y<HTMLDivElement>(true, onDeny);

  if (!request) {
    return null;
  }

  return (
    <div className="approvalBackdrop" role="presentation">
      <div
        ref={modalRef}
        className="approvalModal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="approval-modal-title"
        tabIndex={-1}
      >
        <h3 id="approval-modal-title">Требуется разрешение</h3>
        <p>
          Инструмент: <strong>{request.tool_name}</strong>
        </p>
        <p>
          Разрешение: <strong>{request.permission}</strong>
        </p>
        <p className="approvalScope">
          Область: <code>{request.scope}</code>
        </p>
        <div className="approvalActions">
          <button type="button" onClick={onDeny} aria-label="Запретить действие">
            Запретить
          </button>
          <button type="button" onClick={onGrant} aria-label="Разрешить действие">
            Разрешить
          </button>
        </div>
      </div>
    </div>
  );
}
