import Editor from "@monaco-editor/react";
import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import type {
  ClientCommand,
  ServerEvent,
  SessionBootstrap,
} from "./protocol";
import type { TaskStatusChangedEvent, TaskStepChangedEvent, ActionLoggedEvent } from "./protocol";

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

type TaskView = { id: string; message: string; status: string; steps: Record<string, string> };
type ActionView = { taskId: string; action: string; detail: string; createdAt: string };

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
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved">("idle");
  const [gitStatus, setGitStatus] = useState("Loading git status...");
  const [gitDiff, setGitDiff] = useState("Loading git diff...");
  const [gitDiffPath, setGitDiffPath] = useState<string | null>(null);
  const [tasks, setTasks] = useState<Record<string, TaskView>>({});
  const [actions, setActions] = useState<ActionView[]>([]);
  const socketRef = useRef<WebSocket | null>(null);

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
      applyEvent(parsed);
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
        setLines((current) => [
          ...current,
          {
            role: "tool",
            text: `Result from ${event.tool_name}:\n${event.output}`,
          },
        ]);
        break;
      case "tool.completed":
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
            text: `Agent plan: ${event.plan.join(" -> ")}`,
          },
        ]);
        break;
      case "file.changed":
        void refreshDirectory(".").catch(() => undefined);
        void refreshDirectory(parentPath(event.path)).catch(() => undefined);
        if (normalizePath(selectedFilePath ?? undefined) === normalizePath(event.path)) {
          void refreshSelectedFile(event.path).catch(() => undefined);
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
      [key]: data.entries,
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
    const diffPath = normalizePath(path ?? gitDiffPath ?? selectedFilePath ?? undefined);
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
  }

  async function openFile(path: string) {
    try {
      setActivePanel("editor");
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

  function renderTree(path = ".", depth = 0) {
    const normalized = normalizePath(path);
    const entries = directoryCache[normalized] ?? [];
    const isExpanded = expandedDirectories[normalized] ?? normalized === ".";

    return (
      <div className="treeBranch" data-depth={depth}>
        {entries.map((entry) => {
          const key = entry.path;
          const expanded = expandedDirectories[key] ?? false;

          return (
            <div key={key} className="treeNode" data-kind={entry.kind} data-depth={depth}>
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
        </div>
      );
    }

    if (activePanel === "files") {
      return (
        <div className="filesPanel">
          <div className="panelToolbar">
            <button type="button" onClick={() => refreshDirectory(".")}>Refresh tree</button>
            <span>{directoryCache["."]?.length ?? 0} items</span>
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
            <button
              type="button"
              onClick={() => void handleSave()}
              disabled={!selectedFilePath || selectedFileContent === selectedFileOriginal}
            >
              Save
            </button>
          </div>
          {selectedFilePath ? (
            <Editor
              height="100%"
              theme="vs-dark"
              language="typescript"
              value={selectedFileContent}
              onChange={(value) => {
                setSelectedFileContent(value ?? "");
                setSaveState("idle");
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
            <button type="button" onClick={() => void refreshGitSnapshot()}>Refresh git</button>
            <span>{gitDiffPath ? `Diff: ${gitDiffPath}` : "Repository diff"}</span>
          </div>
          <div className="gitSummary">
            <h3>Status</h3>
            <pre>{gitStatus}</pre>
          </div>
          <div className="gitSummary">
            <h3>Diff</h3>
            <pre>{gitDiff || "No diff"}</pre>
          </div>
        </div>
      );
    }

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
    </main>
  );
}
