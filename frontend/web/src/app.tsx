import { ChangeEvent, FormEvent, Fragment, UIEvent, useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type {
  ClientCommand,
  ServerEvent,
  SessionBootstrap,
} from "./protocol";
import type { ApprovalRequiredEvent, MemoryAskEvent } from "./protocol";
import { TerminalPanel, TerminalEntry } from "./components/TerminalPanel";
import { ApprovalModal } from "./components/ApprovalModal";
import { MemoryAskModal } from "./components/MemoryAskModal";
import { AgentAvatar } from "./components/AgentAvatar";
import { AgentBrand } from "./components/AgentBrand";
import { AgentMark } from "./components/AgentMark";
import { ActionsPanel } from "./panels/ActionsPanel";
import { EditorPanel } from "./panels/EditorPanel";
import { FilesPanel } from "./panels/FilesPanel";
import { GitPanel } from "./panels/GitPanel";
import { PluginsPanel } from "./panels/PluginsPanel";
import { MemoryPanel } from "./panels/MemoryPanel";
import { PullRequestsPanel } from "./panels/PullRequestsPanel";
import { ScheduledPanel } from "./panels/ScheduledPanel";
import { SettingsPanel } from "./panels/SettingsPanel";
import { SitesPanel } from "./panels/SitesPanel";
import { TasksPanel } from "./panels/TasksPanel";
import { useServerEventHandler } from "./hooks/useServerEventHandler";
import {
  filesApi,
  gitApi,
  githubApi,
  mcpApi,
  modelsApi,
  permissionsApi,
  projectsApi,
  sessionsApi,
} from "./api";
import { websocketUrl } from "./api/client";
import {
  chatMatchesProject,
  formatProfileInitials,
  formatSessionPreview,
  formatSessionTitle,
  summarizeChatTitle,
  summarizeGitStatus,
  translateChatRole,
  translateGitAction,
  translateModelConfigStatus,
  translatePermissionMode,
  translateSaveState,
  translateSocketState,
  translateStepStatus,
  translateTaskStatus,
} from "./lib/format";
import {
  formatFileSize,
  inferMonacoLanguage,
  normalizePath,
  parentPath,
  sortFileNodes,
} from "./lib/paths";
import {
  loadProjectComposerPreference,
  loadSelectedProject,
  loadTraceOpen,
  projectPreferenceKey,
  saveProjectComposerPreference,
  saveSelectedProject,
  saveTraceOpen,
} from "./lib/storage";
import type {
  ActionView,
  ChatLine,
  ChatSessionSummary,
  FileNode,
  GitAction,
  GithubAuthInfo,
  McpServerConfig,
  ModelConfig,
  ModelRouteDraft,
  PermissionAuditEntry,
  PermissionMode,
  PermissionScopes,
  PermissionSettings,
  ProjectSelection,
  ProjectSummary,
  PullRequestScope,
  PullRequestSummary,
  SettingsTab,
  TaskView,
  ToolDefinition,
  WorkspacePanel,
} from "./types";
import { sidebarQuickLinks, workspacePanels } from "./types";

const initialLines: ChatLine[] = [];

export function App() {
  const [session, setSession] = useState<SessionBootstrap | null>(null);
  const [chatSessions, setChatSessions] = useState<ChatSessionSummary[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [socketState, setSocketState] = useState<
    "idle" | "connecting" | "connected" | "failed"
  >("idle");
  const [input, setInput] = useState("");
  const [lines, setLines] = useState<ChatLine[]>(initialLines);
  const [stream, setStream] = useState("");
  const [chatActionNotice, setChatActionNotice] = useState<string | null>(null);
  const [composerNotice, setComposerNotice] = useState<string | null>(null);
  const [activePanel, setActivePanel] = useState<WorkspacePanel>("chat");
  const [traceOpen, setTraceOpen] = useState(loadTraceOpen);
  const [selectedProject, setSelectedProject] = useState<ProjectSelection>(loadSelectedProject);
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [projectPickerOpen, setProjectPickerOpen] = useState(false);
  const [projectSearch, setProjectSearch] = useState("");
  const [newProjectName, setNewProjectName] = useState("");
  const [projectCreating, setProjectCreating] = useState(false);
  const [projectCreateError, setProjectCreateError] = useState<string | null>(null);
  const [modelConfig, setModelConfig] = useState<ModelConfig | null>(null);
  const [modelConfigError, setModelConfigError] = useState<string | null>(null);
  const [selectedModelRoute, setSelectedModelRoute] = useState("");
  const [composerModels, setComposerModels] = useState<string[]>([]);
  const [selectedComposerModel, setSelectedComposerModel] = useState(() => loadProjectComposerPreference(loadSelectedProject().path).model ?? "");
  const [composerModelsLoading, setComposerModelsLoading] = useState(false);
  const [composerModelsError, setComposerModelsError] = useState<string | null>(null);
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
  const [memoryAsk, setMemoryAsk] = useState<MemoryAskEvent | null>(null);
  const [githubAuth, setGithubAuth] = useState<GithubAuthInfo | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("model");
  const [modelDefaultRoute, setModelDefaultRoute] = useState("default");
  const [modelDrafts, setModelDrafts] = useState<ModelRouteDraft[]>([]);
  const [modelSaving, setModelSaving] = useState(false);
  const [modelNotice, setModelNotice] = useState<string | null>(null);
  const [orchestratorModels, setOrchestratorModels] = useState<string[]>([]);
  const modelAutosaveInitializedRef = useRef(false);
  const skipNextModelAutosaveRef = useRef(false);
  const [pullRequests, setPullRequests] = useState<PullRequestSummary[]>([]);
  const [pullRequestsLoading, setPullRequestsLoading] = useState(false);
  const [pullRequestsError, setPullRequestsError] = useState<string | null>(null);
  const [pullRequestScope, setPullRequestScope] = useState<PullRequestScope>("all");
  const [pullRequestSearch, setPullRequestSearch] = useState("");
  const [siteSearch, setSiteSearch] = useState("");
  const [terminalEntries, setTerminalEntries] = useState<TerminalEntry[]>([]);
  const [permissionSettings, setPermissionSettings] = useState<PermissionSettings>({});
  const [permissionAudit, setPermissionAudit] = useState<PermissionAuditEntry[]>([]);
  const [permissionScopes, setPermissionScopes] = useState<PermissionScopes | null>(null);
  const [permissionModeSaving, setPermissionModeSaving] = useState(false);
  const [attachments, setAttachments] = useState<File[]>([]);
  const [workModeOpen, setWorkModeOpen] = useState(false);
  const [modelPickerOpen, setModelPickerOpen] = useState(false);
  const restoredProjectPreferenceRef = useRef<string | null>(null);
  const [deletingSessionId, setDeletingSessionId] = useState<string | null>(null);
  const [archivedChats, setArchivedChats] = useState<ChatSessionSummary[]>([]);
  const [toolCatalog, setToolCatalog] = useState<ToolDefinition[]>([]);
  const [toolCatalogError, setToolCatalogError] = useState<string | null>(null);
  const [mcpServers, setMcpServers] = useState<McpServerConfig[]>([]);
  const [mcpServersError, setMcpServersError] = useState<string | null>(null);
  const [mcpServersSaving, setMcpServersSaving] = useState(false);
  const [mcpServersNotice, setMcpServersNotice] = useState<string | null>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const attachmentInputRef = useRef<HTMLInputElement | null>(null);
  const applyEventRef = useRef<(event: ServerEvent) => void>(() => undefined);
  const saveFileRef = useRef<() => void>(() => undefined);
  const sessionLoadRef = useRef(0);
  const chatLogRef = useRef<HTMLDivElement | null>(null);
  const chatAutoScrollRef = useRef(true);

  useEffect(() => {
    let cancelled = false;

    const loadModelConfig = async () => {
      const data = await modelsApi.getModelConfig();
      if (!cancelled) {
        const routes = data.routes.map((route) => ({ ...route, api_key: "", configured: route.configured }));
        if (!routes.some((route) => route.name === "orchestrator")) {
          const mainRoute = routes.find((route) => route.name === data.default_route) ?? routes[0];
          if (mainRoute) {
            routes.push({ ...mainRoute, name: "orchestrator", api_key: "" });
          }
        }
        setModelConfig(data);
        setModelDefaultRoute(data.default_route);
        setModelDrafts(routes);
      }
    };
    permissionsApi.getPermissions().then((data) => setPermissionSettings(data)).catch(() => undefined);
    permissionsApi
      .getPermissionAudit()
      .then((data) => setPermissionAudit(data.entries ?? []))
      .catch(() => undefined);
    permissionsApi
      .getPermissionScopes()
      .then((data) => setPermissionScopes(data))
      .catch(() => undefined);
    sessionsApi.listArchivedSessions().then((data) => setArchivedChats(data)).catch(() => undefined);
    mcpApi.listTools()
      .then((data) => {
        if (!cancelled) {
          setToolCatalog(data);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setToolCatalogError(String(error));
        }
      });
    mcpApi.listMcpServers()
      .then((data) => {
        if (!cancelled) {
          setMcpServers(data);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setMcpServersError(String(error));
        }
      });
    githubApi.getGithubAuth()
      .then((data) => {
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
      const data = await sessionsApi.listSessions();
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

      const bootstrap = await sessionsApi.createSession();
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
    if (!projectPickerOpen) {
      return;
    }
    projectsApi.listProjects()
      .then((data) => setProjects(data))
      .catch(() => undefined);
  }, [projectPickerOpen]);

  useEffect(() => {
    saveSelectedProject(selectedProject);
  }, [selectedProject]);

  useEffect(() => {
    saveTraceOpen(traceOpen);
  }, [traceOpen]);

  useEffect(() => {
    const closeFloatingMenus = (event: PointerEvent) => {
      const target = event.target as HTMLElement | null;
      if (!target?.closest(".composerMenu")) {
        setWorkModeOpen(false);
        setModelPickerOpen(false);
      }
      if (!target?.closest(".projectContext")) {
        setProjectPickerOpen(false);
      }
    };
    document.addEventListener("pointerdown", closeFloatingMenus);
    return () => document.removeEventListener("pointerdown", closeFloatingMenus);
  }, []);

  useEffect(() => {
    const route = modelDrafts.find((item) => item.name === "orchestrator");
    if (!route) {
      return;
    }
    let cancelled = false;
    modelsApi.getAvailableModels("orchestrator")
      .then((data) => {
        if (!cancelled) {
          setOrchestratorModels(data.models);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setOrchestratorModels([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [modelDrafts.find((route) => route.name === "orchestrator")?.provider, modelDrafts.find((route) => route.name === "orchestrator")?.base_url, modelDrafts.find((route) => route.name === "orchestrator")?.billing_mode]);

  useEffect(() => {
    if (!modelConfig || modelDrafts.length === 0) {
      return;
    }
    if (!modelAutosaveInitializedRef.current) {
      modelAutosaveInitializedRef.current = true;
      return;
    }
    if (skipNextModelAutosaveRef.current) {
      skipNextModelAutosaveRef.current = false;
      return;
    }
    const timer = window.setTimeout(() => {
      void saveModelConfig();
    }, 650);
    return () => window.clearTimeout(timer);
  }, [modelConfig, modelDrafts, modelDefaultRoute]);

  useEffect(() => {
    if (!modelConfig || !selectedModelRoute) {
      return;
    }
    const route = modelConfig.routes.find((item) => item.name === selectedModelRoute);
    if (!route) {
      return;
    }
    if (!route.configured) {
      setComposerModels([]);
      setSelectedComposerModel("");
      setComposerModelsError("Сначала укажите API-ключ провайдера в настройках");
      setComposerModelsLoading(false);
      return;
    }
    let cancelled = false;
    setComposerModelsLoading(true);
    setComposerModelsError(null);
    modelsApi.getAvailableModels(selectedModelRoute)
      .then((data) => {
        if (!cancelled) {
          const models = data.models.length > 0 ? data.models : [route.model];
          setComposerModels(models);
          setSelectedComposerModel((current) => models.includes(current) ? current : route.model);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setComposerModels([]);
          setSelectedComposerModel("");
          setComposerModelsError(String(error));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setComposerModelsLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [modelConfig, selectedModelRoute]);

  useEffect(() => {
    let cancelled = false;

    const loadPullRequests = async (scope: PullRequestScope) => {
      setPullRequestsLoading(true);
      setPullRequestsError(null);
      try {
        const data = await githubApi.listPullRequests(scope);
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
    const socket = new WebSocket(websocketUrl(`/ws/${session.session_id}`));
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
  const activeProjectLabel = selectedProject.label;
  const projectChatSessions = useMemo(
    () => chatSessions.filter((chat) => chatMatchesProject(chat, selectedProject)),
    [chatSessions, selectedProject],
  );
  const standaloneChatSessions = useMemo(
    () => chatSessions.filter((chat) => !chat.workspace_path),
    [chatSessions],
  );
  const activeChatTitle = useMemo(
    () => chatSessions.find((chat) => chat.session_id === activeSessionId)?.title?.trim() || "Новый чат",
    [chatSessions, activeSessionId],
  );
  const projectFolders = useMemo(
    () => projects.filter((project) => project.name.toLowerCase().includes(projectSearch.trim().toLowerCase())),
    [projects, projectSearch],
  );
  const hasConversation = lines.some((line) => line.role !== "system" && line.text.trim()) || Boolean(stream.trim());
  const traceLines = useMemo(
    () => lines.filter((line) => line.role === "system" || line.role === "tool"),
    [lines],
  );
  const visibleChatLines = useMemo(
    () => lines.filter((line) => line.role !== "system" && line.role !== "tool"),
    [lines],
  );
  const lastAssistantLineIndex = useMemo(
    () => visibleChatLines.reduce((last, line, index) => line.role === "assistant" ? index : last, -1),
    [visibleChatLines],
  );
  const selectedFileLanguage = useMemo(
    () => inferMonacoLanguage(selectedFilePath),
    [selectedFilePath],
  );
  const gitSummary = useMemo(() => summarizeGitStatus(gitStatus), [gitStatus]);
  const activeTaskId = useMemo(
    () => Object.values(tasks).find((task) => task.status === "running" || task.status === "cancelling")?.id ?? null,
    [tasks],
  );
  useEffect(() => {
    if (activePanel !== "chat" || !chatAutoScrollRef.current) {
      return;
    }

    const frame = window.requestAnimationFrame(() => {
      const chatLog = chatLogRef.current;
      if (chatLog) {
        chatLog.scrollTop = chatLog.scrollHeight;
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, [activePanel, activeSessionId, lines, stream]);

  function handleChatScroll(event: UIEvent<HTMLDivElement>) {
    const chatLog = event.currentTarget;
    const distanceFromBottom = chatLog.scrollHeight - chatLog.scrollTop - chatLog.clientHeight;
    chatAutoScrollRef.current = distanceFromBottom <= 72;
  }
  const workMode = useMemo<PermissionMode | "mixed">(() => {
    const modes = Object.values(permissionSettings).map((setting) => setting.mode);
    if (modes.length === 0 || modes.every((mode) => mode === modes[0])) {
      return modes[0] ?? "ask";
    }
    return "mixed";
  }, [permissionSettings]);
  useEffect(() => {
    const preference = loadProjectComposerPreference(selectedProject.path);
    const key = projectPreferenceKey(selectedProject.path);
    if (restoredProjectPreferenceRef.current === key || Object.keys(permissionSettings).length === 0) {
      return;
    }
    restoredProjectPreferenceRef.current = key;
    if (preference.model) {
      setSelectedComposerModel(preference.model);
    }
    if (preference.workMode && preference.workMode !== workMode) {
      void updateWorkMode(preference.workMode);
    }
  }, [selectedProject.path, permissionSettings, workMode]);

  useEffect(() => {
    if (restoredProjectPreferenceRef.current !== projectPreferenceKey(selectedProject.path)) {
      return;
    }
    saveProjectComposerPreference(selectedProject.path, {
      model: selectedComposerModel || undefined,
      workMode: workMode === "mixed" ? undefined : workMode,
    });
  }, [selectedProject.path, selectedComposerModel, workMode]);
  const activeModelRouteIndex = Math.max(
    0,
    modelDrafts.findIndex((route) => route.name === modelDefaultRoute),
  );
  const activeModelRoute = modelDrafts[activeModelRouteIndex] ?? null;
  const orchestratorRouteIndex = modelDrafts.findIndex((route) => route.name === "orchestrator");
  const orchestratorRoute = orchestratorRouteIndex >= 0 ? modelDrafts[orchestratorRouteIndex] : null;
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

  const applyEvent = useServerEventHandler({
    selectedProjectPath: selectedProject.path ?? ".",
    selectedFilePath,
    selectedFileContent,
    selectedFileOriginal,
    setLines,
    setChatSessions,
    setTasks,
    setStream,
    setTerminalEntries,
    setApproval,
    setMemoryAsk,
    setActions,
    setSelectedFileNotice,
    setGitStatus,
    setGitDiff,
    refreshDirectory,
    refreshSelectedFile,
  });
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
    setStream("");
    setTasks({});
    setActions([]);
    setApproval(null);
    setMemoryAsk(null);
    setTerminalEntries([]);
    chatAutoScrollRef.current = true;
    for (const item of history) {
      applyEventRef.current(item.event);
    }
  }

  async function openSession(summary: ChatSessionSummary) {
    const requestId = sessionLoadRef.current + 1;
    sessionLoadRef.current = requestId;
    setActiveSessionId(summary.session_id);

    const history = await sessionsApi.getSessionHistory(summary.session_id);
    if (requestId !== sessionLoadRef.current) {
      return;
    }

    hydrateSession(summary, history);
  }

  async function createNewChat() {
    const bootstrap = await sessionsApi.createSession();
    const createdSummary: ChatSessionSummary = {
      session_id: bootstrap.session_id,
      created_at: bootstrap.created_at,
      last_message_at: null,
      last_message: null,
      last_role: null,
    };
    setChatSessions((current) => [createdSummary, ...current]);
    setActivePanel("chat");
    hydrateSession(createdSummary, bootstrap.events);
  }

  async function createProject() {
    const name = newProjectName.trim();
    if (!name) {
      return;
    }
    setProjectCreating(true);
    setProjectCreateError(null);
    try {
      const project = await projectsApi.createProject(name);
      setProjects((current) => [...current, project].sort((left, right) => left.name.localeCompare(right.name)));
      setSelectedProject({ label: project.name, path: project.path });
      setNewProjectName("");
      setProjectPickerOpen(false);
    } catch (error) {
      setProjectCreateError(error instanceof Error ? error.message : String(error).replace(/^Error:\s*/, ""));
    } finally {
      setProjectCreating(false);
    }
  }

  async function archiveChat(summary: ChatSessionSummary) {
    setDeletingSessionId(summary.session_id);
    try {
      await sessionsApi.archiveSession(summary.session_id);

      setChatSessions((current) => current.filter((chat) => chat.session_id !== summary.session_id));
      setArchivedChats((current) => [summary, ...current]);
      if (summary.session_id === activeSessionId) {
        const next = chatSessions.find((chat) => chat.session_id !== summary.session_id);
        if (next) {
          await openSession(next);
        } else {
          await createNewChat();
        }
      }
    } catch (error) {
      setLines((current) => [...current, { role: "system", text: String(error) }]);
    } finally {
      setDeletingSessionId(null);
    }
  }

  async function deleteSession(summary: ChatSessionSummary) {
    setDeletingSessionId(summary.session_id);
    try {
      await sessionsApi.deleteSession(summary.session_id);

      setArchivedChats((current) => current.filter((chat) => chat.session_id !== summary.session_id));
      const remaining = chatSessions.filter((chat) => chat.session_id !== summary.session_id);
      setChatSessions(remaining);
      if (summary.session_id !== activeSessionId) {
        return;
      }

      const next = remaining[0];
      if (next) {
        await openSession(next);
        return;
      }

      await createNewChat();
    } catch (error) {
      setLines((current) => [...current, { role: "system", text: String(error) }]);
    } finally {
      setDeletingSessionId(null);
    }
  }

  async function refreshDirectory(path: string) {
    const data = await filesApi.listFiles(normalizePath(path));
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
      const data = await filesApi.readFile(normalizePath(path));
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
    const statusData = await gitApi.getGitStatus();
    const diffData = await gitApi.getGitDiff(diffPath === "." ? null : diffPath);
    setGitStatus(statusData.status);
    setGitDiff(diffData.diff);
    setGitDiffPath(diffPath === "." ? null : diffPath);
    setGitDiffPathInput(diffPath === "." ? "" : diffPath);
  }

  async function updatePermission(name: string, mode: PermissionMode) {
    await permissionsApi.putPermission(name, mode);
    setPermissionSettings((current) => ({ ...current, [name]: { mode } }));
  }

  async function updateWorkMode(mode: PermissionMode) {
    const names = Object.keys(permissionSettings);
    if (names.length === 0) {
      return;
    }

    setPermissionModeSaving(true);
    try {
      await Promise.all(names.map((name) => permissionsApi.putPermission(name, mode)));
      setPermissionSettings(Object.fromEntries(names.map((name) => [name, { mode }])));
    } finally {
      setPermissionModeSaving(false);
    }
  }

  function handleAttachmentChange(event: ChangeEvent<HTMLInputElement>) {
    setAttachments(Array.from(event.target.files ?? []));
  }

  function updateModelDraft(index: number, patch: Partial<ModelRouteDraft>) {
    setModelDrafts((current) =>
      current.map((route, routeIndex) => (routeIndex === index ? { ...route, ...patch } : route)),
    );
  }

  function addModelRoute() {
    setModelDrafts((current) => [
      ...current,
      {
        name: `route-${current.length + 1}`,
        provider: "openai-compatible",
        model: "",
        base_url: "https://api.openai.com/v1",
        api_key: "",
        billing_mode: "paid",
      },
    ]);
  }

  function removeModelRoute(index: number) {
    setModelDrafts((current) => current.filter((_, routeIndex) => routeIndex !== index));
  }

  async function saveModelConfig() {
    setModelSaving(true);
    setModelNotice(null);
    try {
      const data = await modelsApi.putModelConfig({ default_route: modelDefaultRoute, routes: modelDrafts });
      skipNextModelAutosaveRef.current = true;
      setModelConfig(data);
      setModelDefaultRoute(data.default_route);
      setModelDrafts(data.routes.map((route) => ({ ...route, api_key: "", configured: route.configured })));
      setSelectedModelRoute(data.default_route);
      setModelNotice("Настройки модели применены к агенту.");
      setComposerNotice(null);
    } catch (error) {
      setModelNotice(String(error));
    } finally {
      setModelSaving(false);
    }
  }

  async function saveMcpServers() {
    setMcpServersSaving(true);
    setMcpServersNotice(null);
    try {
      const data = await mcpApi.putMcpServers(mcpServers);
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
      const data = await filesApi.saveFile(selectedFilePath, selectedFileContent, session.session_id);

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
      await filesApi.createFile(path, newFileContent, session.session_id);
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
    try {
      if (action === "commit") {
        await gitApi.gitCommit(session.session_id, gitCommitMessage);
      } else if (action === "pull") {
        await gitApi.gitPull(session.session_id, gitRemote || undefined, gitBranch || undefined);
      } else {
        await gitApi.gitPush(session.session_id, gitRemote || undefined, gitBranch || undefined);
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
    if (!modelConfig?.configured) {
      setComposerNotice("Сначала укажите API-ключ провайдера в настройках модели.");
      return;
    }
    setComposerNotice(null);

    const attachmentNote = attachments.length > 0
      ? `\n\nВложения: ${attachments.map((file) => file.name).join(", ")}`
      : "";
    const payload: ClientCommand = {
      type: "user.message",
      content: `${text}${attachmentNote}`,
      model_route: selectedModelRoute || undefined,
      model: selectedComposerModel || undefined,
      workspace_path: selectedProject.path ?? undefined,
    };

    setChatSessions((current) => current.map((chat) => chat.session_id === activeSessionId
      ? { ...chat, workspace_path: selectedProject.path }
      : chat));
    socketRef.current.send(JSON.stringify(payload));
    setInput("");
    setAttachments([]);
    if (attachmentInputRef.current) {
      attachmentInputRef.current.value = "";
    }
  }

  async function copyChat() {
    const content = [
      ...lines,
      ...(stream ? [{ role: "assistant" as const, text: stream }] : []),
    ]
      .map((line) => `${translateChatRole(line.role, githubAuth?.login)}:\n${line.text}`)
      .join("\n\n");
    try {
      await navigator.clipboard.writeText(content || "Чат пока пуст.");
      setChatActionNotice("Чат скопирован");
    } catch {
      setChatActionNotice("Не удалось скопировать чат");
    }
  }

  function exportTrace() {
    const entries = [
      ...lines,
      ...(stream ? [{ role: "assistant" as const, text: stream }] : []),
    ].map((line) => ({ role: line.role, text: line.text }));
    const payload = {
      format: "evohime.trace.v1",
      exported_at: new Date().toISOString(),
      session_id: session?.session_id ?? null,
      task_id: activeTaskId,
      entries,
    };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `evohime-trace-${new Date().toISOString().replace(/[:.]/g, "-")}.json`;
    link.click();
    URL.revokeObjectURL(url);
    setChatActionNotice("Трейс экспортирован");
  }

  function sendTaskCommand(type: "task.cancel" | "task.resume" | "task.retry", taskId: string) {
    if (socketRef.current?.readyState === WebSocket.OPEN) {
      socketRef.current.send(JSON.stringify({ type, task_id: taskId }));
    }
  }

  function stopCurrentTask() {
    if (activeTaskId) {
      sendTaskCommand("task.cancel", activeTaskId);
    }
  }

  function resolveApproval(type: "approval.granted" | "approval.denied") {
    if (approval && socketRef.current?.readyState === WebSocket.OPEN) {
      socketRef.current.send(JSON.stringify({ type, approval_id: approval.approval_id }));
      setApproval(null);
    }
  }

  function resolveMemoryAsk(type: "memory.accept" | "memory.reject") {
    if (memoryAsk && socketRef.current?.readyState === WebSocket.OPEN) {
      socketRef.current.send(JSON.stringify({ type, memory_id: memoryAsk.memory_id }));
      setMemoryAsk(null);
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

  function settingsPanelElement() {
    return (
      <SettingsPanel
        settingsTab={settingsTab}
        onSettingsTabChange={setSettingsTab}
        modelConfig={modelConfig}
        modelConfigError={modelConfigError}
        modelSaving={modelSaving}
        modelNotice={modelNotice}
        activeModelRoute={activeModelRoute}
        activeModelRouteIndex={activeModelRouteIndex}
        orchestratorRoute={orchestratorRoute}
        orchestratorRouteIndex={orchestratorRouteIndex}
        orchestratorModels={orchestratorModels}
        onUpdateModelDraft={updateModelDraft}
        onSaveModelConfig={() => void saveModelConfig()}
        permissionSettings={permissionSettings}
        permissionAudit={permissionAudit}
        permissionScopes={permissionScopes}
        onUpdatePermission={(name, mode) => void updatePermission(name, mode)}
        mcpServers={mcpServers}
        mcpServersError={mcpServersError}
        mcpServersNotice={mcpServersNotice}
        mcpServersSaving={mcpServersSaving}
        onAddMcpServer={addMcpServer}
        onSaveMcpServers={() => void saveMcpServers()}
        onUpdateMcpServer={updateMcpServer}
        onRemoveMcpServer={removeMcpServer}
        toolCatalog={toolCatalog}
        toolCatalogError={toolCatalogError}
        archivedChats={archivedChats}
        deletingSessionId={deletingSessionId}
        onDeleteSession={(chat) => void deleteSession(chat)}
      />
    );
  }

  function renderPanelContent() {
    if (activePanel === "settings") {
      return settingsPanelElement();
    }

    if (activePanel === "plugins") {
      return <PluginsPanel />;
    }

    if (activePanel === "memory") {
      return <MemoryPanel />;
    }

    if (activePanel === "sites") {
      return (
        <SitesPanel
          siteSearch={siteSearch}
          onSiteSearchChange={setSiteSearch}
        />
      );
    }

    if (activePanel === "files") {
      return (
        <FilesPanel
          rootEntryCount={(directoryCache["."] ?? []).length}
          newFilePath={newFilePath}
          newFileContent={newFileContent}
          onNewFilePathChange={setNewFilePath}
          onNewFileContentChange={setNewFileContent}
          onRefreshTree={() => void refreshDirectory(".")}
          onCreateFile={() => void handleCreateFile()}
          fileTree={renderTree(".")}
        />
      );
    }

    if (activePanel === "editor") {
      return (
        <EditorPanel
          selectedFilePath={selectedFilePath}
          selectedFileContent={selectedFileContent}
          selectedFileOriginal={selectedFileOriginal}
          selectedFileLanguage={selectedFileLanguage}
          selectedFileLoading={selectedFileLoading}
          selectedFileNotice={selectedFileNotice}
          saveState={saveState}
          saveFileRef={saveFileRef}
          onContentChange={(value) => {
            setSelectedFileContent(value);
            setSaveState("idle");
          }}
          onReload={() => void refreshSelectedFile(selectedFilePath ?? ".")}
          onSave={() => void handleSave()}
        />
      );
    }

    if (activePanel === "git") {
      return (
        <GitPanel
          branchLabel={gitSummary.branch}
          changedCount={gitSummary.changed}
          gitDiffPath={gitDiffPath}
          gitDiffPathInput={gitDiffPathInput}
          gitCommitMessage={gitCommitMessage}
          gitRemote={gitRemote}
          gitBranch={gitBranch}
          gitAction={gitAction}
          gitActionNotice={gitActionNotice}
          gitStatus={gitStatus}
          gitDiff={gitDiff}
          selectedFilePath={selectedFilePath}
          onDiffPathInputChange={setGitDiffPathInput}
          onCommitMessageChange={setGitCommitMessage}
          onRemoteChange={setGitRemote}
          onBranchChange={setGitBranch}
          onRefresh={(path) => void refreshGitSnapshot(path)}
          onUseSelectedFile={() => {
            const nextPath = selectedFilePath ?? "";
            setGitDiffPathInput(nextPath);
            void refreshGitSnapshot(nextPath || undefined);
          }}
          onGitAction={(action) => void handleGitAction(action)}
        />
      );
    }

    if (activePanel === "terminal") return <TerminalPanel entries={terminalEntries} />;

    if (activePanel === "scheduled") {
      return (
        <ScheduledPanel
          onPickPrompt={(prompt) => {
            setInput(prompt);
            setActivePanel("chat");
          }}
        />
      );
    }

    if (activePanel === "tasks") {
      return (
        <TasksPanel
          tasks={tasks}
          chatSessions={chatSessions}
          activeSessionId={activeSessionId}
          onNewChat={() => setActivePanel("chat")}
          onOpenSession={(chat) => {
            setActivePanel("chat");
            void openSession(chat).catch((error) => {
              setSocketState("failed");
              setLines((current) => [...current, { role: "system", text: String(error) }]);
            });
          }}
        />
      );
    }

    if (activePanel === "actions") {
      return <ActionsPanel actions={actions} />;
    }

    if (activePanel === "pull-requests") {
      return (
        <PullRequestsPanel
          githubLogin={githubAuth?.login ?? null}
          pullRequestSearch={pullRequestSearch}
          pullRequestScope={pullRequestScope}
          pullRequestsLoading={pullRequestsLoading}
          pullRequestsError={pullRequestsError}
          visiblePullRequests={visiblePullRequests}
          onSearchChange={setPullRequestSearch}
          onScopeChange={setPullRequestScope}
        />
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
        <div
          ref={chatLogRef}
          onScroll={handleChatScroll}
          className={`chatLog${lines.every((line) => line.role === "system") && !stream ? " empty" : ""}`}
        >
          {lines.every((line) => line.role === "system") && !stream ? (
            <div className="chatWelcome">
              <p className="eyebrow">Новая задача</p>
              <h3>Что будем делать?</h3>
              <p className="chatWelcomeText">
                Опиши задачу обычным языком — я помогу разобраться в проекте, изменить файлы или проверить результат.
              </p>
              <div className="chatWelcomeHints">
                <button type="button" onClick={() => setInput("Разберись в коде проекта и объясни, с чего начать.")}>
                  Разобраться в коде
                </button>
                <button type="button" onClick={() => setInput("Измени нужный файл в проекте.")}>
                  Изменить файл
                </button>
                <button type="button" onClick={() => setInput("Запусти проверку проекта и покажи результат.")}>
                  Запустить проверку
                </button>
              </div>
            </div>
          ) : (
            visibleChatLines.map((line, index) => (
              <Fragment key={`${line.role}-${index}`}>
                {index === lastAssistantLineIndex && traceLines.length > 0 && (hasConversation || Boolean(activeTaskId)) ? (
                  <ChatTraceSummary traceLines={traceLines} active={Boolean(activeTaskId)} userLogin={githubAuth?.login} />
                ) : null}
                <article className={`line ${line.role}`}>
                  {line.role === "assistant" ? <AgentAvatar size="sm" /> : null}
                  <strong>{translateChatRole(line.role, githubAuth?.login)}</strong>
                  {line.role === "assistant" ? <MarkdownMessage text={line.text} /> : <pre>{line.text}</pre>}
                </article>
              </Fragment>
            ))
          )}
          {lastAssistantLineIndex === -1 && traceLines.length > 0 && (hasConversation || Boolean(activeTaskId)) ? (
            <ChatTraceSummary traceLines={traceLines} active={Boolean(activeTaskId)} userLogin={githubAuth?.login} />
          ) : null}
          {stream && lastAssistantLineIndex === -1 ? (
            <>
              <article className="line assistant streaming">
                <AgentAvatar size="sm" />
                <strong>Ассистент</strong>
                <MarkdownMessage text={stream} />
              </article>
            </>
          ) : null}
        </div>
        {!hasConversation ? (
          <div className="projectContext">
            <button
              type="button"
              className="projectContextButton"
              onClick={() => setProjectPickerOpen((open) => !open)}
              aria-expanded={projectPickerOpen}
            >
              <span className="projectContextMark">▱</span>
              <strong>{selectedProject.label}</strong>
              <span className="projectContextMeta">Локальный</span>
              <span className="projectContextMeta">main</span>
              <span className="projectContextChevron">⌄</span>
            </button>
            {projectPickerOpen ? (
              <div className="projectPicker" role="menu">
                <label className="projectPickerSearch">
                  <span>⌕</span>
                  <input
                    value={projectSearch}
                    onChange={(event) => setProjectSearch(event.target.value)}
                    placeholder="Поиск проектов"
                    autoFocus
                  />
                </label>
                <button
                  type="button"
                  className={selectedProject.path === "." ? "projectOption active" : "projectOption"}
                  onClick={() => {
                    setSelectedProject({ label: "EvoHime", path: "." });
                    setProjectPickerOpen(false);
                    setProjectSearch("");
                  }}
                >
                  <AgentMark size="sm" />
                  <span><strong>EvoHime</strong><small>Текущий workspace</small></span>
                </button>
                {projectFolders.map((folder) => (
                  <button
                    key={folder.path}
                    type="button"
                    className={selectedProject.path === folder.path ? "projectOption active" : "projectOption"}
                    onClick={() => {
                      setSelectedProject({ label: folder.name, path: folder.path });
                      setProjectPickerOpen(false);
                      setProjectSearch("");
                    }}
                  >
                    <span>▱</span>
                    <span><strong>{folder.name}</strong><small>Проект</small></span>
                    </button>
                  ))}
                {projectCreating ? (
                  <div className="projectCreateForm">
                    <input
                      value={newProjectName}
                      onChange={(event) => setNewProjectName(event.target.value)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter") {
                          event.preventDefault();
                          void createProject();
                        }
                      }}
                      placeholder="Название проекта"
                      autoFocus
                    />
                    <div>
                      <button type="button" onClick={() => void createProject()} disabled={!newProjectName.trim()}>
                        Создать
                      </button>
                      <button type="button" onClick={() => { setProjectCreating(false); setProjectCreateError(null); }}>
                        Отмена
                      </button>
                    </div>
                    {projectCreateError ? <small className="projectCreateError">{projectCreateError}</small> : null}
                  </div>
                ) : (
                  <button
                    type="button"
                    className="projectOption projectCreateOption"
                    onClick={() => { setProjectCreating(true); setProjectCreateError(null); }}
                  >
                    <span>＋</span>
                    <span><strong>Новый проект</strong><small>Создать отдельную папку</small></span>
                  </button>
                )}
                <button
                  type="button"
                  className={selectedProject.path === null ? "projectOption active" : "projectOption"}
                  onClick={() => {
                    setSelectedProject({ label: "Без проекта", path: null });
                    setProjectPickerOpen(false);
                    setProjectSearch("");
                  }}
                >
                  <span>×</span>
                  <span><strong>Работать без проекта</strong><small>Без выбранной папки</small></span>
                </button>
              </div>
            ) : null}
          </div>
        ) : null}
        {composerNotice ? <p className="composerNotice">{composerNotice}</p> : null}
        <form onSubmit={sendMessage} className="composer">
          <div className="composerField">
            <div className="composerLeading">
              <input
                ref={attachmentInputRef}
                type="file"
                multiple
                className="attachmentInput"
                onChange={handleAttachmentChange}
                aria-label="Добавить вложения"
              />
              <button
                type="button"
                className="attachmentButton"
                onClick={() => attachmentInputRef.current?.click()}
                aria-label="Добавить вложения"
              >
                +
              </button>
              <div className="composerMenu">
                <button
                  type="button"
                  className={workMode === "allow" ? "workModeSelect workModeAllow" : "workModeSelect"}
                  onClick={() => { setModelPickerOpen(false); setWorkModeOpen((open) => !open); }}
                  disabled={permissionModeSaving || Object.keys(permissionSettings).length === 0}
                  aria-label="Режим работы агента"
                  aria-expanded={workModeOpen}
                >
                  {workMode === "allow" ? "Полный доступ" : workMode === "deny" ? "Запретить всё" : workMode === "mixed" ? "Смешанный режим" : "Спрашивать всё"}
                  <span className="composerMenuChevron">⌄</span>
                </button>
                {workModeOpen ? (
                  <div className="composerMenuPopover workModePopover" role="listbox">
                    {[
                      ["ask", "Спрашивать всё"],
                      ["allow", "Полный доступ"],
                      ["deny", "Запретить всё"],
                    ].map(([value, label]) => (
                      <button
                        type="button"
                        role="option"
                        aria-selected={workMode === value}
                        key={value}
                        onClick={() => { void updateWorkMode(value as PermissionMode); setWorkModeOpen(false); }}
                      >
                        {label}
                      </button>
                    ))}
                  </div>
                ) : null}
              </div>
            </div>
            <textarea
              rows={1}
              value={input}
              onChange={(event) => setInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
                  event.preventDefault();
                  event.currentTarget.form?.requestSubmit();
                }
              }}
              placeholder="Введите сообщение..."
            />
            <div className="composerControls">
              <div className="composerMenu composerModelMenu">
                <button
                  type="button"
                  className="composerModelSelect"
                  onClick={() => { setWorkModeOpen(false); setModelPickerOpen((open) => !open); }}
                  disabled={composerModelsLoading || composerModels.length === 0}
                  aria-label="Модель агента"
                  aria-expanded={modelPickerOpen}
                  title={composerModelsError ?? "Модель агента"}
                >
                  <span>{selectedComposerModel || "Модели недоступны"}</span>
                  <span className="composerMenuChevron">⌄</span>
                </button>
                {modelPickerOpen ? (
                  <div className="composerMenuPopover modelPopover" role="listbox">
                    {composerModels.map((model) => (
                      <button
                        type="button"
                        role="option"
                        aria-selected={selectedComposerModel === model}
                        key={model}
                        onClick={() => { setSelectedComposerModel(model); setModelPickerOpen(false); }}
                      >
                        {model}
                      </button>
                    ))}
                  </div>
                ) : null}
              </div>
              <button
                type={activeTaskId ? "button" : "submit"}
                className={activeTaskId ? "sendButton stopButton" : "sendButton"}
                onClick={activeTaskId ? stopCurrentTask : undefined}
                disabled={socketState !== "connected"}
                aria-label={activeTaskId ? "Остановить ответ" : "Отправить сообщение"}
              >
                <span aria-hidden="true">{activeTaskId ? "■" : "↑"}</span>
              </button>
            </div>
            {attachments.length > 0 ? (
              <div className="attachmentList" aria-label="Выбранные вложения">
                {attachments.map((file) => <span key={`${file.name}-${file.size}`}>{file.name}</span>)}
              </div>
            ) : null}
          </div>
        </form>
      </>
    );
  }

  return (
    <main className="shell">
      <header className="topBar">
        <AgentBrand />
        <button
          type="button"
          className={traceOpen ? "traceToggle active" : "traceToggle"}
          onClick={() => setTraceOpen((open) => !open)}
          aria-expanded={traceOpen}
          aria-controls="task-trace"
        >
          <span aria-hidden="true">⌁</span>
          Трейс
        </button>
        <div className="statusCard">
          <span className="statusDot" data-state={socketState} />
          <div>
            <strong>{connectedLabel}</strong>
            <span>{session ? session.session_id : "сессия ещё не создана"}</span>
          </div>
        </div>
      </header>

      <section className={traceOpen ? "workspace traceOpen" : "workspace"}>
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
                  onClick={() => {
                    if (item.id === "new-task") {
                      void createNewChat().catch((error) => {
                        setLines((current) => [...current, { role: "system", text: String(error) }]);
                      });
                      return;
                    }
                    setActivePanel(item.panel);
                  }}
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
            {projectChatSessions.length > 0 ? (
              <div className="projectChatList">
                {projectChatSessions.map((chat, index) => (
                  <div className="projectChatRow" key={chat.session_id}>
                    <button
                      type="button"
                      className={chat.session_id === activeSessionId ? "projectChatItem active" : "projectChatItem"}
                      onClick={() => {
                        setActivePanel("chat");
                        void openSession(chat).catch((error) => setLines((current) => [...current, { role: "system", text: String(error) }]));
                      }}
                    >
                      <strong>{formatSessionTitle(chat, index)}</strong>
                    </button>
                    <button
                      type="button"
                      className="chatArchiveButton"
                      onClick={() => void archiveChat(chat)}
                      aria-label="Архивировать чат"
                      title="Архивировать чат"
                    >
                      ▱
                    </button>
                  </div>
                ))}
              </div>
            ) : null}
          </section>

          <section className="sidebarSection">
            <header className="sidebarHeader">
              <strong>Чаты без проекта</strong>
            </header>
            {standaloneChatSessions.length > 0 ? (
              <div className="standaloneSidebarChatList">
                {standaloneChatSessions.slice(0, 5).map((chat, index) => (
                  <div className="standaloneSidebarChatRow" key={chat.session_id}>
                    <button
                      type="button"
                      className={chat.session_id === activeSessionId ? "standaloneSidebarChat active" : "standaloneSidebarChat"}
                      onClick={() => {
                        setActivePanel("chat");
                        void openSession(chat).catch((error) => setLines((current) => [...current, { role: "system", text: String(error) }]));
                      }}
                    >
                      <strong>{formatSessionTitle(chat, index)}</strong>
                      <span>{formatSessionPreview(chat)}</span>
                    </button>
                    <button
                      type="button"
                      className="chatArchiveButton"
                      onClick={() => void archiveChat(chat)}
                      aria-label="Архивировать чат"
                      title="Архивировать чат"
                    >
                      ▱
                    </button>
                  </div>
                ))}
              </div>
            ) : (
              <button type="button" className="taskSummaryCard" onClick={() => void createNewChat()}>
                <strong>Нет чатов</strong>
                <span>Создай первый чат без проекта</span>
              </button>
            )}
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
          {activePanel !== "pull-requests" && activePanel !== "plugins" && activePanel !== "sites" && activePanel !== "memory" ? (
            <header>
              <h2>{activePanel === "chat" ? activeChatTitle : currentPanelLabel}</h2>
              <div className="panelHeaderActions">
                {activePanel === "chat" ? (
                  <button type="button" className="panelHeaderButton" onClick={() => void copyChat()}>
                    Копировать чат
                  </button>
                ) : null}
              </div>
            </header>
          ) : null}
          {renderPanelContent()}
        </div>

        {traceOpen ? (
          <aside className="traceSidebar" id="task-trace" aria-label="Трейс задачи">
            <header className="traceHeader">
              <div>
                <strong>Трейс задачи</strong>
                <span>{activeTaskId ? "Текущая задача" : "События чата"}</span>
              </div>
              <div className="traceHeaderActions">
                <button type="button" className="panelHeaderButton" onClick={exportTrace}>
                  Экспорт
                </button>
                <button type="button" className="traceClose" onClick={() => setTraceOpen(false)} aria-label="Закрыть трейс">
                  ×
                </button>
              </div>
            </header>
            <div className="traceList">
              {lines.filter((line) => line.role === "system" || line.role === "tool").length > 0 ? (
                lines.filter((line) => line.role === "system" || line.role === "tool").map((line, index) => (
                  <article className={`traceItem ${line.role}`} key={`${line.role}-${index}`}>
                    <strong>{translateChatRole(line.role, githubAuth?.login)}</strong>
                    <pre>{line.text}</pre>
                  </article>
                ))
              ) : (
                <div className="traceEmpty">Здесь появятся план, статусы и действия агента.</div>
              )}
            </div>
          </aside>
        ) : null}
        {chatActionNotice ? <div className="chatActionNotice" role="status">{chatActionNotice}</div> : null}

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
                <AgentBrand title="Параметры EvoHime" as="h2" markSize="sm" />
              </div>
              <button type="button" className="settingsCloseButton" onClick={() => setSettingsOpen(false)}>
                Закрыть
              </button>
            </header>
            <div className="settingsModalBody">{settingsPanelElement()}</div>
          </section>
        </div>
      ) : null}
      {approval ? <ApprovalModal request={approval} onGrant={() => resolveApproval("approval.granted")} onDeny={() => resolveApproval("approval.denied")} /> : null}
      {memoryAsk ? (
        <MemoryAskModal
          request={memoryAsk}
          onAccept={() => resolveMemoryAsk("memory.accept")}
          onReject={() => resolveMemoryAsk("memory.reject")}
        />
      ) : null}
    </main>
  );
}

function MarkdownMessage({ text }: { text: string }) {
  return (
    <div className="markdownBody">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ node: _node, ...props }) => <a {...props} target="_blank" rel="noreferrer" />,
          code: ({ node: _node, className, children, ...props }) => {
            const inline = !className;
            return inline ? (
              <code className="inlineCode" {...props}>{children}</code>
            ) : (
              <pre className="codeBlock"><code className={className} {...props}>{children}</code></pre>
            );
          },
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}

function ChatTraceSummary({ traceLines, active, userLogin }: { traceLines: ChatLine[]; active: boolean; userLogin?: string | null }) {
  return (
    <details className="chatTraceSummary" open={active}>
      <summary>
        <span className="chatTraceSummaryTitle">
          {active ? <AgentAvatar size="sm" className="agentAvatarThinking" /> : <span className="thinkingOrb" aria-hidden="true" />}
          {active ? "Модель работает…" : "Ход работы"}
        </span>
        <span className="chatTraceSummaryMeta">{active ? "Выполняю план" : "Завершено"}</span>
      </summary>
      <div className="chatTraceSummaryBody">
        {traceLines.map((line, index) => (
          <article className={`chatTraceEntry ${line.role}`} key={`${line.role}-${index}`}>
            <strong>{translateChatRole(line.role, userLogin)}</strong>
            <pre>{line.text}</pre>
          </article>
        ))}
      </div>
    </details>
  );
}
