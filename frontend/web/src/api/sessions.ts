import type { HistoryItem, SessionBootstrap } from "../protocol";
import type { ChatSessionSummary } from "../types";
import { apiRequest, apiRequestVoid } from "./client";

export function listSessions() {
  return apiRequest<ChatSessionSummary[]>("/api/sessions", undefined, "Не удалось загрузить сессии");
}

export function listArchivedSessions() {
  return apiRequest<ChatSessionSummary[]>(
    "/api/sessions/archived",
    undefined,
    "Не удалось загрузить архив",
  );
}

export function createSession() {
  return apiRequest<SessionBootstrap>(
    "/api/sessions",
    { method: "POST" },
    "Не удалось создать сессию",
  );
}

export function getSessionHistory(sessionId: string) {
  return apiRequest<HistoryItem[]>(
    `/api/sessions/${sessionId}/history`,
    undefined,
    "Не удалось загрузить историю",
  );
}

export function archiveSession(sessionId: string) {
  return apiRequestVoid(
    `/api/sessions/${sessionId}/archive`,
    { method: "POST" },
    "Не удалось архивировать чат",
  );
}

export function deleteSession(sessionId: string) {
  return apiRequestVoid(
    `/api/sessions/${sessionId}`,
    { method: "DELETE" },
    "Не удалось удалить чат",
  );
}
