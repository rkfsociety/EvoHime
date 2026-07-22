import type { ChatSessionSummary, TaskView } from "../types";
import type { PlanStep } from "../protocol";
import { useEffect, useState } from "react";
import { CopyButton } from "../components/CopyButton";
import {
  formatSessionPreview,
  formatSessionTimestamp,
  formatSessionTitle,
} from "../lib/format";

type TasksPanelProps = {
  tasks: Record<string, TaskView>;
  chatSessions: ChatSessionSummary[];
  activeSessionId: string | null;
  onNewChat: () => void;
  onOpenSession: (chat: ChatSessionSummary) => void;
  onApprovePlan: (taskId: string, plan: PlanStep[]) => void;
  onRejectPlan: (taskId: string) => void;
};

function PlanApprovalEditor({
  task,
  onApprove,
  onReject,
}: {
  task: TaskView;
  onApprove: (plan: PlanStep[]) => void;
  onReject: () => void;
}) {
  const [draft, setDraft] = useState<PlanStep[]>(task.pendingPlan ?? task.plan);

  useEffect(() => {
    setDraft(task.pendingPlan ?? task.plan);
  }, [task.pendingPlan, task.plan]);

  function updateStep(index: number, patch: Partial<PlanStep>) {
    setDraft((current) => current.map((step, stepIndex) => stepIndex === index ? { ...step, ...patch } : step));
  }

  return (
    <div className="taskPlanApproval">
      <strong>План ждёт подтверждения</strong>
      <span>Проверь шаги до запуска инструментов.</span>
      {draft.map((step, index) => (
        <div className="taskPlanEditRow" key={step.id}>
          <label>
            <span>{step.id} · инструмент</span>
            <input value={step.tool_name} onChange={(event) => updateStep(index, { tool_name: event.target.value })} />
          </label>
          <label>
            <span>Описание</span>
            <input value={step.description} onChange={(event) => updateStep(index, { description: event.target.value })} />
          </label>
          <label>
            <span>Зависимости через запятую</span>
            <input
              value={(step.depends_on ?? []).join(", ")}
              onChange={(event) => updateStep(index, {
                depends_on: event.target.value.split(",").map((value) => value.trim()).filter(Boolean),
              })}
            />
          </label>
        </div>
      ))}
      <div className="taskPlanApprovalActions">
        <button type="button" onClick={() => onApprove(draft)}>Запустить план</button>
        <button type="button" onClick={onReject}>Отклонить</button>
      </div>
    </div>
  );
}

function statusLabel(status: string) {
  const labels: Record<string, string> = {
    running: "Выполняется",
    completed: "Завершена",
    failed: "Ошибка",
    paused: "Пауза",
    retrying: "Повторная попытка",
    cancelling: "Отмена",
    cancelled: "Отменена",
  };
  return labels[status] ?? status;
}

