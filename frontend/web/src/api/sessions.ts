import type { HistoryItem, SessionBootstrap } from "../protocol";
import type { ChatSessionSummary, SessionAttachment } from "../types";
import { ApiError, apiRequest, apiRequestVoid, parseApiErrorBody, withAuth } from "./client";

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

export function getSessionHistory(sessionId: string, afterSequence = 0) {
  const query = afterSequence > 0 ? `?after=${afterSequence}` : "";
  return apiRequest<HistoryItem[]>(
    `/api/sessions/${sessionId}/history${query}`,
    undefined,
    "Не удалось загрузить историю",
  );
}

export interface PaginatedHistoryResponse {
  items: HistoryItem[];
  next_cursor?: string;
  prev_cursor?: string;
  has_more: boolean;
  total_available: number;
}

export function getSessionHistoryPaginated(
  sessionId: string,
  limit = 50,
  cursor?: string,
  order: "asc" | "desc" = "asc",
) {
  const params = new URLSearchParams({
    limit: Math.max(1, Math.min(500, limit)).toString(),
    order,
  });
  if (cursor) {
    params.set("cursor", cursor);
  }
  return apiRequest<PaginatedHistoryResponse>(
    `/api/sessions/${sessionId}/history?${params.toString()}`,
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

export function unarchiveSession(sessionId: string) {
  return apiRequestVoid(
    `/api/sessions/${sessionId}/unarchive`,
    { method: "POST" },
    "Не удалось восстановить чат",
  );
}

export function deleteSession(sessionId: string) {
  return apiRequestVoid(
    `/api/sessions/${sessionId}`,
    { method: "DELETE" },
    "Не удалось удалить чат",
  );
}

export async function uploadAttachments(
  sessionId: string,
  files: File[],
  workspacePath?: string | null,
) {
  const form = new FormData();
  for (const file of files) {
    form.append("files", file, file.name);
  }
  const query = workspacePath
    ? `?workspace_path=${encodeURIComponent(workspacePath)}`
    : "";
  const response = await fetch(
    `/api/sessions/${sessionId}/attachments${query}`,
    withAuth({ method: "POST", body: form }),
  );
  const text = await response.text();
  if (!response.ok) {
    throw new ApiError(
      response.status,
      text,
      "Не удалось загрузить вложения",
      parseApiErrorBody(text),
    );
  }
  return JSON.parse(text) as SessionAttachment[];
}
