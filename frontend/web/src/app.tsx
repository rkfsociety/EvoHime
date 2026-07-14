import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import type {
  ClientCommand,
  ServerEvent,
  SessionBootstrap,
} from "./protocol";

type ChatLine = {
  role: "assistant" | "tool" | "system" | "user";
  text: string;
};

const initialLines: ChatLine[] = [
  {
    role: "system",
    text: "EvoHime is ready. Send a message to create a task and watch the event stream.",
  },
];

export function App() {
  const [session, setSession] = useState<SessionBootstrap | null>(null);
  const [socketState, setSocketState] = useState<
    "idle" | "connecting" | "connected" | "failed"
  >("idle");
  const [input, setInput] = useState("");
  const [lines, setLines] = useState<ChatLine[]>(initialLines);
  const [events, setEvents] = useState<ServerEvent[]>([]);
  const [stream, setStream] = useState("");
  const socketRef = useRef<WebSocket | null>(null);

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
        setEvents(data.events);
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
        setLines((current) => [
          ...current,
          { role: "assistant", text: event.final_message },
        ]);
        setStream("");
        break;
      case "task.failed":
        setLines((current) => [
          ...current,
          { role: "system", text: `Task failed: ${event.error}` },
        ]);
        setStream("");
        break;
      case "agent.plan.updated":
        setLines((current) => [
          ...current,
          {
            role: "system",
            text: `Agent plan: ${event.plan.join(" -> ")}`,
          },
        ]);
        break;
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

  return (
    <main className="shell">
      <section className="hero">
        <div>
          <p className="eyebrow">EvoHime</p>
          <h1>Web-first AI-agent workspace</h1>
          <p className="lede">
            Minimal vertical slice: message, task, streamed response,
            `filesystem.read`, history in PostgreSQL.
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
        <div className="panel chatPanel">
          <header>
            <h2>Chat</h2>
            <span>WebSocket</span>
          </header>
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

