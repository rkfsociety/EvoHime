import type { Dispatch, SetStateAction } from "react";
import type {
  ActionLoggedEvent,
  ApprovalRequiredEvent,
  MemoryAskEvent,
  PlanStep,
  ServerEvent,
  TaskStatusChangedEvent,
  TaskStepChangedEvent,
} from "../protocol";
import type { ActionView, ChatLine, ChatSessionSummary, TaskStepView, TaskView } from "../types";
import { createChatLine } from "../lib/chat-lines";
import { formatPlan, summarizeChatTitle } from "../lib/format";
import { normalizePath, parentPath } from "../lib/paths";

/** Minimal terminal entry shape used by applyServerEvent (avoids coupling to TerminalPanel). */
export type TerminalStreamEntry = {
  stream: "stdout" | "stderr" | "status";
  text: string;
};

export type ServerEventHandlerContext = {
  selectedProjectPath: string;
  selectedFilePath: string | null;
  selectedFileContent: string;
  selectedFileOriginal: string;
  setLines: Dispatch<SetStateAction<ChatLine[]>>;
  setChatSessions: Dispatch<SetStateAction<ChatSessionSummary[]>>;
  setTasks: Dispatch<SetStateAction<Record<string, TaskView>>>;
  setStream: Dispatch<SetStateAction<string>>;
  setTerminalEntries: Dispatch<SetStateAction<TerminalStreamEntry[]>>;
  setApproval: Dispatch<SetStateAction<ApprovalRequiredEvent | null>>;
  setMemoryAsk: Dispatch<SetStateAction<MemoryAskEvent | null>>;
  setActions: Dispatch<SetStateAction<ActionView[]>>;
  setSelectedFileNotice: Dispatch<SetStateAction<string | null>>;
  setGitStatus: Dispatch<SetStateAction<string>>;
  setGitDiff: Dispatch<SetStateAction<string>>;
  refreshDirectory: (path: string) => Promise<void>;
  refreshSelectedFile: (path: string) => Promise<void>;
};

function updateStepStatus(
  steps: Record<string, TaskStepView>,
  event: TaskStepChangedEvent,
): Record<string, TaskStepView> {
  const entries = Object.entries(steps);
  const matching = entries.find(([, step]) => step.runtimeId === event.step_id)
    ?? entries.find(([, step]) => step.toolName === event.tool_name && step.status !== "completed" && step.status !== "failed");
  if (!matching) {
    return {
      ...steps,
      [event.step_id]: {
        id: event.step_id,
        runtimeId: event.step_id,
        toolName: event.tool_name,
        description: "",
        dependsOn: [],
        status: event.status,
      },
    };
  }
  const [key, step] = matching;
  return { ...steps, [key]: { ...step, runtimeId: event.step_id, status: event.status } };
}

