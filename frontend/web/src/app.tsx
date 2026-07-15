import Editor from "@monaco-editor/react";
import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import type {
  ClientCommand,
  PlanStep,
  ServerEvent,
  SessionBootstrap,
} from "./protocol";
import type { TaskStatusChangedEvent, TaskStepChangedEvent, ActionLoggedEvent } from "./protocol";
import type { ApprovalRequiredEvent } from "./protocol";
import { TerminalPanel, TerminalEntry } from "./components/TerminalPanel";
import { ApprovalModal } from "./components/ApprovalModal";

type ChatLine = {
  role: "assistant" | "tool" | "system" | "user";
  text: string;
};

type WorkspacePanel =
  | "chat"
  | "files"
  | "sites"
  | "editor"
  | "terminal"
  | "git"
  | "plugins"
  | "pull-requests"
  | "tasks"
  | "actions"
  | "settings";

type ModelConfig = {
  provider: string;
  model: string;
  base_url: string;
  configured: boolean;
  available_models: string[];
  default_route: string;
  routes: Array<{
    name: string;
    provider: string;
    model: string;
    base_url: string;
    configured: boolean;
    available_models: string[];
  }>;
};

type ChatSessionSummary = {
  session_id: string;
  created_at: string;
  last_message_at: string | null;
  last_message: string | null;
  last_role: string | null;
};

type GithubAuthInfo = {
  authenticated: boolean;
  login: string | null;
  source: string;
};

type PullRequestAuthor = {
  login: string;
};

