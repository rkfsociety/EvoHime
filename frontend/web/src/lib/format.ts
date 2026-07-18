import type {
  ChatLine,
  ChatSessionSummary,
  GitAction,
  PermissionMode,
  ProjectSelection,
} from "../types";
import { normalizePath } from "./paths";

export function summarizeGitStatus(status: string) {
  const lines = status.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  const branch = lines[0] ?? "Нет статуса";
  const changed = lines.filter((line) => !line.startsWith("##")).length;
  return {
    branch,
    changed,
    lines,
  };
}

export function translateSocketState(
  state: "idle" | "connecting" | "reconnecting" | "connected" | "failed",
) {
  switch (state) {
    case "idle":
      return "Ожидание";
    case "connecting":
      return "Подключение";
    case "reconnecting":
      return "Переподключение";
    case "connected":
      return "Подключено";
    case "failed":
      return "Ошибка подключения";
  }
}

export function translateTaskStatus(status: string) {
  switch (status) {
    case "running":
      return "Выполняется";
    case "cancelling":
      return "Отмена";
    case "paused":
      return "На паузе";
    case "cancelled":
      return "Отменена";
    case "failed":
      return "Сбой";
    case "completed":
      return "Завершена";
    default:
      return status;
  }
}

export function translateStepStatus(status: string) {
  switch (status) {
    case "pending":
      return "Ожидание";
    case "running":
      return "Выполняется";
    case "completed":
      return "Завершён";
    case "failed":
      return "Сбой";
    case "cancelled":
      return "Отменён";
    default:
      return status;
  }
}

export function translatePermissionMode(mode: PermissionMode) {
  switch (mode) {
    case "ask":
      return "спрашивать";
    case "allow":
      return "разрешать";
    case "deny":
      return "запрещать";
  }
}

export function translateSaveState(state: "idle" | "saving" | "saved") {
  switch (state) {
    case "idle":
      return "Готово";
    case "saving":
      return "Сохранение...";
    case "saved":
      return "Сохранено";
  }
}

export function translateGitAction(action: GitAction) {
  switch (action) {
    case "commit":
      return "коммит";
    case "pull":
      return "загрузка";
    case "push":
      return "отправка";
  }
}

export function translateChatRole(role: ChatLine["role"], userLogin?: string | null) {
  switch (role) {
    case "assistant":
      return "EvoHime";
    case "tool":
      return "Действие";
    case "system":
      return "Ход работы";
    case "user":
      return userLogin?.trim() || "Пользователь";
  }
}

