import type { GitSnapshot } from "../types";
import { apiRequest } from "./client";

export function getGitStatus() {
  return apiRequest<GitSnapshot>("/api/git/status", undefined, "Не удалось получить статус Git");
}

export function getGitDiff(path?: string | null) {
  const normalized = path && path !== "." ? path : null;
  const query = normalized ? `?path=${encodeURIComponent(normalized)}` : "";
  return apiRequest<GitSnapshot>(`/api/git/diff${query}`, undefined, "Не удалось получить diff");
}

export function gitCommit(sessionId: string, message: string) {
  return apiRequest<{ ok?: boolean }>(
    `/api/git/commit?session_id=${encodeURIComponent(sessionId)}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ message }),
    },
    "Не удалось сделать коммит",
  );
}

export function gitPull(sessionId: string, remote?: string, branch?: string) {
  return apiRequest<{ ok?: boolean }>(
    `/api/git/pull?session_id=${encodeURIComponent(sessionId)}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ remote: remote || undefined, branch: branch || undefined }),
    },
    "Не удалось выполнить pull",
  );
}

export function gitPush(sessionId: string, remote?: string, branch?: string) {
  return apiRequest<{ ok?: boolean }>(
    `/api/git/push?session_id=${encodeURIComponent(sessionId)}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ remote: remote || undefined, branch: branch || undefined }),
    },
    "Не удалось выполнить push",
  );
}