export function TasksPanel({
  tasks,
  chatSessions,
  activeSessionId,
  onNewChat,
  onOpenSession,
  onApprovePlan,
  onRejectPlan,
}: TasksPanelProps) {
  const taskList = Object.values(tasks).reverse();
  const maxDuration = Math.max(...taskList.map((task) => task.durationMs ?? 0), 1);

  return (
    <div className="standaloneChatsPanel tasksDeepPanel">
      <div className="panelToolbar">
        <div>
          <strong>Задачи агента</strong>
          <span>План, зависимости, паузы, retries и восстановление из событий runtime</span>
        </div>
        <span>{taskList.length} задач</span>
      </div>

      {taskList.length > 0 ? (
        <div className="taskDetailList">
          {taskList.map((task) => {
            const steps = Object.values(task.steps);
            const completed = steps.filter((step) => step.status === "completed").length;
            return (
              <article className="taskDetailCard" key={task.id}>
                <div className="taskDetailHeader">
                  <div>
                    <span className={`taskStatus taskStatus-${task.status}`}>{statusLabel(task.status)}</span>
                    <h3>{task.message}</h3>
                    <small className="taskCorrelationRow">{task.id}<CopyButton value={task.id} /></small>
                  </div>
                  <div className="taskDetailMetrics">
                    <strong>{completed}/{steps.length}</strong>
                    <span>шагов</span>
                    <strong>{task.retryCount}</strong>
                    <span>retries</span>
                  </div>
                </div>
                <div className="latencyMetric" aria-label={task.durationMs == null ? "Длительность пока недоступна" : `Длительность ${task.durationMs} миллисекунд`}>
                  <div className="latencyMetricHeader"><span>Latency</span><strong>{task.durationMs == null ? "—" : `${task.durationMs} ms`}</strong></div>
                  <div className="latencyTrack"><span style={{ width: `${task.durationMs == null ? 0 : Math.max(4, (task.durationMs / maxDuration) * 100)}%` }} /></div>
                </div>

                {task.pauseReason ? (
                  <div className="taskNotice">
                    <strong>Причина паузы:</strong> {task.pauseReason}
                    {task.approvalWait ? (
                      <span>
                        Ожидание approval для {task.approvalWait.toolName} · {task.approvalWait.permission}
                      </span>
                    ) : null}
                  </div>
                ) : null}
                {task.recovery ? (
                  <div className="taskNotice">
                    <strong>Recovery:</strong> {task.recovery}
                  </div>
                ) : null}

                {task.pendingPlan && task.pauseReason === "plan_approval_required" ? (
                  <PlanApprovalEditor
                    task={task}
                    onApprove={(plan) => onApprovePlan(task.id, plan)}
                    onReject={() => onRejectPlan(task.id)}
                  />
                ) : null}

                <div className="taskStepList">
                  {steps.length === 0 ? <span className="taskMuted">План ещё не опубликован.</span> : null}
                  {steps.map((step) => (
                    <div className="taskStepRow" key={step.id}>
                      <span className={`taskStepStatus taskStepStatus-${step.status}`}>{statusLabel(step.status)}</span>
                      <div>
                        <strong>{step.id} · {step.toolName}</strong>
                        {step.description ? <span>{step.description}</span> : null}
                        <small>
                          {step.dependsOn.length > 0 ? `Зависит от: ${step.dependsOn.join(", ")}` : "Независимый шаг"}
                        </small>
                      </div>
                    </div>
                  ))}
                </div>
              </article>
            );
          })}
        </div>
      ) : (
        <div className="emptyPanelState">
          <strong>Активных задач пока нет</strong>
          <span>План и recovery-состояния появятся после запуска задачи.</span>
        </div>
      )}

      <div className="panelToolbar">
        <div>
          <strong>Чаты без проектов</strong>
          <span>Личные задачи и разговоры, не привязанные к проекту</span>
        </div>
        <button type="button" onClick={onNewChat}>Новый чат</button>
      </div>
      {chatSessions.length > 0 ? (
        <div className="standaloneChatList">
          {chatSessions.map((chat, index) => (
            <button
              key={chat.session_id}
              type="button"
              className={chat.session_id === activeSessionId ? "standaloneChatItem active" : "standaloneChatItem"}
              onClick={() => onOpenSession(chat)}
            >
              <span className="standaloneChatIcon">⊕</span>
              <span className="standaloneChatDetails">
                <strong>{formatSessionTitle(chat, index)}</strong>
                <span>{formatSessionPreview(chat)}</span>
              </span>
              <time dateTime={chat.last_message_at ?? chat.created_at}>
                {formatSessionTimestamp(chat.last_message_at ?? chat.created_at)}
              </time>
            </button>
          ))}
        </div>
      ) : (
        <div className="emptyPanelState">
          <strong>Пока нет чатов без проекта</strong>
          <span>Создай новый чат, и он появится здесь.</span>
        </div>
      )}
    </div>
  );
}