type PullRequestSummary = {
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

type PullRequestScope = "all" | "created" | "review_requested";

type FileNode = {
  name: string;
  path: string;
  kind: "dir" | "file";
  size: number;
  modified_at: string | null;
};

type FileListing = {
  path: string;
  entries: FileNode[];
};

type FileContent = {
  path: string;
  content: string;
};

type SaveResponse = {
  path: string;
  bytes: number;
  change: "created" | "updated";
};

type GitSnapshot = {
  status: string;
  diff: string;
};

type GitAction = "commit" | "pull" | "push";

type TaskView = { id: string; message: string; status: string; steps: Record<string, string> };
type ActionView = { taskId: string; action: string; detail: string; createdAt: string };
type PermissionMode = "ask" | "allow" | "deny";
type PermissionSettings = Record<string, { mode: PermissionMode }>;
type ToolDefinition = {
  name: string;
  description: string;
  permissions: string[];
  timeout_ms: number;
};
type McpServerConfig = {
  name: string;
  url: string;
  enabled: boolean;
  description?: string | null;
};

type PluginCard = {
  name: string;
  description: string;
  icon: string;
  tag: string;
  actionLabel?: string;
};

const initialLines: ChatLine[] = [
  {
    role: "system",
    text: "EvoHime готова. Отправь сообщение, чтобы создать задачу и посмотреть поток событий.",
  },
];

const workspacePanels: Array<{ id: WorkspacePanel; label: string; phase: string }> = [
  { id: "chat", label: "Чат", phase: "активно" },
  { id: "files", label: "Файлы", phase: "этап 4" },
  { id: "sites", label: "Сайты", phase: "этап 6" },
  { id: "editor", label: "Редактор", phase: "этап 4" },
  { id: "terminal", label: "Терминал", phase: "этап 3" },
  { id: "git", label: "Гит", phase: "этап 4" },
  { id: "plugins", label: "Плагины", phase: "этап 6" },
  { id: "pull-requests", label: "Пулл-реквесты", phase: "GitHub" },
  { id: "tasks", label: "Задачи", phase: "этап 5" },
  { id: "actions", label: "Действия", phase: "этап 5" },
  { id: "settings", label: "Настройки", phase: "этап 2" },
];

const sidebarQuickLinks: Array<{
  id: "new-task" | "scheduled" | "plugins" | "sites" | "pull-requests" | "chat";
  label: string;
  icon: string;
  panel: WorkspacePanel;
}> = [
  { id: "new-task", label: "Новая задача", icon: "✎", panel: "chat" },
  { id: "scheduled", label: "Запланировано", icon: "◷", panel: "tasks" },
  { id: "plugins", label: "Плагины", icon: "◌", panel: "plugins" },
  { id: "sites", label: "Сайты", icon: "▦", panel: "sites" },
  { id: "pull-requests", label: "Пулл-реквесты", icon: "⟡", panel: "pull-requests" },
  { id: "chat", label: "Чат", icon: "⊕", panel: "chat" },
];

const featuredPlugins: PluginCard[] = [
  {
    name: "Computer Use",
    description: "Управление Windows-приложениями через Codex.",
    icon: "⌘",
    tag: "Инструменты",
    actionLabel: "Установить",
  },
  {
    name: "Chrome",
    description: "Контроль вкладок и действий в Chrome.",
    icon: "⌖",
    tag: "Браузер",
    actionLabel: "Установить",
  },
  {
    name: "Spreadsheets",
    description: "Создание и редактирование таблиц.",
    icon: "▦",
    tag: "Документы",
    actionLabel: "Установить",
  },
  {
    name: "Presentations",
    description: "Сборка и правка презентаций.",
    icon: "▤",
    tag: "Документы",
    actionLabel: "Установить",
  },
  {
    name: "GitHub",
    description: "Разбор PR, issues, CI и публикация изменений.",
    icon: "⌂",
    tag: "Разработка",
  },
  {
    name: "Notion",
    description: "Поиск и чтение заметок и баз знаний.",
    icon: "◫",
    tag: "Продуктивность",
    actionLabel: "Установить",
  },
];

function normalizePath(path?: string) {
  if (!path || path === ".") {
    return ".";
  }
  return path.replace(/\\/g, "/");
}

function parentPath(path: string) {
  const normalized = normalizePath(path);
  if (normalized === ".") {
    return ".";
  }
  const segments = normalized.split("/").filter(Boolean);
  segments.pop();
  return segments.length > 0 ? segments.join("/") : ".";
}

function inferMonacoLanguage(path: string | null) {
  if (!path) {
    return "plaintext";
  }

  const lower = path.toLowerCase();
  if (lower.endsWith(".ts") || lower.endsWith(".tsx")) return "typescript";
  if (lower.endsWith(".js") || lower.endsWith(".jsx") || lower.endsWith(".mjs")) return "javascript";
  if (lower.endsWith(".json")) return "json";
  if (lower.endsWith(".md") || lower.endsWith(".markdown")) return "markdown";
  if (lower.endsWith(".rs")) return "rust";
  if (lower.endsWith(".toml")) return "toml";
  if (lower.endsWith(".yml") || lower.endsWith(".yaml")) return "yaml";
  if (lower.endsWith(".css")) return "css";
  if (lower.endsWith(".html") || lower.endsWith(".htm")) return "html";
  if (lower.endsWith(".sh") || lower.endsWith(".bash")) return "shell";
  if (lower.endsWith(".sql")) return "sql";
  return "plaintext";
}

function formatFileSize(bytes: number) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function sortFileNodes(entries: FileNode[]) {
  return [...entries].sort((left, right) => {
    if (left.kind !== right.kind) {
      return left.kind === "dir" ? -1 : 1;
    }
    return left.name.localeCompare(right.name, undefined, { sensitivity: "base" });
  });
}

function summarizeGitStatus(status: string) {
  const lines = status.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  const branch = lines[0] ?? "Нет статуса";
  const changed = lines.filter((line) => !line.startsWith("##")).length;
  return {
    branch,
    changed,
    lines,
  };
}

function translateSocketState(state: "idle" | "connecting" | "connected" | "failed") {
  switch (state) {
    case "idle":
      return "Ожидание";
    case "connecting":
      return "Подключение";
    case "connected":
      return "Подключено";
    case "failed":
      return "Ошибка подключения";
  }
}

function translateTaskStatus(status: string) {
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

function translateStepStatus(status: string) {
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

function translatePermissionMode(mode: PermissionMode) {
  switch (mode) {
    case "ask":
      return "спрашивать";
    case "allow":
      return "разрешать";
    case "deny":
      return "запрещать";
  }
}

function translateSaveState(state: "idle" | "saving" | "saved") {
  switch (state) {
    case "idle":
      return "Готово";
    case "saving":
      return "Сохранение...";
    case "saved":
      return "Сохранено";
  }
}

function translateGitAction(action: GitAction) {
  switch (action) {
    case "commit":
      return "коммит";
    case "pull":
      return "загрузка";
    case "push":
      return "отправка";
  }
}

function translateEventType(type: ServerEvent["type"]) {
  switch (type) {
    case "session.created":
      return "Сессия создана";
    case "task.started":
      return "Задача запущена";
    case "agent.message.delta":
      return "Частичный ответ агента";
    case "agent.plan.updated":
      return "План агента обновлён";
    case "tool.started":
      return "Инструмент запущен";
    case "tool.output":
      return "Вывод инструмента";
    case "tool.completed":
      return "Инструмент завершён";
    case "task.completed":
      return "Задача завершена";
    case "task.failed":
      return "Задача завершилась с ошибкой";
    case "task.status.changed":
      return "Статус задачи изменён";
    case "task.step.changed":
      return "Шаг задачи изменён";
    case "action.logged":
      return "Действие записано";
    case "file.changed":
      return "Файл изменён";
    case "git.diff.changed":
      return "Изменения Гит обновлены";
    case "approval.required":
      return "Требуется разрешение";
  }
}

function translateChatRole(role: ChatLine["role"]) {
  switch (role) {
    case "assistant":
      return "Ассистент";
    case "tool":
      return "Инструмент";
    case "system":
      return "Система";
    case "user":
      return "Пользователь";
  }
}

function translateModelConfigStatus(configured: boolean) {
  return configured ? "настроено" : "не хватает LITEROUTER_API_KEY";
}

function formatSessionTitle(session: ChatSessionSummary, index: number) {
  return `Чат ${index + 1}`;
}

function formatSessionTimestamp(value: string) {
  return new Date(value).toLocaleString("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatSessionPreview(session: ChatSessionSummary) {
  if (session.last_message) {
    const trimmed = session.last_message.replace(/\s+/g, " ").trim();
    return trimmed.length > 64 ? `${trimmed.slice(0, 64)}…` : trimmed;
  }
  return "Пока без сообщений";
}

function formatProfileInitials(login: string | null) {
  if (!login) {
    return "??";
  }
  const compact = login.trim();
  if (!compact) {
    return "??";
  }
  return compact.slice(0, 2).toUpperCase();
}

function formatRelativeAge(value: string) {
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
  if (diffMinutes > 0) {
    return `${diffMinutes}м`;
  }
  return "только что";
}

export function App() {
  const [session, setSession] = useState<SessionBootstrap | null>(null);
  const [chatSessions, setChatSessions] = useState<ChatSessionSummary[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [socketState, setSocketState] = useState<
    "idle" | "connecting" | "connected" | "failed"
  >("idle");
  const [input, setInput] = useState("");
  const [lines, setLines] = useState<ChatLine[]>(initialLines);
  const [events, setEvents] = useState<ServerEvent[]>([]);
  const [stream, setStream] = useState("");
  const [activePanel, setActivePanel] = useState<WorkspacePanel>("chat");
  const [modelConfig, setModelConfig] = useState<ModelConfig | null>(null);
  const [modelConfigError, setModelConfigError] = useState<string | null>(null);
  const [selectedModelRoute, setSelectedModelRoute] = useState("");
  const [directoryCache, setDirectoryCache] = useState<Record<string, FileNode[]>>({});
  const [expandedDirectories, setExpandedDirectories] = useState<Record<string, boolean>>({
    ".": true,
  });
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [selectedFileContent, setSelectedFileContent] = useState("");
  const [selectedFileOriginal, setSelectedFileOriginal] = useState("");
  const [selectedFileLoading, setSelectedFileLoading] = useState(false);
  const [selectedFileNotice, setSelectedFileNotice] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved">("idle");
  const [gitStatus, setGitStatus] = useState("Загрузка статуса Гит...");
  const [gitDiff, setGitDiff] = useState("Загрузка изменений Гит...");
  const [gitDiffPath, setGitDiffPath] = useState<string | null>(null);
  const [gitDiffPathInput, setGitDiffPathInput] = useState("");
  const [newFilePath, setNewFilePath] = useState("");
  const [newFileContent, setNewFileContent] = useState("");
  const [gitCommitMessage, setGitCommitMessage] = useState("");
  const [gitRemote, setGitRemote] = useState("origin");
  const [gitBranch, setGitBranch] = useState("");
  const [gitAction, setGitAction] = useState<GitAction | null>(null);
  const [gitActionNotice, setGitActionNotice] = useState<string | null>(null);
  const [tasks, setTasks] = useState<Record<string, TaskView>>({});
  const [actions, setActions] = useState<ActionView[]>([]);
  const [approval, setApproval] = useState<ApprovalRequiredEvent | null>(null);
  const [githubAuth, setGithubAuth] = useState<GithubAuthInfo | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [pullRequests, setPullRequests] = useState<PullRequestSummary[]>([]);
  const [pullRequestsLoading, setPullRequestsLoading] = useState(false);
  const [pullRequestsError, setPullRequestsError] = useState<string | null>(null);
  const [pullRequestScope, setPullRequestScope] = useState<PullRequestScope>("all");
  const [pullRequestSearch, setPullRequestSearch] = useState("");
  const [siteSearch, setSiteSearch] = useState("");
  const [terminalEntries, setTerminalEntries] = useState<TerminalEntry[]>([]);
  const [permissionSettings, setPermissionSettings] = useState<PermissionSettings>({});
  const [toolCatalog, setToolCatalog] = useState<ToolDefinition[]>([]);
  const [toolCatalogError, setToolCatalogError] = useState<string | null>(null);
  const [mcpServers, setMcpServers] = useState<McpServerConfig[]>([]);
  const [mcpServersError, setMcpServersError] = useState<string | null>(null);
  const [mcpServersSaving, setMcpServersSaving] = useState(false);
  const [mcpServersNotice, setMcpServersNotice] = useState<string | null>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const applyEventRef = useRef<(event: ServerEvent) => void>(() => undefined);
  const saveFileRef = useRef<() => void>(() => undefined);
  const sessionLoadRef = useRef(0);

  useEffect(() => {
    let cancelled = false;

    const loadModelConfig = async () => {
      const response = await fetch("/api/models/config");
      if (!response.ok) {
        throw new Error("Не удалось загрузить конфигурацию модели");
      }
      const data = (await response.json()) as ModelConfig;
      if (!cancelled) {
        setModelConfig(data);
      }
    };
    fetch("/api/permissions").then((response) => response.json()).then((data: PermissionSettings) => setPermissionSettings(data)).catch(() => undefined);
    fetch("/api/tools")
      .then((response) => {
        if (!response.ok) {
          throw new Error("Не удалось загрузить каталог инструментов");
        }
        return response.json();
      })
      .then((data: ToolDefinition[]) => {
        if (!cancelled) {
          setToolCatalog(data);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setToolCatalogError(String(error));
        }
      });
    fetch("/api/mcp/servers")
      .then((response) => {
        if (!response.ok) {
          throw new Error("Не удалось загрузить MCP-серверы");
        }
        return response.json();
      })
      .then((data: McpServerConfig[]) => {
        if (!cancelled) {
          setMcpServers(data);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setMcpServersError(String(error));
        }
      });
    fetch("/api/auth/github")
      .then((response) => {
        if (!response.ok) {
          throw new Error("Не удалось загрузить GitHub-авторизацию");
        }
        return response.json();
      })
      .then((data: GithubAuthInfo) => {
        if (!cancelled) {
          setGithubAuth(data);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setGithubAuth({ authenticated: false, login: null, source: "gh" });
        }
      });

    loadModelConfig().catch((error) => {
      if (!cancelled) {
        setModelConfigError(String(error));
      }
    });

    const loadSessions = async () => {
      const response = await fetch("/api/sessions");
      if (!response.ok) {
        throw new Error("Не удалось загрузить список чатов");
      }
      const data = (await response.json()) as ChatSessionSummary[];
      if (cancelled) {
        return;
      }
      setChatSessions(data);
      if (data.length > 0) {
        void openSession(data[0]).catch((error) => {
          if (!cancelled) {
            setSocketState("failed");
            setLines((current) => [
              ...current,
              { role: "system", text: String(error) },
            ]);
          }
        });
        return;
      }

      const createdResponse = await fetch("/api/sessions", { method: "POST" });
      if (!createdResponse.ok) {
        throw new Error("Не удалось создать сессию");
      }
      const bootstrap = (await createdResponse.json()) as SessionBootstrap;
      if (cancelled) {
        return;
      }
      const createdSummary: ChatSessionSummary = {
        session_id: bootstrap.session_id,
        created_at: bootstrap.created_at,
        last_message_at: null,
        last_message: null,
        last_role: null,
      };
      setChatSessions([createdSummary]);
      setActiveSessionId(createdSummary.session_id);
      hydrateSession(createdSummary, bootstrap.events);
    };

    loadSessions().catch((error) => {
      if (!cancelled) {
        setSocketState("failed");
        setLines((current) => [
          ...current,
          { role: "system", text: String(error) },
        ]);
      }
    });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!modelConfig) {
      return;
    }

    const routeNames = new Set(modelConfig.routes.map((route) => route.name));
    if (!selectedModelRoute || !routeNames.has(selectedModelRoute)) {
      setSelectedModelRoute(modelConfig.default_route);
    }
  }, [modelConfig, selectedModelRoute]);

  useEffect(() => {
    let cancelled = false;

    const loadPullRequests = async (scope: PullRequestScope) => {
      setPullRequestsLoading(true);
      setPullRequestsError(null);
      try {
        const response = await fetch(`/api/github/pull-requests?scope=${encodeURIComponent(scope)}`);
        if (!response.ok) {
          throw new Error("Не удалось загрузить pull request'ы");
        }
        const data = (await response.json()) as PullRequestSummary[];
        if (!cancelled) {
          setPullRequests(data);
        }
      } catch (error) {
        if (!cancelled) {
          setPullRequests([]);
          setPullRequestsError(String(error));
        }
      } finally {
        if (!cancelled) {
          setPullRequestsLoading(false);
        }
      }
    };

    void loadPullRequests(pullRequestScope);

    return () => {
      cancelled = true;
    };
  }, [pullRequestScope]);

  useEffect(() => {
    if (!session) {
      return;
    }

    setSocketState("connecting");
    const protocol = window.location.protocol === "https:" ? "wss" : "ws";
    const socket = new WebSocket(
      `${protocol}://${window.location.host}/ws/${session.session_id}`,
    );
    socketRef.current = socket;

    socket.onopen = () => {
      setSocketState("connected");
    };

    socket.onclose = () => {
      setSocketState("idle");
    };

    socket.onerror = () => {
      setSocketState("failed");
    };

    socket.onmessage = (event) => {
      const parsed = JSON.parse(event.data as string) as ServerEvent;
      setEvents((current) => [...current, parsed]);
      applyEventRef.current(parsed);
    };

    return () => {
      socket.close();
      socketRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session]);

  useEffect(() => {
    document.body.style.overflow = settingsOpen ? "hidden" : "";
    return () => {
      document.body.style.overflow = "";
    };
  }, [settingsOpen]);

  useEffect(() => {
    if (!session) {
      return;
    }

    void refreshDirectory(".").catch(() => undefined);
    void refreshGitSnapshot().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session]);

  const connectedLabel = useMemo(() => {
    if (!session) {
      return "Загрузка чатов...";
    }
    if (socketState === "connected") {
      return "Подключено";
    }
    if (socketState === "failed") {
      return "Ошибка подключения";
    }
    return "Подключение...";
  }, [session, socketState]);

  const currentPanelLabel = useMemo(
    () => workspacePanels.find((panel) => panel.id === activePanel)?.label ?? "Рабочее пространство",
    [activePanel],
  );
  const activeProjectLabel = "EvoHime";
  const selectedFileLanguage = useMemo(
    () => inferMonacoLanguage(selectedFilePath),
    [selectedFilePath],
  );
  const gitSummary = useMemo(() => summarizeGitStatus(gitStatus), [gitStatus]);
  const pendingTaskCount = useMemo(
    () => Object.values(tasks).filter((task) => task.status === "running" || task.status === "paused" || task.status === "cancelling").length,
    [tasks],
  );
  const visiblePullRequests = useMemo(() => {
    const query = pullRequestSearch.trim().toLowerCase();
    if (!query) {
      return pullRequests;
    }
    return pullRequests.filter((pullRequest) => {
      const haystack = [
        `#${pullRequest.number}`,
        pullRequest.title,
        pullRequest.state,
        pullRequest.author?.login ?? "",
        pullRequest.headRefName,
        pullRequest.baseRefName,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
  }, [pullRequests, pullRequestSearch]);
  applyEventRef.current = applyEvent;
  saveFileRef.current = () => {
    void handleSave();
  };

  function hydrateSession(summary: ChatSessionSummary, history: SessionBootstrap["events"]) {
    setActiveSessionId(summary.session_id);
    setSession({
      session_id: summary.session_id,
      created_at: summary.created_at,
      events: history,
    });
    setLines(initialLines);
    setEvents(history.map((item) => item.event));
    setStream("");
    setTasks({});
    setActions([]);
    setApproval(null);
    setTerminalEntries([]);
    for (const item of history) {
      applyEventRef.current(item.event);
    }
  }

  async function openSession(summary: ChatSessionSummary) {
    const requestId = sessionLoadRef.current + 1;
    sessionLoadRef.current = requestId;
    setActiveSessionId(summary.session_id);

    const response = await fetch(`/api/sessions/${summary.session_id}/history`);
    if (!response.ok) {
      throw new Error("Не удалось загрузить чат");
    }

    const history = (await response.json()) as SessionBootstrap["events"];
    if (requestId !== sessionLoadRef.current) {
      return;
    }

    hydrateSession(summary, history);
  }

  function applyEvent(event: ServerEvent) {
    switch (event.type) {
      case "session.created":
        setLines((current) => [
          ...current,
          {
            role: "system",
            text: `Сессия создана: ${event.session_id}`,
          },
        ]);
        break;
      case "task.started":
        setChatSessions((current) =>
          current.map((chat) =>
            chat.session_id === event.session_id
              ? {
                  ...chat,
                  last_message: event.user_message,
                  last_message_at: event.created_at,
                  last_role: "user",
                }
              : chat,
          ),
        );
        setTasks((current) => ({ ...current, [event.task_id]: { id: event.task_id, message: event.user_message, status: "running", steps: {} } }));
        setLines((current) => [
          ...current,
          { role: "user", text: event.user_message },
          { role: "assistant", text: "" },
        ]);
        setStream("");
        break;
      case "agent.message.delta":
        setStream((current) => {
          const next = `${current}${event.delta}`;
          setLines((items) => {
            const copy = [...items];
            for (let index = copy.length - 1; index >= 0; index -= 1) {
              if (copy[index]?.role === "assistant") {
                copy[index] = { role: "assistant", text: next };
                break;
              }
            }
            return copy;
          });
          return next;
        });
        break;
      case "tool.started":
        setLines((current) => [
          ...current,
          {
            role: "tool",
            text: `Инструмент запущен: ${event.tool_name}`,
          },
        ]);
        break;
      case "tool.output":
        if (event.tool_name === "shell.execute") setTerminalEntries((current) => [...current, { stream: "stdout", text: event.output }]);
        setLines((current) => [
          ...current,
          {
            role: "tool",
            text: `Результат из ${event.tool_name}:\n${event.output}`,
          },
        ]);
        break;
      case "approval.required":
        setApproval(event);
        break;
      case "tool.completed":
        if (event.tool_name === "shell.execute") {
          setTerminalEntries((current) => [
            ...current,
            {
              stream: event.success ? "status" : "stderr",
              text: event.success ? "shell.execute выполнен" : "shell.execute завершился с ошибкой",
            },
          ]);
        }
        setLines((current) => [
          ...current,
          {
            role: "tool",
            text: `Инструмент ${event.tool_name} завершён`,
          },
        ]);
        break;
      case "task.completed":
        setTasks((current) => current[event.task_id] ? { ...current, [event.task_id]: { ...current[event.task_id], status: "completed" } } : current);
        setLines((current) => [
          ...current,
          { role: "assistant", text: event.final_message },
        ]);
        setStream("");
        break;
      case "task.failed":
        setTasks((current) => current[event.task_id] ? { ...current, [event.task_id]: { ...current[event.task_id], status: "failed" } } : current);
        setLines((current) => [
          ...current,
          { role: "system", text: `Задача завершилась с ошибкой: ${event.error}` },
        ]);
        setStream("");
        break;
      case "task.status.changed": {
        const statusEvent = event as TaskStatusChangedEvent;
        setTasks((current) => current[statusEvent.task_id] ? { ...current, [statusEvent.task_id]: { ...current[statusEvent.task_id], status: statusEvent.status } } : current);
        break;
      }
      case "task.step.changed": {
        const stepEvent = event as TaskStepChangedEvent;
        setTasks((current) => current[stepEvent.task_id] ? { ...current, [stepEvent.task_id]: { ...current[stepEvent.task_id], steps: { ...current[stepEvent.task_id].steps, [stepEvent.tool_name]: stepEvent.status } } } : current);
        break;
      }
      case "action.logged": {
        const actionEvent = event as ActionLoggedEvent;
        setActions((current) => [...current, { taskId: actionEvent.task_id, action: actionEvent.action, detail: actionEvent.detail, createdAt: actionEvent.created_at }]);
        break;
      }
      case "agent.plan.updated":
        setLines((current) => [
          ...current,
          {
            role: "system",
            text: formatPlan(event.plan),
          },
        ]);
        break;
      case "file.changed":
        void refreshDirectory(".").catch(() => undefined);
        void refreshDirectory(parentPath(event.path)).catch(() => undefined);
        if (normalizePath(selectedFilePath ?? undefined) === normalizePath(event.path)) {
          if (selectedFileContent !== selectedFileOriginal) {
            setSelectedFileNotice("Файл изменился на диске. Сохрани или перезагрузи, чтобы не потерять правки.");
          } else {
            void refreshSelectedFile(event.path).catch(() => undefined);
          }
        }
        break;
      case "git.diff.changed":
        setGitStatus(event.status);
        setGitDiff(event.diff);
        break;
    }
  }

  async function refreshDirectory(path: string) {
    const normalized = normalizePath(path);
    const query = normalized === "." ? "" : `?path=${encodeURIComponent(normalized)}`;
    const response = await fetch(`/api/files${query}`);
    if (!response.ok) {
      throw new Error("Не удалось загрузить дерево файлов");
    }
    const data = (await response.json()) as FileListing;
    const key = normalizePath(data.path);
    setDirectoryCache((current) => ({
      ...current,
      [key]: sortFileNodes(data.entries),
    }));
  }

  async function toggleDirectory(path: string) {
    const normalized = normalizePath(path);
    setExpandedDirectories((current) => ({
      ...current,
      [normalized]: !current[normalized],
    }));

    if (!directoryCache[normalized]) {
      await refreshDirectory(normalized);
    }
  }

  async function refreshSelectedFile(path: string) {
    setSelectedFileLoading(true);
    setSelectedFileNotice(null);
    try {
      const response = await fetch(
        `/api/files/content?path=${encodeURIComponent(normalizePath(path))}`,
      );
      if (!response.ok) {
        throw new Error("Не удалось загрузить содержимое файла");
      }
      const data = (await response.json()) as FileContent;
      setSelectedFilePath(data.path);
      setSelectedFileContent(data.content);
      setSelectedFileOriginal(data.content);
      setSaveState("idle");
    } finally {
      setSelectedFileLoading(false);
    }
  }

  async function refreshGitSnapshot(path?: string | null) {
    const diffPath = normalizePath(path ?? gitDiffPathInput ?? gitDiffPath ?? selectedFilePath ?? undefined);
    const statusResponse = await fetch("/api/git/status");
    if (!statusResponse.ok) {
      throw new Error("Не удалось загрузить статус Гит");
    }
    const statusData = (await statusResponse.json()) as GitSnapshot;
    const diffResponse = await fetch(
      diffPath === "." ? "/api/git/diff" : `/api/git/diff?path=${encodeURIComponent(diffPath)}`,
    );
    if (!diffResponse.ok) {
      throw new Error("Не удалось загрузить изменения Гит");
    }
    const diffData = (await diffResponse.json()) as GitSnapshot;
    setGitStatus(statusData.status);
    setGitDiff(diffData.diff);
    setGitDiffPath(diffPath === "." ? null : diffPath);
    setGitDiffPathInput(diffPath === "." ? "" : diffPath);
  }

  async function updatePermission(name: string, mode: PermissionMode) {
    const response = await fetch(`/api/permissions/${name}`, { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ mode }) });
    if (!response.ok) throw new Error("Не удалось обновить разрешение");
    setPermissionSettings((current) => ({ ...current, [name]: { mode } }));
  }

  async function saveMcpServers() {
    setMcpServersSaving(true);
    setMcpServersNotice(null);
    try {
      const response = await fetch("/api/mcp/servers", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(mcpServers),
      });
      if (!response.ok) {
        throw new Error(await response.text());
      }
      const data = (await response.json()) as McpServerConfig[];
      setMcpServers(data);
      setMcpServersNotice("MCP-серверы сохранены.");
    } catch (error) {
      setMcpServersNotice(`Не удалось сохранить MCP-серверы: ${String(error)}`);
    } finally {
      setMcpServersSaving(false);
    }
  }

  function addMcpServer() {
    setMcpServers((current) => [
      ...current,
      {
        name: "",
        url: "https://",
        enabled: true,
        description: "",
      },
    ]);
  }

  function updateMcpServer(index: number, patch: Partial<McpServerConfig>) {
    setMcpServers((current) =>
      current.map((server, currentIndex) =>
        currentIndex === index
          ? {
              ...server,
              ...patch,
            }
          : server,
      ),
    );
  }

  function removeMcpServer(index: number) {
    setMcpServers((current) => current.filter((_, currentIndex) => currentIndex !== index));
  }

  async function openFile(path: string) {
    try {
      setActivePanel("editor");
      setSelectedFileNotice(null);
      await refreshSelectedFile(path);
      await refreshGitSnapshot(path);
    } catch (error) {
      setLines((current) => [
        ...current,
        { role: "system", text: String(error) },
      ]);
    }
  }

  async function handleSave() {
    if (!session || !selectedFilePath) {
      return;
    }

    setSaveState("saving");
    try {
      const response = await fetch(
        `/api/files/content?path=${encodeURIComponent(selectedFilePath)}&session_id=${encodeURIComponent(session.session_id)}`,
        {
          method: "PUT",
          headers: {
            "Content-Type": "application/json",
          },
          body: JSON.stringify({
            content: selectedFileContent,
          }),
        },
      );

      if (!response.ok) {
        throw new Error("Не удалось сохранить файл");
      }

      const data = (await response.json()) as SaveResponse;
      setSelectedFileOriginal(selectedFileContent);
      setSaveState("saved");
      setSelectedFileNotice(data.change === "created" ? "Новый файл создан в рабочем пространстве." : "Изменения сохранены.");
      await refreshDirectory(parentPath(data.path));
      await refreshGitSnapshot(data.path);
    } catch (error) {
      setSaveState("idle");
      setLines((current) => [
        ...current,
        { role: "system", text: String(error) },
      ]);
    }
  }

  async function handleCreateFile() {
    if (!session || !newFilePath.trim()) {
      return;
    }

    const path = normalizePath(newFilePath.trim());
    try {
      const response = await fetch(
        `/api/files/content?path=${encodeURIComponent(path)}&session_id=${encodeURIComponent(session.session_id)}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ content: newFileContent }),
        },
      );
      if (!response.ok) {
        throw new Error(await response.text());
      }
      setNewFilePath("");
      setNewFileContent("");
      await refreshDirectory(parentPath(path));
      await refreshDirectory(".");
      await openFile(path);
    } catch (error) {
      setSelectedFileNotice(`Не удалось создать файл: ${String(error)}`);
    }
  }

  async function handleGitAction(action: GitAction) {
    if (!session || gitAction) {
      return;
    }

      setGitAction(action);
      setGitActionNotice(null);
    const payload = action === "commit"
      ? { message: gitCommitMessage }
      : { remote: gitRemote || undefined, branch: gitBranch || undefined };
    try {
      const response = await fetch(
        `/api/git/${action}?session_id=${encodeURIComponent(session.session_id)}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
        },
      );
      if (!response.ok) {
        const detail = await response.text();
        throw new Error(detail || `Гит-действие ${action} завершилось с ошибкой`);
      }
      setGitActionNotice(`Операция Гит «${translateGitAction(action)}» завершена.`);
      if (action === "commit") {
        setGitCommitMessage("");
      }
      await refreshGitSnapshot(gitDiffPath);
      await refreshDirectory(".");
    } catch (error) {
      setGitActionNotice(`Операция Гит «${translateGitAction(action)}»: ${String(error)}`);
    } finally {
      setGitAction(null);
    }
  }

  async function sendMessage(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const text = input.trim();
    if (!text || !socketRef.current || socketState !== "connected") {
      return;
    }

    const payload: ClientCommand = {
      type: "user.message",
      content: text,
      model_route: selectedModelRoute || undefined,
    };

    socketRef.current.send(JSON.stringify(payload));
    setInput("");
  }

  function sendTaskCommand(type: "task.cancel" | "task.resume" | "task.retry", taskId: string) {
    if (socketRef.current?.readyState === WebSocket.OPEN) {
      socketRef.current.send(JSON.stringify({ type, task_id: taskId }));
    }
  }

  function resolveApproval(type: "approval.granted" | "approval.denied") {
    if (approval && socketRef.current?.readyState === WebSocket.OPEN) {
      socketRef.current.send(JSON.stringify({ type, approval_id: approval.approval_id }));
      setApproval(null);
    }
  }

  function renderTree(path = ".", depth = 0) {
    const normalized = normalizePath(path);
    const entries = directoryCache[normalized] ?? [];

    return (
      <div className="treeBranch" data-depth={depth}>
        {entries.map((entry) => {
          const key = entry.path;
          const expanded = expandedDirectories[key] ?? false;
          const isSelected = normalizePath(selectedFilePath ?? undefined) === normalizePath(key);

          return (
            <div
              key={key}
              className="treeNode"
              data-kind={entry.kind}
              data-depth={depth}
              data-selected={isSelected}
              data-expanded={expanded}
            >
              <button
                type="button"
                className="treeButton"
                onClick={async () => {
                  if (entry.kind === "dir") {
                    await toggleDirectory(key);
                    return;
                  }
                  await openFile(key);
                }}
              >
                <span className="treeName">{entry.name}</span>
                <span className="treeMeta">
                  {entry.kind}
                  {entry.size ? ` • ${formatFileSize(entry.size)}` : ""}
                  {entry.modified_at ? ` • ${entry.modified_at}` : ""}
                </span>
              </button>
              {entry.kind === "dir" && expanded ? renderTree(key, depth + 1) : null}
            </div>
          );
        })}
      </div>
    );
  }

  function renderSettingsContent() {
    return (
      <div className="settingsPanel">
        <section className="settingsSection">
          <h3>Провайдер модели</h3>
          {modelConfigError ? <p className="settingsError">{modelConfigError}</p> : null}
          {modelConfig ? (
            <dl className="settingsGrid">
              <div>
                <dt>Маршрут по умолчанию</dt>
                <dd>{modelConfig.default_route}</dd>
              </div>
              <div>
                <dt>Провайдер</dt>
                <dd>{modelConfig.provider}</dd>
              </div>
              <div>
                <dt>Модель</dt>
                <dd>{modelConfig.model}</dd>
              </div>
              <div>
                <dt>Базовый URL</dt>
                <dd>{modelConfig.base_url}</dd>
              </div>
              <div>
                <dt>API-ключ</dt>
                <dd data-configured={modelConfig.configured}>
                  {translateModelConfigStatus(modelConfig.configured)}
                </dd>
              </div>
              <div>
                <dt>Доступные модели</dt>
                <dd>{modelConfig.available_models.join(", ")}</dd>
              </div>
              <div>
                <dt>Маршруты</dt>
                <dd>
                  {modelConfig.routes
                    .map((route) => `${route.name} (${route.provider}:${route.model})`)
                    .join(", ")}
                </dd>
              </div>
            </dl>
          ) : (
            <p>Загрузка конфигурации модели...</p>
          )}
          <p className="settingsHint">
            Укажи `MODEL_ROUTES_JSON` на сервере, чтобы настроить несколько маршрутов и выбирать один для каждой задачи.
          </p>
        </section>

        <section className="settingsSection">
          <h3>Разрешения инструментов</h3>
          <div className="permissionList">
            {Object.entries(permissionSettings).map(([name, value]) => (
              <label key={name}>
                <span>{name}</span>
                <select
                  value={value.mode}
                  onChange={(event) =>
                    void updatePermission(name, event.target.value as PermissionMode)
                  }
                >
                  <option value="ask">спрашивать</option>
                  <option value="allow">разрешать</option>
                  <option value="deny">запрещать</option>
                </select>
              </label>
            ))}
          </div>
        </section>

        <section className="settingsSection">
          <div className="settingsHeaderRow">
            <div>
              <h3>MCP-серверы</h3>
              <p className="settingsHint">
                Эти конечные точки редактируются в памяти и управляют интерфейсом MCP.
              </p>
            </div>
            <div className="toolbarActions">
              <button type="button" onClick={addMcpServer}>
                Добавить сервер
              </button>
              <button type="button" onClick={() => void saveMcpServers()} disabled={mcpServersSaving}>
                {mcpServersSaving ? "Сохранение..." : "Сохранить серверы"}
              </button>
            </div>
          </div>
          {mcpServersError ? <p className="settingsError">{mcpServersError}</p> : null}
          {mcpServersNotice ? <p className="settingsHint">{mcpServersNotice}</p> : null}
          <div className="mcpServerList">
            {mcpServers.length === 0 ? (
              <div className="emptyState">
                <strong>Пока нет MCP-серверов</strong>
                <p>Добавь первый сервер, чтобы держать общие точки доступа под рукой для агентов.</p>
              </div>
            ) : null}
            {mcpServers.map((server, index) => (
              <article className="mcpServerCard" key={`${server.name || "server"}-${index}`}>
                <div className="mcpServerRow">
                  <label>
                    <span>Название</span>
                    <input
                      value={server.name}
                      onChange={(event) => updateMcpServer(index, { name: event.target.value })}
                      placeholder="docs"
                    />
                  </label>
                  <label>
                    <span>Ссылка</span>
                    <input
                      value={server.url}
                      onChange={(event) => updateMcpServer(index, { url: event.target.value })}
                      placeholder="https://example.com/rpc"
                    />
                  </label>
                </div>
                <label className="mcpServerDescription">
                  <span>Описание</span>
                  <input
                    value={server.description ?? ""}
                    onChange={(event) =>
                      updateMcpServer(index, { description: event.target.value })
                    }
                    placeholder="Необязательная заметка"
                  />
                </label>
                <div className="mcpServerFooter">
                  <label className="toggleRow">
                    <input
                      type="checkbox"
                      checked={server.enabled}
                      onChange={(event) =>
                        updateMcpServer(index, { enabled: event.target.checked })
                      }
                    />
                    <span>Включён</span>
                  </label>
                  <button type="button" onClick={() => removeMcpServer(index)}>
                    Удалить
                  </button>
                </div>
              </article>
            ))}
          </div>
        </section>

        <section className="settingsSection">
          <h3>Каталог инструментов</h3>
          {toolCatalogError ? <p className="settingsError">{toolCatalogError}</p> : null}
          <div className="toolCatalog">
            {toolCatalog.map((tool) => (
              <article className="toolCard" key={tool.name}>
                <strong>{tool.name}</strong>
                <p>{tool.description}</p>
                <small>{tool.permissions.join(", ") || "нет разрешений"}</small>
                <span>Таймаут {tool.timeout_ms} мс</span>
              </article>
            ))}
          </div>
        </section>
      </div>
    );
  }

  function renderPluginsContent() {
    return (
      <div className="pluginsPage">
        <section className="pluginsHero">
          <div>
            <h3>Плагины</h3>
            <p>Работайте с EvoHime в ваших любимых инструментах.</p>
          </div>
          <div className="pluginsActions">
            <button type="button" className="pluginsGearButton" aria-label="Настройки плагинов">
              ⚙
            </button>
          </div>
        </section>

        <div className="pluginsSearchRow">
          <label className="pluginsSearch">
            <span>Искать плагины</span>
            <input placeholder="Искать плагины" />
          </label>
        </div>

        <div className="pluginsBody">
          <section className="pluginsInstalled">
            <div className="pluginsSectionHeader">
              <h4>Установленные</h4>
            </div>
            <div className="pluginsInstalledList">
              <div className="pluginsInstalledEmpty">
                <strong>Пока нет плагинов</strong>
                <p>Когда подключим каталог, они появятся здесь.</p>
              </div>
            </div>
          </section>

          <section className="pluginsSwitcherRow">
            <div className="pluginsTabs">
              <button type="button" className="pluginsTab active">
                Общедоступные
              </button>
              <button type="button" className="pluginsTab">
                Личные
              </button>
            </div>
            <button type="button" className="pluginsFilterButton" aria-label="Фильтр">
              ≡
            </button>
          </section>

          <section className="pluginsCatalog">
            <div className="pluginsSectionHeader">
              <h4>Featured</h4>
            </div>
            <div className="pluginsGrid">
              {featuredPlugins.map((plugin) => (
                <article key={plugin.name} className="pluginCard">
                  <span className="pluginIcon">{plugin.icon}</span>
                  <div className="pluginBody">
                    <div className="pluginTopRow">
                      <strong>{plugin.name}</strong>
                      <button type="button" className="pluginMenuButton" aria-label={`Меню ${plugin.name}`}>
                        ···
                      </button>
                    </div>
                    <p>{plugin.description}</p>
                    <small>{plugin.tag}</small>
                  </div>
                  {plugin.actionLabel ? (
                    <button type="button" className="pluginInstallButton">
                      {plugin.actionLabel}
                    </button>
                  ) : null}
                </article>
              ))}
            </div>
          </section>

          <section className="pluginsCatalog">
            <div className="pluginsSectionHeader">
              <h4>Productivity</h4>
            </div>
            <div className="pluginsGrid pluginsGridCompact">
              {["Notion", "Google Calendar", "Linear", "ClickUp"].map((name) => (
                <article key={name} className="pluginCard pluginCardCompact">
                  <span className="pluginIcon">◌</span>
                  <div className="pluginBody">
                    <div className="pluginTopRow">
                      <strong>{name}</strong>
                      <button type="button" className="pluginMenuButton" aria-label={`Меню ${name}`}>
                        ···
                      </button>
                    </div>
                    <p>Подключение к {name} для работы из Codex.</p>
                  </div>
                  <button type="button" className="pluginInstallButton">
                    Установить
                  </button>
                </article>
              ))}
            </div>
          </section>
          </div>
      </div>
    );
  }

  function renderSitesContent() {
    return (
      <div className="sitesPage">
        <section className="sitesHero">
          <div>
            <h3>Сайты</h3>
            <p>Превратите свои идеи в готовые сайты.</p>
          </div>
        </section>

        <div className="sitesSearchRow">
          <label className="sitesSearch">
            <span className="sitesSearchIcon" aria-hidden="true">
              ⌕
            </span>
            <input
              value={siteSearch}
              onChange={(event) => setSiteSearch(event.target.value)}
              placeholder="Поиск сайтов"
              aria-label="Поиск сайтов"
            />
          </label>
        </div>

        <div className="sitesBody">
          <div className="sitesEmptyState">
            <div className="sitesEmptyIcon" aria-hidden="true">
              ▢
            </div>
            <strong>Сайтов пока нет</strong>
            <button type="button" className="sitesCreateButton">
              Создать новый сайт
            </button>
          </div>
        </div>
      </div>
    );
  }

  function renderPanelContent() {
    if (activePanel === "settings") {
      return renderSettingsContent();
    }

    if (activePanel === "plugins") {
      return renderPluginsContent();
    }

    if (activePanel === "sites") {
      return renderSitesContent();
    }

    if (activePanel === "files") {
      const rootEntries = directoryCache["."] ?? [];
      return (
        <div className="filesPanel">
          <div className="panelToolbar">
            <div>
              <strong>Дерево рабочего пространства</strong>
              <span>{rootEntries.length} элементов в корне</span>
            </div>
            <button type="button" onClick={() => void refreshDirectory(".")}>Обновить дерево</button>
          </div>
          <div className="createFileForm">
            <input
              value={newFilePath}
              onChange={(event) => setNewFilePath(event.target.value)}
              placeholder="путь/до/нового-файла.ts"
              aria-label="Путь нового файла"
            />
            <input
              value={newFileContent}
              onChange={(event) => setNewFileContent(event.target.value)}
              placeholder="Начальное содержимое"
              aria-label="Содержимое нового файла"
            />
            <button type="button" onClick={() => void handleCreateFile()} disabled={!newFilePath.trim()}>
              Создать файл
            </button>
          </div>
          <div className="fileTree">
            {renderTree(".")}
          </div>
        </div>
      );
    }

    if (activePanel === "editor") {
      return (
        <div className="editorPanel">
          <div className="panelToolbar">
            <div>
              <strong>{selectedFilePath ?? "Файл не выбран"}</strong>
              <span>
                {selectedFileLoading
                  ? "Загрузка..."
                  : saveState === "saving"
                    ? "Сохранение..."
                    : saveState === "saved"
                      ? "Сохранено"
                      : selectedFilePath && selectedFileContent !== selectedFileOriginal
                        ? "Есть несохранённые изменения"
                        : "Готово"}
              </span>
            </div>
            <div className="toolbarActions">
              <button
                type="button"
                onClick={() => void refreshSelectedFile(selectedFilePath ?? ".")}
                disabled={!selectedFilePath || selectedFileLoading}
              >
                Перезагрузить
              </button>
              <button
                type="button"
                onClick={() => void handleSave()}
                disabled={!selectedFilePath || selectedFileContent === selectedFileOriginal}
              >
                Сохранить
              </button>
            </div>
          </div>
          {selectedFileNotice ? <div className="editorNotice">{selectedFileNotice}</div> : null}
          {selectedFilePath ? (
            <div className="editorMeta">
              <span>Язык: {selectedFileLanguage}</span>
              <span>Размер: {formatFileSize(selectedFileContent.length)}</span>
              <span>{selectedFileContent === selectedFileOriginal ? "Чисто" : "Есть изменения"}</span>
            </div>
          ) : null}
          {selectedFilePath ? (
            <Editor
              height="100%"
              theme="vs-dark"
              language={selectedFileLanguage}
              value={selectedFileContent}
              onChange={(value) => {
                setSelectedFileContent(value ?? "");
                setSaveState("idle");
              }}
              onMount={(editor, monaco) => {
                editor.addAction({
                  id: "evohime-save-file",
                  label: "Сохранить файл",
                  keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS],
                  run: () => saveFileRef.current(),
                });
              }}
              options={{
                minimap: { enabled: false },
                fontSize: 14,
                automaticLayout: true,
                scrollBeyondLastLine: false,
                wordWrap: "on",
              }}
            />
          ) : (
            <div className="placeholderPanel">
              <h3>Выберите файл</h3>
              <p>Выбери файл во вкладке «Файлы», чтобы открыть его в редакторе Monaco.</p>
            </div>
          )}
        </div>
      );
    }

    if (activePanel === "git") {
      return (
        <div className="gitPanel">
          <div className="panelToolbar">
            <div>
              <strong>Состояние репозитория</strong>
              <span>
                {gitSummary.branch}
                {gitSummary.changed ? ` • изменено: ${gitSummary.changed}` : " • чисто"}
              </span>
            </div>
            <div className="toolbarActions">
              <button type="button" onClick={() => void refreshGitSnapshot(gitDiffPathInput || undefined)}>
                Обновить Гит
              </button>
            </div>
          </div>
          <div className="gitControls">
            <label>
              <span>Путь diff</span>
              <input
                value={gitDiffPathInput}
                onChange={(event) => setGitDiffPathInput(event.target.value)}
                placeholder="Корень репозитория или путь к файлу"
              />
            </label>
            <div className="gitControlButtons">
              <button type="button" onClick={() => void refreshGitSnapshot(gitDiffPathInput || undefined)}>
                Загрузить diff
              </button>
              <button
                type="button"
                onClick={() => {
                  const nextPath = selectedFilePath ?? "";
                  setGitDiffPathInput(nextPath);
                  void refreshGitSnapshot(nextPath || undefined);
                }}
                disabled={!selectedFilePath}
              >
                Использовать выбранный файл
              </button>
            </div>
            <label>
              <span>Сообщение коммита</span>
              <input
                value={gitCommitMessage}
                onChange={(event) => setGitCommitMessage(event.target.value)}
                placeholder="Опиши изменение"
              />
            </label>
            <div className="gitRemoteFields">
              <label>
                <span>Удалённый репозиторий</span>
                <input value={gitRemote} onChange={(event) => setGitRemote(event.target.value)} />
              </label>
              <label>
                <span>Ветка</span>
                <input value={gitBranch} onChange={(event) => setGitBranch(event.target.value)} placeholder="Текущая" />
              </label>
            </div>
            <div className="gitControlButtons">
              <button type="button" onClick={() => void handleGitAction("commit")} disabled={!gitCommitMessage.trim() || Boolean(gitAction)}>
                {gitAction === "commit" ? "Коммитим..." : "Коммит"}
              </button>
              <button type="button" onClick={() => void handleGitAction("pull")} disabled={Boolean(gitAction)}>
                {gitAction === "pull" ? "Забираем..." : "Забрать"}
              </button>
              <button type="button" onClick={() => void handleGitAction("push")} disabled={Boolean(gitAction)}>
                {gitAction === "push" ? "Отправляем..." : "Отправить"}
              </button>
            </div>
            {gitActionNotice ? <p className="gitActionNotice">{gitActionNotice}</p> : null}
          </div>
          <div className="gitSummary">
            <h3>Статус</h3>
            <pre>{gitStatus}</pre>
          </div>
          <div className="gitSummary">
            <h3>Изменения{gitDiffPath ? ` · ${gitDiffPath}` : ""}</h3>
            <pre className="gitDiffViewer">
              {(gitDiff || "Нет изменений").split("\n").map((line, index) => (
                <span
                  className={line.startsWith("+") && !line.startsWith("+++") ? "diffAdded" : line.startsWith("-") && !line.startsWith("---") ? "diffRemoved" : line.startsWith("@@") ? "diffContext" : ""}
                  key={`${index}-${line}`}
                >
                  {line || " "}
                </span>
              ))}
            </pre>
          </div>
        </div>
      );
    }

    if (activePanel === "terminal") return <TerminalPanel entries={terminalEntries} />;

    if (activePanel === "tasks") {
      return <div className="tasksPanel">{Object.values(tasks).map((task) => <article className="taskCard" key={task.id}>
        <strong>{translateTaskStatus(task.status)}</strong><p>{task.message}</p><small>{task.id}</small>
        <div className="taskActions">
          {(task.status === "running" || task.status === "cancelling") && <button type="button" onClick={() => sendTaskCommand("task.cancel", task.id)}>Отменить</button>}
          {(task.status === "paused" || task.status === "cancelled") && <button type="button" onClick={() => sendTaskCommand("task.resume", task.id)}>Продолжить</button>}
          {task.status === "failed" && <button type="button" onClick={() => sendTaskCommand("task.retry", task.id)}>Повторить</button>}
        </div>
        <ul>{Object.entries(task.steps).map(([name, status]) => <li key={name}>{name}: {translateStepStatus(status)}</li>)}</ul>
      </article>)}</div>;
    }

    if (activePanel === "actions") {
      return <div className="actionsPanel">{actions.map((action, index) => <article className="actionItem" key={`${action.taskId}-${index}`}><strong>{action.action}</strong><span>{action.detail}</span><small>{action.createdAt}</small></article>)}</div>;
    }

    if (activePanel === "pull-requests") {
      return (
        <div className="pullRequestsPage">
          <section className="pullRequestsHero">
            <div>
              <h3>Пул-реквесты</h3>
              <p>
                Просматривайте и отслеживайте работу на GitHub от имени {githubAuth?.login ?? "вашего аккаунта"}.
              </p>
            </div>
            <div className="pullRequestsMeta">
              <strong>{pullRequestsLoading ? "…" : `${visiblePullRequests.length}`}</strong>
              <span>pull request'ов</span>
            </div>
          </section>

          <div className="pullRequestsSearchRow">
            <label className="pullRequestsSearch">
              <span>Поиск pull-request'ов</span>
              <input
                value={pullRequestSearch}
                onChange={(event) => setPullRequestSearch(event.target.value)}
                placeholder="Поиск pull-request'ов"
              />
            </label>
            <button type="button" className="pullRequestsFilterButton" aria-label="Фильтр">
              ⌕
            </button>
          </div>

          <div className="pullRequestsTabs">
            <button
              type="button"
              className={pullRequestScope === "all" ? "pullRequestsTab active" : "pullRequestsTab"}
              onClick={() => setPullRequestScope("all")}
            >
              Все
            </button>
            <button
              type="button"
              className={pullRequestScope === "review_requested" ? "pullRequestsTab active" : "pullRequestsTab"}
              onClick={() => setPullRequestScope("review_requested")}
            >
              Проверяемые мной
            </button>
            <button
              type="button"
              className={pullRequestScope === "created" ? "pullRequestsTab active" : "pullRequestsTab"}
              onClick={() => setPullRequestScope("created")}
            >
              Созданные мной
            </button>
          </div>

          <div className="pullRequestsBody">
            {pullRequestsError ? <p className="pullRequestsError">{pullRequestsError}</p> : null}
            <div className="pullRequestsList">
              {visiblePullRequests.length === 0 ? (
                <div className="pullRequestsEmpty">
                  <strong>Пока нет pull request'ов</strong>
                  <p>
                    {pullRequestsLoading
                      ? "Подтягиваю список из GitHub..."
                      : "Если в репозитории будут pull request'ы, они появятся здесь."}
                  </p>
                </div>
              ) : (
                visiblePullRequests.map((pullRequest) => (
                  <a
                    key={pullRequest.number}
                    className="pullRequestItem"
                    href={pullRequest.url}
                    target="_blank"
                    rel="noreferrer"
                  >
                    <div className="pullRequestLine">
                      <strong>{pullRequest.title}</strong>
                      <span>{formatRelativeAge(pullRequest.updatedAt)}</span>
                    </div>
                    <div className="pullRequestSubline">
                      <span>
                        {pullRequest.author?.login ?? "unknown"} / {pullRequest.headRefName}
                      </span>
                      <span>{pullRequest.baseRefName}</span>
                    </div>
                    <div className="pullRequestFooter">
                      <span className="pullRequestState">{pullRequest.state}</span>
                      <span>#{pullRequest.number}</span>
                    </div>
                  </a>
                ))
              )}
            </div>
          </div>
        </div>
      );
    }

    if (activePanel !== "chat") {
      const panel = workspacePanels.find((item) => item.id === activePanel);
      return (
        <div className="placeholderPanel">
          <h3>{panel?.label}</h3>
          <p>Запланировано на {panel?.phase}.</p>
        </div>
      );
    }

    return (
      <>
        <div className={`chatLog${lines.length === 1 && lines[0]?.role === "system" && !stream ? " empty" : ""}`}>
          {lines.length === 1 && lines[0]?.role === "system" && !stream ? (
            <div className="chatWelcome">
              <span className="chatWelcomeIcon">✦</span>
              <p className="eyebrow">Новая задача</p>
              <h3>Что будем делать?</h3>
              <p className="chatWelcomeText">
                Опиши задачу обычным языком — я помогу разобраться в проекте, изменить файлы или проверить результат.
              </p>
              <div className="chatWelcomeHints">
                <span>Разобраться в коде</span>
                <span>Изменить файл</span>
                <span>Запустить проверку</span>
              </div>
            </div>
          ) : (
            lines.map((line, index) => (
              <article className={`line ${line.role}`} key={`${line.role}-${index}`}>
                <strong>{translateChatRole(line.role)}</strong>
                <pre>{line.text}</pre>
              </article>
            ))
          )}
          {stream ? (
            <article className="line assistant streaming">
              <strong>Ассистент</strong>
              <pre>{stream}</pre>
            </article>
          ) : null}
        </div>
        <form onSubmit={sendMessage} className="composer">
          <select
            value={selectedModelRoute}
            onChange={(event) => setSelectedModelRoute(event.target.value)}
            disabled={!modelConfig || modelConfig.routes.length === 0}
            aria-label="Маршрут модели"
          >
            {modelConfig?.routes.map((route) => (
              <option key={route.name} value={route.name}>
                {route.name}
              </option>
            ))}
          </select>
          <input
            value={input}
            onChange={(event) => setInput(event.target.value)}
            placeholder="Введите сообщение..."
          />
          <button type="submit" disabled={socketState !== "connected"}>
            Отправить
          </button>
        </form>
      </>
    );
  }

  return (
    <main className="shell">
      <header className="topBar">
        <div className="agentBrand">
          <h1>EvoHime</h1>
        </div>
        <div className="statusCard">
          <span className="statusDot" data-state={socketState} />
          <div>
            <strong>{connectedLabel}</strong>
            <span>{session ? session.session_id : "сессия ещё не создана"}</span>
          </div>
        </div>
      </header>

      <section className="workspace">
        <nav className="sidebar">
          <div className="sidebarTop">
            <button type="button" className="sidebarSearchButton" aria-label="Поиск">
              ⌕
            </button>
          </div>

          <section className="sidebarSection">
            <div className="quickLinks">
              {sidebarQuickLinks.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className={item.panel === activePanel ? "quickLink active" : "quickLink"}
                  onClick={() => setActivePanel(item.panel)}
                >
                  <span className="quickLinkIcon">{item.icon}</span>
                  <span>{item.label}</span>
                </button>
              ))}
            </div>
          </section>

          <section className="sidebarSection">
            <header className="sidebarHeader">
              <strong>Проекты</strong>
            </header>
            <button
              type="button"
              className="projectCard"
              onClick={() => setActivePanel("chat")}
            >
              <span className="projectIcon">⌂</span>
              <span className="projectName">{activeProjectLabel}</span>
            </button>
            <div className="projectChatList">
              {chatSessions.map((chat, index) => (
                <button
                  key={chat.session_id}
                  type="button"
                  className={chat.session_id === activeSessionId ? "projectChatItem active" : "projectChatItem"}
                  onClick={() => {
                    void openSession(chat).catch((error) => {
                      setSocketState("failed");
                      setLines((current) => [
                        ...current,
                        { role: "system", text: String(error) },
                      ]);
                    });
                  }}
                >
                  <span className="projectChatTitle">{formatSessionTitle(chat, index)}</span>
                  <span className="projectChatStatus" />
                </button>
              ))}
            </div>
          </section>

          <section className="sidebarSection">
            <header className="sidebarHeader">
              <strong>Задачи</strong>
            </header>
            <button
              type="button"
              className="taskSummaryCard"
              onClick={() => setActivePanel("tasks")}
            >
              {pendingTaskCount > 0 ? (
                <>
                  <strong>{pendingTaskCount}</strong>
                  <span>активных задач</span>
                </>
              ) : (
                <>
                  <strong>Нет задач</strong>
                  <span>Пока тихо, не нагружайся раньше времени</span>
                </>
              )}
            </button>
          </section>

          <section className="sidebarFooter">
            <div className="sidebarFooterTop">
              <div className="profileChip">
                <span className="profileAvatar">{formatProfileInitials(githubAuth?.login ?? null)}</span>
                <div className="profileText">
                  <strong>{githubAuth?.login ?? "не авторизован"}</strong>
                  <small>{githubAuth?.authenticated ? `gh` : "Вход через gh"}</small>
                </div>
              </div>
              <button
                type="button"
                className="settingsGear"
                onClick={() => setSettingsOpen(true)}
                aria-label="Открыть настройки"
                title="Открыть настройки"
              >
                ⚙
              </button>
            </div>
          </section>
        </nav>

        <div className="panel mainPanel">
          {activePanel !== "pull-requests" && activePanel !== "plugins" && activePanel !== "sites" ? (
            <header>
              <h2>{currentPanelLabel}</h2>
              <span>Веб-сокет</span>
            </header>
          ) : null}
          {renderPanelContent()}
        </div>

        <div className="panel timelinePanel">
          <header>
            <h2>События</h2>
            <span>{events.length}</span>
          </header>
          <div className="eventList">
            {events.map((event, index) => (
              <article key={`${event.type}-${index}`} className="eventItem">
                <strong>{translateEventType(event.type)}</strong>
                <code>{JSON.stringify(event, null, 2)}</code>
              </article>
            ))}
          </div>
        </div>
      </section>
      {settingsOpen ? (
        <div
          className="settingsBackdrop"
          onClick={() => setSettingsOpen(false)}
          role="presentation"
        >
          <section className="settingsModal" onClick={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-label="Настройки">
            <header className="settingsModalHeader">
              <div>
                <span className="sidebarFooterLabel">Настройки</span>
                <h2>Параметры EvoHime</h2>
              </div>
              <button type="button" className="settingsCloseButton" onClick={() => setSettingsOpen(false)}>
                Закрыть
              </button>
            </header>
            <div className="settingsModalBody">{renderSettingsContent()}</div>
          </section>
        </div>
      ) : null}
      {approval ? <ApprovalModal request={approval} onGrant={() => resolveApproval("approval.granted")} onDeny={() => resolveApproval("approval.denied")} /> : null}
    </main>
  );
}

function formatPlan(plan: PlanStep[]) {
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
