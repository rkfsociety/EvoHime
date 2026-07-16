import type { Dispatch, SetStateAction } from "react";
import type {
  ActionLoggedEvent,
  ApprovalRequiredEvent,
  PlanStep,
  ServerEvent,
  TaskStatusChangedEvent,
  TaskStepChangedEvent,
} from "../protocol";
import type { ActionView, ChatLine, ChatSessionSummary, TaskView } from "../types";
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
  setActions: Dispatch<SetStateAction<ActionView[]>>;
  setSelectedFileNotice: Dispatch<SetStateAction<string | null>>;
  setGitStatus: Dispatch<SetStateAction<string>>;
  setGitDiff: Dispatch<SetStateAction<string>>;
  refreshDirectory: (path: string) => Promise<void>;
  refreshSelectedFile: (path: string) => Promise<void>;
};

export function applyServerEvent(event: ServerEvent, ctx: ServerEventHandlerContext): void {
  switch (event.type) {
    case "session.created":
      ctx.setLines((current) => [
        ...current,
        {
          role: "system",
          text: `Сессия создана: ${event.session_id}`,
        },
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
        [event.task_id]: { id: event.task_id, message: event.user_message, status: "running", steps: {} },
      }));
      ctx.setLines((current) => [...current, { role: "user", text: event.user_message, taskId: event.task_id }]);
      ctx.setStream("");
      break;
    case "agent.message.delta":
      ctx.setStream((current) => {
        const next = `${current}${event.delta}`;
        ctx.setLines((items) => {
          const copy = [...items];
          const index = copy.findIndex((line) => line.role === "assistant" && line.taskId === event.task_id);
          if (index !== -1) {
            copy[index] = { role: "assistant", text: next, taskId: event.task_id };
            return copy;
          }
          copy.push({ role: "assistant", text: next, taskId: event.task_id });
          return copy;
        });
        return next;
      });
      break;
    case "tool.started":
      ctx.setLines((current) => [
        ...current,
        { role: "tool", text: `Запускаю инструмент: ${event.tool_name}` },
      ]);
      break;
    case "tool.output":
      if (event.tool_name === "shell.execute") {
        ctx.setTerminalEntries((current) => [...current, { stream: "stdout", text: event.output }]);
      }
      break;
    case "approval.required":
      ctx.setApproval(event);
      break;
    case "tool.completed":
      ctx.setLines((current) => [
        ...current,
        {
          role: "tool",
          text: `${event.tool_name}: ${event.success ? "завершён успешно" : "завершён с ошибкой"}`,
        },
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
          copy[index] = { role: "assistant", text: event.final_message, taskId: event.task_id };
          return copy;
        }
        copy.push({ role: "assistant", text: event.final_message, taskId: event.task_id });
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
        { role: "system", text: `Задача завершилась с ошибкой: ${event.error}` },
      ]);
      ctx.setStream("");
      break;
    case "task.status.changed": {
      const statusEvent = event as TaskStatusChangedEvent;
      ctx.setTasks((current) =>
        current[statusEvent.task_id]
          ? {
              ...current,
              [statusEvent.task_id]: { ...current[statusEvent.task_id], status: statusEvent.status },
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
                steps: { ...current[stepEvent.task_id].steps, [stepEvent.tool_name]: stepEvent.status },
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
      break;
    }
    case "agent.plan.updated":
      ctx.setLines((current) => [
        ...current,
        {
          role: "system",
          text: formatPlan(event.plan as PlanStep[]),
        },
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