/** Visible plain text from chat markdown (no fences, emphasis, link markup). */
export function markdownToPlainText(input: string) {
  let text = input.replace(/\r\n?/g, "\n");

  text = text.replace(/```[^\n]*\n?([\s\S]*?)```/g, (_match, code: string) => code.replace(/\n$/, ""));
  text = text.replace(/`([^`]+)`/g, "$1");
  text = text.replace(/!\[([^\]]*)\]\([^)]+\)/g, "$1");
  text = text.replace(/\[([^\]]+)\]\([^)]+\)/g, "$1");
  text = text.replace(/^#{1,6}\s+/gm, "");
  text = text.replace(/^\s{0,3}>\s?/gm, "");
  text = text.replace(/^\s*[-*+]\s+/gm, "");
  text = text.replace(/^\s*\d+\.\s+/gm, "");
  text = text.replace(/(\*\*|__)(.*?)\1/g, "$2");
  text = text.replace(/(\*|_)(.*?)\1/g, "$2");
  text = text.replace(/~~(.*?)~~/g, "$1");
  text = text.replace(/<\/?[^>]+>/g, "");
  text = text.replace(/&nbsp;/g, " ").replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"');
  text = text.replace(/[ \t]+\n/g, "\n").replace(/\n{3,}/g, "\n\n");

  return text.trim();
}

export function chatLinePlainText(role: ChatLine["role"], text: string) {
  if (role === "assistant") {
    return markdownToPlainText(text);
  }
  return text.trim();
}

export function translateModelConfigStatus(configured: boolean) {
  return configured ? "настроено" : "не хватает LITEROUTER_API_KEY";
}

export function formatSessionTitle(session: ChatSessionSummary, index: number) {
  return session.title?.trim() || `Чат ${index + 1}`;
}

export function summarizeChatTitle(message: string) {
  const normalized = message.split("\n\nВложения:")[0].replace(/\s+/g, " ").trim();
  const lower = normalized.toLowerCase();
  if (lower.includes("разберись") && lower.includes("код")) return "Разбор кода проекта";
  if (lower.includes("запусти") && lower.includes("провер")) return "Проверка проекта";
  if (lower.includes("исправ") || lower.includes("почини")) return "Исправление проекта";
  return normalized.length > 56 ? `${normalized.slice(0, 56).trimEnd()}…` : normalized;
}

export function chatMatchesProject(chat: ChatSessionSummary, project: ProjectSelection) {
  if (!chat.workspace_path || project.path === null) {
    return false;
  }
  const chatPath = normalizePath(chat.workspace_path).toLowerCase();
  const projectPath = normalizePath(project.path).toLowerCase();
  if (projectPath !== ".") {
    return chatPath === projectPath;
  }
  return chatPath.endsWith(`/${project.label.toLowerCase()}`);
}

/** Split sessions so foreign-workspace chats stay visible (and deletable) in the sidebar. */
export function partitionSessionsForSidebar(
  chats: ChatSessionSummary[],
  project: ProjectSelection,
): { projectChats: ChatSessionSummary[]; otherChats: ChatSessionSummary[] } {
  if (project.path === null) {
    return { projectChats: [], otherChats: chats };
  }
  const projectChats: ChatSessionSummary[] = [];
  const otherChats: ChatSessionSummary[] = [];
  for (const chat of chats) {
    if (chatMatchesProject(chat, project)) {
      projectChats.push(chat);
    } else {
      otherChats.push(chat);
    }
  }
  return { projectChats, otherChats };
}

/**
 * Session to open on boot / project switch.
 * Never auto-open a chat bound to another workspace — that created invisible "ghost" chats.
 */
export function pickBootstrapSession(
  chats: ChatSessionSummary[],
  project: ProjectSelection,
): ChatSessionSummary | null {
  const { projectChats, otherChats } = partitionSessionsForSidebar(chats, project);
  if (project.path === null) {
    return otherChats[0] ?? null;
  }
  return projectChats[0] ?? otherChats.find((chat) => !chat.workspace_path) ?? null;
}

export function formatSessionTimestamp(value: string) {
  return new Date(value).toLocaleString("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatSessionPreview(session: ChatSessionSummary) {
  if (session.last_message) {
    const trimmed = session.last_message.replace(/\s+/g, " ").trim();
    return trimmed.length > 64 ? `${trimmed.slice(0, 64)}…` : trimmed;
  }
  return "Пока без сообщений";
}

export function formatProfileInitials(login: string | null) {
  if (!login) {
    return "??";
  }
  const compact = login.trim();
  if (!compact) {
    return "??";
  }
  return compact.slice(0, 2).toUpperCase();
}

export function formatRelativeAge(value: string) {
  const diffMs = Date.now() - new Date(value).getTime();
  const diffDays = Math.max(0, Math.floor(diffMs / (1000 * 60 * 60 * 24)));
  if (diffDays > 0) {
    return `${diffDays}д`;
  }
  const diffHours = Math.max(0, Math.floor(diffMs / (1000 * 60 * 60)));
  if (diffHours > 0) {
    return `${diffHours}ч`;
  }
  const diffMinutes = Math.max(0, Math.floor(diffMs / (1000 * 60)));
  return `${diffMinutes}м`;
}

export function formatPlan(plan: { id: string; tool_name: string; description: string; depends_on?: string[] | null }[]) {
  if (plan.length === 0) {
    return "План агента: пусто";
  }

  return [
    "План агента:",
    ...plan.map((step) => {
      const dependencies = step.depends_on ?? [];
      const deps = dependencies.length > 0 ? ` зависит от ${dependencies.join(", ")}` : "";
      return `- ${step.id}: ${step.tool_name} — ${step.description}${deps}`;
    }),
  ].join("\n");
}
