import type { ApprovalRequiredEvent } from "../protocol";

export function ApprovalModal({ request, onGrant, onDeny }: { request: ApprovalRequiredEvent; onGrant: () => void; onDeny: () => void }) {
  if (!request) return null;
  return <div className="approvalBackdrop" role="dialog" aria-modal="true"><div className="approvalModal"><h3>Требуется разрешение</h3><p>Инструмент: <strong>{request.tool_name}</strong></p><p>Разрешение: <strong>{request.permission}</strong></p><p className="approvalScope">Область: <code>{request.scope}</code></p><div className="approvalActions"><button type="button" onClick={onDeny}>Запретить</button><button type="button" onClick={onGrant}>Разрешить</button></div></div></div>;
}
