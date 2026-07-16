import type { ActionView } from "../types";

export function ActionsPanel({ actions }: { actions: ActionView[] }) {
  return (
    <div className="actionsPanel">
      {actions.map((action, index) => (
        <article className="actionItem" key={`${action.taskId}-${index}`}>
          <strong>{action.action}</strong>
          <span>{action.detail}</span>
          <small>{action.createdAt}</small>
        </article>
      ))}
    </div>
  );
}
