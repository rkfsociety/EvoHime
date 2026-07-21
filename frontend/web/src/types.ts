export type ChatLine = {
  id: string;
  role: "assistant" | "tool" | "system" | "user";
  text: string;
  taskId?: string;
};

export type WorkspacePanel =
  | "chat"
  | "files"
  | "sites"
  | "editor"
  | "terminal"
  | "git"
  | "plugins"
  | "memory"
  | "pull-requests"
  | "scheduled"
  | "tasks"
  | "actions"
  | "settings";

export type PermissionMode = "ask" | "allow" | "deny";
export type PermissionSettings = Record<string, { mode: PermissionMode }>;

export type PermissionAuditEntry = {
  approval_id: string;
  task_id: string;
  session_id?: string | null;
  tool_name: string;
  permission: string;
  scope: string;
  decision: "pending" | "granted" | "denied";
  at_ms: number;
  remembered_path?: boolean;
};

export type PermissionPathGrant = {
  permission: string;
  path: string;
  session_id?: string | null;
  mode: PermissionMode;
  expires_at_ms?: number | null;
};

export type PermissionScopes = {
  session_overrides: Array<{
    session_id: string;
    permission: string;
    mode: PermissionMode;
  }>;
  path_grants: PermissionPathGrant[];
};

export type ModelConfig = {
  provider: string;
  model: string;
  base_url: string;
  configured: boolean;
  available_models: string[];
  billing_mode: "free" | "paid";
  default_route: string;
  routes: Array<{
    name: string;
    provider: string;
    model: string;
    base_url: string;
    configured: boolean;
    available_models: string[];
    billing_mode: "free" | "paid";
  }>;
};

export type ModelRouteDraft = {
  name: string;
  provider: string;
  model: string;
  base_url: string;
  api_key: string;
  billing_mode: "free" | "paid";
  configured?: boolean;
};

export type ChatSessionSummary = {
  session_id: string;
  created_at: string;
  title?: string | null;
  workspace_path?: string | null;
  last_message_at: string | null;
  last_message: string | null;
  last_role: string | null;
};

export type SessionAttachment = {
  id: string;
  session_id: string;
  task_id?: string | null;
  workspace_path: string;
  original_name: string;
  stored_path: string;
  mime_type?: string | null;
  size_bytes: number;
  consumed_at?: string | null;
  created_at: string;
};

export type ProjectSelection = {
  label: string;
  path: string | null;
};

export type ProjectSummary = {
  name: string;
  path: string;
};

export type ProjectComposerPreference = {
  modelRoute?: string;
  model?: string;
  workMode?: PermissionMode;
};

export type GithubAuthInfo = {
  authenticated: boolean;
  login: string | null;
  source: string;
};

export type PullRequestAuthor = {
  login: string;
};

export type PullRequestSummary = {
  number: number;
  title: string;
  url: string;
  state: string;
  author: PullRequestAuthor | null;
  headRefName: string;
  baseRefName: string;
  createdAt: string;
  updatedAt: string;
};

export type PullRequestScope = "all" | "created" | "review_requested";

export type FileNode = {
  name: string;
  path: string;
  kind: "dir" | "file";
  size: number;
  modified_at: string | null;
};

export type FileListing = {
  path: string;
  entries: FileNode[];
};

export type FileContent = {
  path: string;
  content: string;
};

export type SaveResponse = {
  path: string;
  bytes: number;
  change: "created" | "updated";
};

export type SaveState = "idle" | "saving" | "saved";

export type SettingsTab =
  | "model"
  | "permissions"
  | "mcp"
  | "tools"
  | "worker"
  | "metrics"
  | "archive";

export type GitSnapshot = {
  status: string;
  diff: string;
};

export type GitAction = "commit" | "pull" | "push";

import type { PlanStep } from "./protocol";

export type TaskStepView = {
  id: string;
  runtimeId?: string;
  toolName: string;
  description: string;
  dependsOn: string[];
  status: string;
};

export type TaskApprovalWait = {
  approvalId: string;
  toolName: string;
  permission: string;
  scope: string;
};

export type TaskView = {
  id: string;
  message: string;
  status: string;
  steps: Record<string, TaskStepView>;
  retryCount: number;
  pauseReason: string | null;
  approvalWait: TaskApprovalWait | null;
  recovery: string | null;
  plan: PlanStep[];
  pendingPlan: PlanStep[] | null;
};

export type ActionView = {
  taskId: string;
  action: string;
  detail: string;
  createdAt: string;
};

export type ToolDefinition = {
  name: string;
  description: string;
  permissions: string[];
  timeout_ms: number;
};

export type McpServerConfig = {
  name: string;
  url: string;
  enabled: boolean;
  description?: string | null;
};

export const workspacePanels: Array<{ id: WorkspacePanel; label: string; phase: string }> = [
  { id: "chat", label: "Чат", phase: "активно" },
  { id: "files", label: "Файлы", phase: "этап 4" },
  { id: "sites", label: "Сайты", phase: "этап 6" },
  { id: "editor", label: "Редактор", phase: "этап 4" },
  { id: "terminal", label: "Терминал", phase: "этап 3" },
  { id: "git", label: "Гит", phase: "этап 4" },
  { id: "plugins", label: "Плагины", phase: "этап 6" },
  { id: "memory", label: "Память", phase: "этап 6" },
  { id: "pull-requests", label: "Пулл-реквесты", phase: "GitHub" },
  { id: "scheduled", label: "Запланировано", phase: "этап 5" },
  { id: "tasks", label: "Задачи", phase: "этап 5" },
  { id: "actions", label: "Действия", phase: "этап 5" },
  { id: "settings", label: "Настройки", phase: "этап 2" },
];

export const sidebarQuickLinks: Array<{
  id: "new-task" | "scheduled" | "plugins" | "memory" | "sites" | "pull-requests" | "chat";
  label: string;
  icon: string;
  panel: WorkspacePanel;
}> = [
  { id: "new-task", label: "Новая задача", icon: "✎", panel: "chat" },
  { id: "scheduled", label: "Запланировано", icon: "◷", panel: "scheduled" },
  { id: "plugins", label: "Плагины", icon: "◌", panel: "plugins" },
  { id: "memory", label: "Память", icon: "◈", panel: "memory" },
  { id: "sites", label: "Сайты", icon: "▦", panel: "sites" },
  { id: "pull-requests", label: "Пулл-реквесты", icon: "⟡", panel: "pull-requests" },
  { id: "chat", label: "Чат", icon: "⊕", panel: "chat" },
];

export const sidebarWorkspaceLinks: Array<{
  id: Extract<WorkspacePanel, "files" | "editor" | "terminal" | "git" | "tasks" | "actions">;
  label: string;
  icon: string;
}> = [
  { id: "files", label: "Файлы", icon: "▤" },
  { id: "editor", label: "Редактор", icon: "⌥" },
  { id: "terminal", label: "Терминал", icon: "▷" },
  { id: "git", label: "Git", icon: "⟐" },
  { id: "tasks", label: "Задачи", icon: "☰" },
  { id: "actions", label: "Действия", icon: "◎" },
];
