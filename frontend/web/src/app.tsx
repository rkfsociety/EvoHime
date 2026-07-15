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
  | "editor"
  | "terminal"
  | "git"
  | "tasks"
  | "actions"
  | "settings";

type ModelConfig = {
  provider: string;
  model: string;
  base_url: string;
  configured: boolean;
  available_models: string[];
};

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

const initialLines: ChatLine[] = [
  {
    role: "system",
    text: "EvoHime is ready. Send a message to create a task and watch the event stream.",
  },
];

const workspacePanels: Array<{ id: WorkspacePanel; label: string; phase: string }> = [
  { id: "chat", label: "Chat", phase: "active" },
  { id: "files", label: "Files", phase: "stage 4" },
  { id: "editor", label: "Editor", phase: "stage 4" },
  { id: "terminal", label: "Terminal", phase: "stage 3" },
  { id: "git", label: "Git", phase: "stage 4" },
  { id: "tasks", label: "Tasks", phase: "stage 5" },
  { id: "actions", label: "Actions", phase: "stage 5" },
  { id: "settings", label: "Settings", phase: "stage 2" },
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
  const branch = lines[0] ?? "No status";
  const changed = lines.filter((line) => !line.startsWith("##")).length;
  return {
    branch,
    changed,
    lines,
  };
}

