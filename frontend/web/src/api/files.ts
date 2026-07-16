import type { FileContent, FileListing, SaveResponse } from "../types";
import { apiRequest } from "./client";

export function listFiles(path?: string) {
  const query = path && path !== "." ? `?path=${encodeURIComponent(path)}` : "";
  return apiRequest<FileListing>(`/api/files${query}`, undefined, "Не удалось загрузить файлы");
}

export function readFile(path: string) {
  return apiRequest<FileContent>(
    `/api/files/content?path=${encodeURIComponent(path)}`,
    undefined,
    "Не удалось прочитать файл",
  );
}

export function saveFile(path: string, content: string, sessionId: string) {
  return apiRequest<SaveResponse>(
    `/api/files/content?path=${encodeURIComponent(path)}&session_id=${encodeURIComponent(sessionId)}`,
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ content }),
    },
    "Не удалось сохранить файл",
  );
}

export function createFile(path: string, content: string, sessionId: string) {
  return apiRequest<SaveResponse>(
    `/api/files/content?path=${encodeURIComponent(path)}&session_id=${encodeURIComponent(sessionId)}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ content }),
    },
    "Не удалось создать файл",
  );
}
