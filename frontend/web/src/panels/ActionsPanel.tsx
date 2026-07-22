import type { ActionView } from "../types";
import { CopyButton } from "../components/CopyButton";
import { translateActionDetail, translateActionLabel } from "../lib/format";

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
          <span>{retries} повторов</span>
          <span>{recoveries} восстановлений</span>
          <span>{approvals} подтверждений</span>
        </div>
      </div>

      {actions.length === 0 ? (
        <div className="emptyPanelState">
          <strong>Действий пока нет</strong>
          <span>Здесь появятся повторы, подтверждения и события восстановления агента.</span>
        </div>
      ) : (
        <div className="actionTimeline">
          {[...actions].reverse().map((action, index) => (
            <article className="actionItem actionTimelineItem" key={`${action.taskId}-${action.createdAt}-${index}`}>
              <div className="actionItemHeader">
                <strong>{translateActionLabel(action.action)}</strong>
                <time dateTime={action.createdAt}>{action.createdAt}</time>
              </div>
              <span className="actionTaskId">Задача: {action.taskId} <CopyButton value={action.correlationId} /></span>
              {action.durationMs != null ? (
                <div className="latencyMetric" aria-label={`Длительность ${action.durationMs} миллисекунд`}>
                  <div className="latencyMetricHeader"><span>Latency</span><strong>{action.durationMs} ms</strong></div>
                  <div className="latencyTrack"><span style={{ width: "100%" }} /></div>
                </div>
              ) : null}
              <p>{translateActionDetail(action.detail)}</p>
            </article>
          ))}
        </div>
      )}
    </div>
  );
}
