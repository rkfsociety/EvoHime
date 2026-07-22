import { useEffect, useRef, useState } from "react";
import type { HistoryItem, ServerEvent } from "../protocol";
import { websocketUrl } from "../api/client";

type SocketState = "idle" | "connecting" | "reconnecting" | "connected" | "failed";

type UseWebSocketOptions = {
  sessionId: string | null;
  onEvent: (event: ServerEvent) => void;
};

export function useWebSocket({ sessionId, onEvent }: UseWebSocketOptions) {
  const [socketState, setSocketState] = useState<SocketState>("idle");
  const socketRef = useRef<WebSocket | null>(null);
  const lastSequenceRef = useRef(0);

  useEffect(() => {
    if (!sessionId) {
      return;
    }

    let cancelled = false;
    let socket: WebSocket | null = null;
    let reconnectTimer: number | undefined;
    let attempt = 0;

    const connect = () => {
      if (cancelled) return;
      setSocketState(attempt === 0 ? "connecting" : "reconnecting");
      const after = lastSequenceRef.current;
      const path = after > 0 ? `/ws/${sessionId}?after_sequence=${after}` : `/ws/${sessionId}`;
      socket = new WebSocket(websocketUrl(path));
      socketRef.current = socket;

      socket.onopen = () => {
        if (cancelled) return;
        attempt = 0;
        setSocketState("connected");
      };
      socket.onclose = () => {
        if (cancelled) return;
        setSocketState("reconnecting");
        const delay = Math.min(10_000, 500 * 2 ** Math.min(attempt, 5));
        attempt += 1;
        reconnectTimer = window.setTimeout(connect, delay);
      };
      socket.onerror = () => undefined;
      socket.onmessage = (messageEvent) => {
        const raw = JSON.parse(messageEvent.data as string) as HistoryItem | ServerEvent;
        if (raw && typeof raw === "object" && "sequence" in raw && "event" in raw && typeof (raw as HistoryItem).sequence === "number") {
          const item = raw as HistoryItem;
          if (item.sequence <= lastSequenceRef.current) return;
          lastSequenceRef.current = item.sequence;
          onEvent(item.event);
          return;
        }
        onEvent(raw as ServerEvent);
      };
    };

    connect();
    return () => {
      cancelled = true;
      if (reconnectTimer !== undefined) window.clearTimeout(reconnectTimer);
      socket?.close();
      if (socketRef.current === socket) socketRef.current = null;
    };
  }, [onEvent, sessionId]);

  function send(command: unknown): boolean {
    if (socketRef.current?.readyState !== WebSocket.OPEN) return false;
    socketRef.current.send(JSON.stringify(command));
    return true;
  }

  return { socketRef, socketState, setSocketState, lastSequenceRef, send };
}