export function App() {
  const [session, setSession] = useState<SessionBootstrap | null>(null);
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
  const [gitStatus, setGitStatus] = useState("Loading git status...");
  const [gitDiff, setGitDiff] = useState("Loading git diff...");
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
  const [terminalEntries, setTerminalEntries] = useState<TerminalEntry[]>([]);
  const [permissionSettings, setPermissionSettings] = useState<PermissionSettings>({});
  const socketRef = useRef<WebSocket | null>(null);
  const applyEventRef = useRef<(event: ServerEvent) => void>(() => undefined);
  const saveFileRef = useRef<() => void>(() => undefined);

  useEffect(() => {
    let cancelled = false;

    const loadModelConfig = async () => {
      const response = await fetch("/api/models/config");
      if (!response.ok) {
        throw new Error("Failed to load model config");
      }
      const data = (await response.json()) as ModelConfig;
      if (!cancelled) {
        setModelConfig(data);
      }
    };
    fetch("/api/permissions").then((response) => response.json()).then((data: PermissionSettings) => setPermissionSettings(data)).catch(() => undefined);

    loadModelConfig().catch((error) => {
      if (!cancelled) {
        setModelConfigError(String(error));
      }
    });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;

    const bootstrap = async () => {
      const response = await fetch("/api/sessions", { method: "POST" });
      if (!response.ok) {
        throw new Error("Failed to create session");
      }
      const data = (await response.json()) as SessionBootstrap;
      if (!cancelled) {
        setSession(data);
        setEvents(data.events.map((item) => item.event));
        for (const item of data.events) {
          applyEvent(item.event);
        }
      }
    };

    bootstrap().catch((error) => {
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
    if (!session) {
      return;
    }

    void refreshDirectory(".").catch(() => undefined);
    void refreshGitSnapshot().catch(() => undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session]);

  const connectedLabel = useMemo(() => {
    if (!session) {
      return "Creating session...";
    }
    if (socketState === "connected") {
      return "Connected";
    }
    if (socketState === "failed") {
      return "Connection failed";
    }
    return "Connecting...";
  }, [session, socketState]);

  const currentPanelLabel = useMemo(
    () => workspacePanels.find((panel) => panel.id === activePanel)?.label ?? "Workspace",
    [activePanel],
  );
  const selectedFileLanguage = useMemo(
    () => inferMonacoLanguage(selectedFilePath),
    [selectedFilePath],
  );
  const gitSummary = useMemo(() => summarizeGitStatus(gitStatus), [gitStatus]);
  applyEventRef.current = applyEvent;
  saveFileRef.current = () => {
    void handleSave();
  };

  function applyEvent(event: ServerEvent) {
    switch (event.type) {
      case "session.created":
        setLines((current) => [
          ...current,
          {
            role: "system",
            text: `Session created: ${event.session_id}`,
          },
        ]);
        break;
      case "task.started":
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
            text: `Tool started: ${event.tool_name}`,
          },
        ]);
        break;
      case "tool.output":
        if (event.tool_name === "shell.execute") setTerminalEntries((current) => [...current, { stream: "stdout", text: event.output }]);
        setLines((current) => [
          ...current,
          {
            role: "tool",
            text: `Result from ${event.tool_name}:\n${event.output}`,
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
              text: event.success ? "shell.execute completed" : "shell.execute failed",
            },
          ]);
        }
        setLines((current) => [
          ...current,
          {
            role: "tool",
            text: `Tool ${event.tool_name} completed`,
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
          { role: "system", text: `Task failed: ${event.error}` },
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
      throw new Error("Failed to load file tree");
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
        throw new Error("Failed to load file content");
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
      throw new Error("Failed to load git status");
    }
    const statusData = (await statusResponse.json()) as GitSnapshot;
    const diffResponse = await fetch(
      diffPath === "." ? "/api/git/diff" : `/api/git/diff?path=${encodeURIComponent(diffPath)}`,
    );
    if (!diffResponse.ok) {
      throw new Error("Failed to load git diff");
    }
    const diffData = (await diffResponse.json()) as GitSnapshot;
    setGitStatus(statusData.status);
    setGitDiff(diffData.diff);
    setGitDiffPath(diffPath === "." ? null : diffPath);
    setGitDiffPathInput(diffPath === "." ? "" : diffPath);
  }

  async function updatePermission(name: string, mode: PermissionMode) {
    const response = await fetch(`/api/permissions/${name}`, { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ mode }) });
    if (!response.ok) throw new Error("Failed to update permission");
    setPermissionSettings((current) => ({ ...current, [name]: { mode } }));
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
        throw new Error("Failed to save file");
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
        throw new Error(detail || `Git ${action} failed`);
      }
      setGitActionNotice(`Git ${action} завершён.`);
      if (action === "commit") {
        setGitCommitMessage("");
      }
      await refreshGitSnapshot(gitDiffPath);
      await refreshDirectory(".");
    } catch (error) {
      setGitActionNotice(`Git ${action}: ${String(error)}`);
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

  function renderPanelContent() {
    if (activePanel === "settings") {
      return (
        <div className="settingsPanel">
          <h3>Model provider</h3>
          {modelConfigError ? (
            <p className="settingsError">{modelConfigError}</p>
          ) : null}
          {modelConfig ? (
            <dl className="settingsGrid">
              <div>
                <dt>Provider</dt>
                <dd>{modelConfig.provider}</dd>
              </div>
              <div>
                <dt>Model</dt>
                <dd>{modelConfig.model}</dd>
              </div>
              <div>
                <dt>Base URL</dt>
                <dd>{modelConfig.base_url}</dd>
              </div>
              <div>
                <dt>API key</dt>
                <dd data-configured={modelConfig.configured}>
                  {modelConfig.configured ? "configured" : "missing - set LITEROUTER_API_KEY"}
                </dd>
              </div>
              <div>
                <dt>Available models</dt>
                <dd>{modelConfig.available_models.join(", ")}</dd>
              </div>
            </dl>
          ) : (
            <p>Loading model configuration...</p>
          )}
          <p className="settingsHint">
            LiteRouter is the first provider. Configure via environment variables on the server.
          </p>
          <h3>Tool permissions</h3>
          <div className="permissionList">{Object.entries(permissionSettings).map(([name, value]) => <label key={name}><span>{name}</span><select value={value.mode} onChange={(event) => void updatePermission(name, event.target.value as PermissionMode)}><option value="ask">ask</option><option value="allow">allow</option><option value="deny">deny</option></select></label>)}</div>
        </div>
      );
    }

    if (activePanel === "files") {
      const rootEntries = directoryCache["."] ?? [];
      return (
        <div className="filesPanel">
          <div className="panelToolbar">
            <div>
              <strong>Workspace tree</strong>
              <span>{rootEntries.length} items at root</span>
            </div>
            <button type="button" onClick={() => void refreshDirectory(".")}>Refresh tree</button>
          </div>
          <div className="createFileForm">
            <input
              value={newFilePath}
              onChange={(event) => setNewFilePath(event.target.value)}
              placeholder="path/to/new-file.ts"
              aria-label="New file path"
            />
            <input
              value={newFileContent}
              onChange={(event) => setNewFileContent(event.target.value)}
              placeholder="Initial content"
              aria-label="New file content"
            />
            <button type="button" onClick={() => void handleCreateFile()} disabled={!newFilePath.trim()}>
              Create file
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
              <strong>{selectedFilePath ?? "No file selected"}</strong>
              <span>
                {selectedFileLoading
                  ? "Loading..."
                  : saveState === "saving"
                    ? "Saving..."
                    : saveState === "saved"
                      ? "Saved"
                      : selectedFilePath && selectedFileContent !== selectedFileOriginal
                        ? "Unsaved changes"
                        : "Ready"}
              </span>
            </div>
            <div className="toolbarActions">
              <button
                type="button"
                onClick={() => void refreshSelectedFile(selectedFilePath ?? ".")}
                disabled={!selectedFilePath || selectedFileLoading}
              >
                Reload
              </button>
              <button
                type="button"
                onClick={() => void handleSave()}
                disabled={!selectedFilePath || selectedFileContent === selectedFileOriginal}
              >
                Save
              </button>
            </div>
          </div>
          {selectedFileNotice ? <div className="editorNotice">{selectedFileNotice}</div> : null}
          {selectedFilePath ? (
            <div className="editorMeta">
              <span>Language: {selectedFileLanguage}</span>
              <span>Size: {formatFileSize(selectedFileContent.length)}</span>
              <span>{selectedFileContent === selectedFileOriginal ? "Clean" : "Dirty"}</span>
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
                  label: "Save file",
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
              <h3>Select a file</h3>
              <p>Choose a file from Files to open it in Monaco Editor.</p>
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
              <strong>Repository status</strong>
              <span>
                {gitSummary.branch}
                {gitSummary.changed ? ` • ${gitSummary.changed} changed` : " • clean"}
              </span>
            </div>
            <div className="toolbarActions">
              <button type="button" onClick={() => void refreshGitSnapshot(gitDiffPathInput || undefined)}>
                Refresh git
              </button>
            </div>
          </div>
          <div className="gitControls">
            <label>
              <span>Diff path</span>
              <input
                value={gitDiffPathInput}
                onChange={(event) => setGitDiffPathInput(event.target.value)}
                placeholder="Repository root or file path"
              />
            </label>
            <div className="gitControlButtons">
              <button type="button" onClick={() => void refreshGitSnapshot(gitDiffPathInput || undefined)}>
                Load diff
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
                Use selected file
              </button>
            </div>
            <label>
              <span>Commit message</span>
              <input
                value={gitCommitMessage}
                onChange={(event) => setGitCommitMessage(event.target.value)}
                placeholder="Describe the change"
              />
            </label>
            <div className="gitRemoteFields">
              <label>
                <span>Remote</span>
                <input value={gitRemote} onChange={(event) => setGitRemote(event.target.value)} />
              </label>
              <label>
                <span>Branch</span>
                <input value={gitBranch} onChange={(event) => setGitBranch(event.target.value)} placeholder="Current" />
              </label>
            </div>
            <div className="gitControlButtons">
              <button type="button" onClick={() => void handleGitAction("commit")} disabled={!gitCommitMessage.trim() || Boolean(gitAction)}>
                {gitAction === "commit" ? "Committing..." : "Commit"}
              </button>
              <button type="button" onClick={() => void handleGitAction("pull")} disabled={Boolean(gitAction)}>
                {gitAction === "pull" ? "Pulling..." : "Pull"}
              </button>
              <button type="button" onClick={() => void handleGitAction("push")} disabled={Boolean(gitAction)}>
                {gitAction === "push" ? "Pushing..." : "Push"}
              </button>
            </div>
            {gitActionNotice ? <p className="gitActionNotice">{gitActionNotice}</p> : null}
          </div>
          <div className="gitSummary">
            <h3>Status</h3>
            <pre>{gitStatus}</pre>
          </div>
          <div className="gitSummary">
            <h3>Diff{gitDiffPath ? ` · ${gitDiffPath}` : ""}</h3>
            <pre className="gitDiffViewer">
              {(gitDiff || "No diff").split("\n").map((line, index) => (
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
        <strong>{task.status}</strong><p>{task.message}</p><small>{task.id}</small>
        <div className="taskActions">
          {(task.status === "running" || task.status === "cancelling") && <button type="button" onClick={() => sendTaskCommand("task.cancel", task.id)}>Cancel</button>}
          {(task.status === "paused" || task.status === "cancelled") && <button type="button" onClick={() => sendTaskCommand("task.resume", task.id)}>Resume</button>}
          {task.status === "failed" && <button type="button" onClick={() => sendTaskCommand("task.retry", task.id)}>Retry</button>}
        </div>
        <ul>{Object.entries(task.steps).map(([name, status]) => <li key={name}>{name}: {status}</li>)}</ul>
      </article>)}</div>;
    }

    if (activePanel === "actions") {
      return <div className="actionsPanel">{actions.map((action, index) => <article className="actionItem" key={`${action.taskId}-${index}`}><strong>{action.action}</strong><span>{action.detail}</span><small>{action.createdAt}</small></article>)}</div>;
    }

    if (activePanel !== "chat") {
      const panel = workspacePanels.find((item) => item.id === activePanel);
      return (
        <div className="placeholderPanel">
          <h3>{panel?.label}</h3>
          <p>Planned for {panel?.phase}.</p>
        </div>
      );
    }

    return (
      <>
        <div className="chatLog">
          {lines.map((line, index) => (
            <article className={`line ${line.role}`} key={`${line.role}-${index}`}>
              <strong>{line.role}</strong>
              <pre>{line.text}</pre>
            </article>
          ))}
          {stream ? (
            <article className="line assistant streaming">
              <strong>assistant</strong>
              <pre>{stream}</pre>
            </article>
          ) : null}
        </div>
        <form onSubmit={sendMessage} className="composer">
          <input
            value={input}
            onChange={(event) => setInput(event.target.value)}
            placeholder="Type a message..."
          />
          <button type="submit" disabled={socketState !== "connected"}>
            Send
          </button>
        </form>
      </>
    );
  }

  return (
    <main className="shell">
      <section className="hero">
        <div>
          <p className="eyebrow">EvoHime</p>
          <h1>Web-first AI-agent workspace</h1>
          <p className="lede">
            Stage 4: files, Monaco editor, and Git diff flow through the browser and websocket bus.
          </p>
        </div>
        <div className="statusCard">
          <span className="statusDot" data-state={socketState} />
          <div>
            <strong>{connectedLabel}</strong>
            <span>{session ? session.session_id : "no session yet"}</span>
          </div>
        </div>
      </section>

      <section className="workspace">
        <nav className="sidebar">
          {workspacePanels.map((panel) => (
            <button
              key={panel.id}
              type="button"
              className={panel.id === activePanel ? "navItem active" : "navItem"}
              onClick={() => setActivePanel(panel.id)}
            >
              <span>{panel.label}</span>
              <small>{panel.phase}</small>
            </button>
          ))}
        </nav>

        <div className="panel mainPanel">
          <header>
            <h2>{currentPanelLabel}</h2>
            <span>WebSocket</span>
          </header>
          {renderPanelContent()}
        </div>

        <div className="panel timelinePanel">
          <header>
            <h2>Events</h2>
            <span>{events.length}</span>
          </header>
          <div className="eventList">
            {events.map((event, index) => (
              <article key={`${event.type}-${index}`} className="eventItem">
                <strong>{event.type}</strong>
                <code>{JSON.stringify(event, null, 2)}</code>
              </article>
            ))}
          </div>
        </div>
      </section>
      {approval ? <ApprovalModal request={approval} onGrant={() => resolveApproval("approval.granted")} onDeny={() => resolveApproval("approval.denied")} /> : null}
    </main>
  );
}

function formatPlan(plan: PlanStep[]) {
  if (plan.length === 0) {
    return "Agent plan: empty";
  }

  return [
    "Agent plan:",
    ...plan.map((step) => {
      const dependencies = step.depends_on ?? [];
      const deps = dependencies.length > 0 ? ` depends on ${dependencies.join(", ")}` : "";
      return `- ${step.id}: ${step.tool_name} — ${step.description}${deps}`;
    }),
  ].join("\n");
}