export function applyServerEvent(event: ServerEvent, ctx: ServerEventHandlerContext): void {
  switch (event.type) {
    case "session.created":
      ctx.setLines((current) => [
        ...current,
        createChatLine({
          role: "system",
          text: `Сессия создана: ${event.session_id}`,
        }),
      ]);
      break;
    case "task.started":
      ctx.setChatSessions((current) => {
        const existing = current.find((chat) => chat.session_id === event.session_id);
        const summary: ChatSessionSummary = {
          session_id: event.session_id,
          created_at: existing?.created_at ?? event.created_at,
          title: existing?.title || summarizeChatTitle(event.user_message),
          workspace_path: existing?.workspace_path ?? ctx.selectedProjectPath,
          last_message: event.user_message,
          last_message_at: event.created_at,
          last_role: "user",
        };
        return [summary, ...current.filter((chat) => chat.session_id !== event.session_id)];
      });
      ctx.setTasks((current) => ({
        ...current,
        [event.task_id]: {
          id: event.task_id,
          message: event.user_message,
          status: "running",
          steps: {},
          retryCount: 0,
          pauseReason: null,
          approvalWait: null,
          recovery: null,
          plan: [],
          pendingPlan: null,
        },
      }));
      ctx.setLines((current) => [...current, createChatLine({ role: "user", text: event.user_message, taskId: event.task_id })]);
      ctx.setStream("");
      break;
    case "agent.status": {
      const labels: Record<typeof event.phase, string> = {
        selecting_action: "Модель выбирает действие",
        executing_tools: "Выполняю инструмент",
        waiting_approval: "Ожидаю подтверждение",
        responding: "Формирую ответ",
        stopped: "Выполнение остановлено",
      };
      ctx.setLines((current) => [
        ...current,
        createChatLine({ role: "system", text: labels[event.phase], taskId: event.task_id }),
      ]);
      break;
    }
    case "agent.message.delta":
      ctx.setStream((current) => {
        const next = `${current}${event.delta}`;
        ctx.setLines((items) => {
          const copy = [...items];
          const index = copy.findIndex((line) => line.role === "assistant" && line.taskId === event.task_id);
          if (index !== -1) {
            copy[index] = { ...copy[index], text: next };
            return copy;
          }
          copy.push(createChatLine({ role: "assistant", text: next, taskId: event.task_id }));
          return copy;
        });
        return next;
      });
      break;
    case "tool.started":
      ctx.setLines((current) => [
        ...current,
        createChatLine({ role: "tool", text: `Запускаю инструмент: ${event.tool_name}`, taskId: event.task_id }),
      ]);
      break;
    case "tool.output":
      // Live chunks arrive via tool.output.delta; keep final shell blob out of the terminal
      // to avoid duplicating streamed stdout/stderr.
      break;
    case "tool.output.delta": {
      const stream = event.stream === "stderr" ? "stderr" : "stdout";
      ctx.setTerminalEntries((current) => [...current, { stream, text: event.delta }]);
      break;
    }
    case "approval.required":
      ctx.setApproval(event);
      ctx.setTasks((current) => {
        const task = current[event.task_id];
        if (!task) {
          return current;
        }
        return {
          ...current,
          [event.task_id]: {
            ...task,
            status: "paused",
            pauseReason: "approval_required",
            approvalWait: {
              approvalId: event.approval_id,
              toolName: event.tool_name,
              permission: event.permission,
              scope: event.scope,
            },
          },
        };
      });
      break;
    case "memory.ask":
      ctx.setMemoryAsk(event);
      ctx.setLines((current) => [
        ...current,
        createChatLine({
          role: "system",
          text: `Память: запомнить «${event.content.slice(0, 120)}${event.content.length > 120 ? "…" : ""}»? (${event.reason})`,
          taskId: event.task_id,
        }),
      ]);
      break;
    case "memory.proposed":
      ctx.setLines((current) => [
        ...current,
        createChatLine({
          role: "system",
          text: `Память предложена [${event.scope}/${event.kind}]: ${event.content.slice(0, 100)}${event.content.length > 100 ? "…" : ""}`,
          taskId: event.task_id,
        }),
      ]);
      break;
    case "memory.accepted":
      ctx.setMemoryAsk((current) =>
        current?.memory_id === event.memory_id ? null : current,
      );
      ctx.setLines((current) => [
        ...current,
        createChatLine({ role: "system", text: `Память сохранена: ${event.memory_id}`, taskId: event.task_id }),
      ]);
      break;
    case "memory.rejected":
      ctx.setMemoryAsk((current) =>
        current?.memory_id === event.memory_id ? null : current,
      );
      ctx.setLines((current) => [
        ...current,
        createChatLine({ role: "system", text: `Память отклонена: ${event.memory_id}`, taskId: event.task_id }),
      ]);
      break;
    case "memory.used":
      ctx.setLines((current) => [
        ...current,
        createChatLine({
          role: "system",
          text: `Память feedback [${event.signal}]: ${event.memory_id} → conf ${event.confidence.toFixed(2)}`,
          taskId: event.task_id,
        }),
      ]);
      break;
    case "tool.completed":
      ctx.setLines((current) => [
        ...current,
        createChatLine({
          role: "tool",
          text: `${event.tool_name}: ${event.success ? "завершён успешно" : "завершён с ошибкой"}`,
          taskId: event.task_id,
        }),
      ]);
      if (event.tool_name === "shell.execute") {
        ctx.setTerminalEntries((current) => [
          ...current,
          {
            stream: event.success ? "status" : "stderr",
            text: event.success ? "shell.execute выполнен" : "shell.execute завершился с ошибкой",
          },
        ]);
      }
      break;
    case "task.completed":
      ctx.setTasks((current) =>
        current[event.task_id]
          ? { ...current, [event.task_id]: { ...current[event.task_id], status: "completed" } }
          : current,
      );
      ctx.setLines((current) => {
        const copy = [...current];
        const index = copy.findIndex((line) => line.role === "assistant" && line.taskId === event.task_id);
        if (index !== -1) {
          copy[index] = { ...copy[index], text: event.final_message };
          return copy;
        }
        copy.push(createChatLine({ role: "assistant", text: event.final_message, taskId: event.task_id }));
        return copy;
      });
      ctx.setStream("");
      break;
    case "task.failed":
      ctx.setTasks((current) =>
        current[event.task_id]
          ? { ...current, [event.task_id]: { ...current[event.task_id], status: "failed" } }
          : current,
      );
      ctx.setLines((current) => [
        ...current,
        createChatLine({ role: "system", text: `Задача завершилась с ошибкой: ${event.error}`, taskId: event.task_id }),
      ]);
      ctx.setStream("");
      break;
    case "task.status.changed": {
      const statusEvent = event as TaskStatusChangedEvent;
      ctx.setTasks((current) =>
        current[statusEvent.task_id]
          ? {
              ...current,
              [statusEvent.task_id]: {
                ...current[statusEvent.task_id],
                status: statusEvent.status,
                pauseReason:
                  statusEvent.status === "paused"
                    ? current[statusEvent.task_id].pauseReason ?? "paused"
                    : statusEvent.status === "running"
                      ? null
                      : current[statusEvent.task_id].pauseReason,
                approvalWait: statusEvent.status === "running" ? null : current[statusEvent.task_id].approvalWait,
                pendingPlan: statusEvent.status === "running" ? null : current[statusEvent.task_id].pendingPlan,
              },
            }
          : current,
      );
      break;
    }
    case "task.step.changed": {
      const stepEvent = event as TaskStepChangedEvent;
      ctx.setTasks((current) =>
        current[stepEvent.task_id]
          ? {
              ...current,
              [stepEvent.task_id]: {
                ...current[stepEvent.task_id],
                steps: updateStepStatus(current[stepEvent.task_id].steps, stepEvent),
              },
            }
          : current,
      );
      break;
    }
    case "action.logged": {
      const actionEvent = event as ActionLoggedEvent;
      ctx.setActions((current) => [
        ...current,
        {
          taskId: actionEvent.task_id,
          action: actionEvent.action,
          detail: actionEvent.detail,
          createdAt: actionEvent.created_at,
        },
      ]);
      ctx.setTasks((current) => {
        const task = current[actionEvent.task_id];
        if (!task) {
          return current;
        }
        const isRetry = actionEvent.action === "task.retry";
        const isRecovery = /recover|restart/i.test(`${actionEvent.action} ${actionEvent.detail}`);
        const isPlanApprovalRequired = actionEvent.action === "plan.approval.required";
        const isPlanApprovalResolved = actionEvent.action === "plan.approval.granted"
          || actionEvent.action === "plan.approval.rejected";
        return {
          ...current,
          [actionEvent.task_id]: {
            ...task,
            retryCount: task.retryCount + (isRetry ? 1 : 0),
            recovery: isRecovery ? actionEvent.detail : task.recovery,
            pauseReason: isPlanApprovalRequired ? "plan_approval_required" : task.pauseReason,
            pendingPlan: isPlanApprovalRequired ? task.plan : isPlanApprovalResolved ? null : task.pendingPlan,
          },
        };
      });
      break;
    }
    case "agent.plan.updated":
      ctx.setTasks((current) => {
        const task = current[event.task_id];
        if (!task) {
          return current;
        }
        const steps = { ...task.steps };
        for (const step of event.plan as PlanStep[]) {
          const previous = steps[step.id];
          steps[step.id] = {
            id: step.id,
            runtimeId: previous?.runtimeId,
            toolName: step.tool_name,
            description: step.description,
            dependsOn: step.depends_on ?? [],
            status: previous?.status ?? "pending",
          };
        }
        return {
          ...current,
          [event.task_id]: { ...task, steps, plan: event.plan as PlanStep[] },
        };
      });
      ctx.setLines((current) => [
        ...current,
        createChatLine({
          role: "system",
          text: formatPlan(event.plan as PlanStep[]),
          taskId: event.task_id,
        }),
      ]);
      break;
    case "file.changed":
      void ctx.refreshDirectory(".").catch(() => undefined);
      void ctx.refreshDirectory(parentPath(event.path)).catch(() => undefined);
      if (normalizePath(ctx.selectedFilePath ?? undefined) === normalizePath(event.path)) {
        if (ctx.selectedFileContent !== ctx.selectedFileOriginal) {
          ctx.setSelectedFileNotice(
            "Файл изменился на диске. Сохрани или перезагрузи, чтобы не потерять правки.",
          );
        } else {
          void ctx.refreshSelectedFile(event.path).catch(() => undefined);
        }
      }
      break;
    case "git.diff.changed":
      ctx.setGitStatus(event.status);
      ctx.setGitDiff(event.diff);
      break;
  }
}
