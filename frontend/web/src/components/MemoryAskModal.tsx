import type { MemoryAskEvent } from "../protocol";

export function MemoryAskModal({
  request,
  onAccept,
  onReject,
}: {
  request: MemoryAskEvent;
  onAccept: () => void;
  onReject: () => void;
}) {
  return (
    <div className="approvalBackdrop" role="dialog" aria-modal="true">
      <div className="approvalModal">
        <h3>Запомнить?</h3>
        <p className="approvalScope">
          {request.scope} / {request.kind} · confidence {request.confidence.toFixed(2)}
        </p>
        <p>{request.content}</p>
        <p className="approvalScope">Причина: {request.reason}</p>
        <div className="approvalActions">
          <button type="button" onClick={onReject}>
            Отклонить
          </button>
          <button type="button" onClick={onAccept}>
            Запомнить
          </button>
        </div>
      </div>
    </div>
  );
}
