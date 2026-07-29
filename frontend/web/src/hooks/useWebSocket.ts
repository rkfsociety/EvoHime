import { useEffect, useRef, useState } from "react";
import type { HistoryItem, ServerEvent } from "../protocol";
import { websocketUrl } from "../api/client";

type SocketState = "idle" | "connecting" | "reconnecting" | "connected" | "failed";

type UseWebSocketOptions = {
  sessionId: string | null;
  onEvent: (event: ServerEvent) => void;
  onReconnect?: (state: "started" | "succeeded" | "failed") => void;
};

interface ReplayContext {
  sessionId: string;
  lastSequence: number;
  replayCursor?: string;
  replayLastRecvTime?: string;
}

const MAX_RETRY_ATTEMPTS = 5;
const BACKOFF_BASE = 500;
const BACKOFF_JITTER = 1000;
const MAX_BACKOFF = 32000;
const SESSION_STORAGE_KEY = "evohime_replay_context";
const LOCAL_STORAGE_KEY = "evohime_replay_cursor";
const RECONNECT_CONTEXT_TIMEOUT = 30 * 60 * 1000; // 30 minutes

function getReconnectDelay(attempt: number): number {
  const exponential = BACKOFF_BASE * Math.pow(2, Math.min(attempt, 5));
  const jitter = Math.random() * BACKOFF_JITTER;
  return Math.min(MAX_BACKOFF, exponential + jitter);
}

function saveReplayContext(sessionId: string, lastSequence: number, cursor?: string) {
  const context: ReplayContext = {
    sessionId,
    lastSequence,
    replayCursor: cursor,
    replayLastRecvTime: new Date().toISOString(),
  };
  sessionStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(context));
  if (cursor) {
    localStorage.setItem(`${LOCAL_STORAGE_KEY}:${sessionId}`, JSON.stringify({ cursor, lastSequence }));
  }
}

function loadReplayContext(sessionId: string): ReplayContext | null {
  const stored = sessionStorage.getItem(SESSION_STORAGE_KEY);
  if (!stored) return null;

  const context: ReplayContext = JSON.parse(stored);
  if (context.sessionId !== sessionId) return null;

  // Check if context is still fresh (< 30 minutes)
  if (context.replayLastRecvTime) {
    const age = Date.now() - new Date(context.replayLastRecvTime).getTime();
    if (age > RECONNECT_CONTEXT_TIMEOUT) {
      sessionStorage.removeItem(SESSION_STORAGE_KEY);
      return null;
    }
  }

  return context;
}

function clearReplayContext() {
  sessionStorage.removeItem(SESSION_STORAGE_KEY);
}

export function useWebSocket({ sessionId, onEvent, onReconnect }: UseWebSocketOptions) {
  const [socketState, setSocketState] = useState<SocketState>("idle");
  const socketRef = useRef<WebSocket | null>(null);
  const lastSequenceRef = useRef(0);
  const lastCursorRef = useRef<string | undefined>();
  // onEvent/onReconnect are frequently recreated on every render by the
  // caller (e.g. an inline onReconnect callback) — reading them through a
  // ref keeps the effect below keyed only on sessionId, otherwise every
  // parent re-render tears down and reopens the socket before it ever
  // finishes connecting.
  const onEventRef = useRef(onEvent);
  onEventRef.current = onEvent;
  const onReconnectRef = useRef(onReconnect);
  onReconnectRef.current = onReconnect;

  useEffect(() => {
    if (!sessionId) {
      return;
    }

    let cancelled = false;
    let socket: WebSocket | null = null;
    let reconnectTimer: number | undefined;
    let attempt = 0;

    // Load previous replay context if available
    const replayContext = loadReplayContext(sessionId);
    if (replayContext) {
      lastSequenceRef.current = replayContext.lastSequence;
      lastCursorRef.current = replayContext.replayCursor;
    }

    const connect = () => {
      if (cancelled) return;

      const isInitial = attempt === 0;
      setSocketState(isInitial ? "connecting" : "reconnecting");

      if (!isInitial) {
        onReconnectRef.current?.("started");
      }

      const after = lastSequenceRef.current;
      const path = after > 0 ? `/ws/${sessionId}?after_sequence=${after}` : `/ws/${sessionId}`;
      socket = new WebSocket(websocketUrl(path));
      socketRef.current = socket;

      socket.onopen = () => {
        if (cancelled) return;
        attempt = 0;
        setSocketState("connected");
        if (!isInitial) {
          onReconnectRef.current?.("succeeded");
        }
        saveReplayContext(sessionId, lastSequenceRef.current, lastCursorRef.current);
      };

      socket.onclose = () => {
        if (cancelled) return;

        // Check if max retries exceeded
        if (attempt >= MAX_RETRY_ATTEMPTS) {
          setSocketState("failed");
          onReconnectRef.current?.("failed");
          return;
        }

        setSocketState("reconnecting");
        const delay = getReconnectDelay(attempt);
        attempt += 1;
        saveReplayContext(sessionId, lastSequenceRef.current, lastCursorRef.current);
        reconnectTimer = window.setTimeout(connect, delay);
      };

      socket.onerror = () => undefined;

      socket.onmessage = (messageEvent) => {
        const raw = JSON.parse(messageEvent.data as string) as HistoryItem | ServerEvent;
        if (
          raw &&
          typeof raw === "object" &&
          "sequence" in raw &&
          "event" in raw &&
          typeof (raw as HistoryItem).sequence === "number"
        ) {
          const item = raw as HistoryItem;
          if (item.sequence <= lastSequenceRef.current) return;
          lastSequenceRef.current = item.sequence;
          saveReplayContext(sessionId, lastSequenceRef.current, lastCursorRef.current);
          onEventRef.current(item.event);
          return;
        }
        onEventRef.current(raw as ServerEvent);
      };
    };

    connect();
    return () => {
      cancelled = true;
      if (reconnectTimer !== undefined) window.clearTimeout(reconnectTimer);
      socket?.close();
      if (socketRef.current === socket) socketRef.current = null;
      clearReplayContext();
    };
    // Intentionally NOT depending on onEvent/onReconnect: they're read via
    // refs above so a caller passing an inline (identity-unstable)
    // onReconnect doesn't tear down and reopen the socket on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId]);

  function send(command: unknown): boolean {
    if (socketRef.current?.readyState !== WebSocket.OPEN) return false;
    socketRef.current.send(JSON.stringify(command));
    return true;
  }

  return {
    socketRef,
    socketState,
    setSocketState,
    lastSequenceRef,
    lastCursorRef,
    send,
  };
}
