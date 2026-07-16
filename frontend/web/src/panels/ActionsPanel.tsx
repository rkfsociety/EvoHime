import type { ActionView } from "../types";

function actionLabel(action: string) {
  if (/approval/i.test(action)) return "Approval";
  if (/retry/i.test(action)) return "Retry";
  if (/recover|restart/i.test(action)) return "Recovery";
  if (/cancel/i.test(action)) return "Cancel";
  return action;
}

export function ActionsPanel({ actions }: { actions: ActionView[] }) {
  const retries = actions.filter((action) => /retry/i.test(action.action)).length;
  const recoveries = actions.filter((action) => /recover|restart/i.test(`${action.action} ${action.detail}`)).length;
  const approvals = actions.filter((action) => /approval/i.test(action.action)).length;

  return (
    <div className="actionsPanel actionsDeepPanel">
      <div className="panelToolbar">
        <div>
          <strong>Журнал действий</strong>
          <span>Последовательность runtime-событий без скрытой логики</span>
        </div>
        <div className="actionMetrics">
          <span>{actions.length} действий</span>
          <span>{retries} retries</span>
          <span>{recoveries} recovery</span>
          <span>{approvals} approvals</span>
        </div>
      </div>

      {actions.length === 0 ? (
        <div className="emptyPanelState">
          <strong>Действий пока нет</strong>
          <span>Здесь появятся retry, approval и recovery события агента.</span>
        </div>
      ) : (
        <div className="actionTimeline">
          {[...actions].reverse().map((action, index) => (
            <article className="actionItem actionTimelineItem" key={`${action.taskId}-${action.createdAt}-${index}`}>
              <div className="actionItemHeader">
                <strong>{actionLabel(action.action)}</strong>
                <time dateTime={action.createdAt}>{action.createdAt}</time>
              </div>
              <span className="actionTaskId">Task: {action.taskId}</span>
              <p>{action.detail}</p>
            </article>
          ))}
        </div>
      )}
    </div>
  );
}
