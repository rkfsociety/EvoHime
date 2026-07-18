import { apiRequest, apiRequestBlob, apiRequestVoid } from "./client";

export type MemoryItem = {
  id: string;
  scope: string;
  scope_key: string;
  kind: string;
  status: string;
  content: string;
  content_json?: unknown;
  confidence: number;
  importance: number;
  pinned: boolean;
  source_session_id?: string | null;
  source_task_id?: string | null;
  source_label?: string | null;
  supersedes?: string | null;
  valid_until?: string | null;
  validity_hint?: string | null;
  last_used_at?: string | null;
  use_count?: number;
  helpful_count?: number;
  harmful_count?: number;
  created_at: string;
  updated_at: string;
};

export type MemoryPrivacyInfo = {
  redaction_enabled: boolean;
  policy: string;
};

export type MemoryListResponse = {
  items: MemoryItem[];
  privacy: MemoryPrivacyInfo;
  next_cursor?: string | null;
};

export type MemoryListParams = {
  scope?: string;
  scope_key?: string;
  status?: string;
  q?: string;
  limit?: number;
  cursor?: string;
};

export type MemoryUpdatePayload = {
  content?: string;
  status?: string;
  pinned?: boolean;
};

export type MemoryCreatePayload = {
  content: string;
  scope?: string;
  scope_key?: string;
  kind?: string;
  confidence?: number;
  importance?: number;
  pinned?: boolean;
};

export type MemoryCreateResponse = {
  outcome: "inserted" | "duplicate" | "conflict" | "rejected";
  item?: MemoryItem | null;
  existing_id?: string | null;
  reason?: string | null;
};

export type MemoryPackFormat = "json" | "zip";

export type MemoryImportResponse = {
  inserted: number;
  duplicates: number;
  conflicts: number;
  rejected: number;
  errors: string[];
};

export function exportMemoryPack(format: MemoryPackFormat) {
  return apiRequestBlob(`/api/memory/export?format=${format}`, undefined, "Не удалось экспортировать memory pack");
}

export function importMemoryPack(body: BodyInit, contentType: string) {
  return apiRequest<MemoryImportResponse>(
    "/api/memory/import",
    { method: "POST", headers: { "Content-Type": contentType }, body },
    "Не удалось импортировать memory pack",
  );
}

function toQuery(params: MemoryListParams) {
  const query = new URLSearchParams();
  if (params.scope) query.set("scope", params.scope);
  if (params.scope_key) query.set("scope_key", params.scope_key);
  if (params.status) query.set("status", params.status);
  if (params.q) query.set("q", params.q);
  if (params.limit != null) query.set("limit", String(params.limit));
  if (params.cursor) query.set("cursor", params.cursor);
  const text = query.toString();
  return text ? `?${text}` : "";
}

export function listMemory(params: MemoryListParams = {}) {
  return apiRequest<MemoryListResponse>(
    `/api/memory${toQuery(params)}`,
    undefined,
    "Не удалось загрузить память",
  );
}

export function createMemory(payload: MemoryCreatePayload) {
  return apiRequest<MemoryCreateResponse>(
    "/api/memory",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    },
    "Не удалось добавить запись памяти",
  );
}

export function updateMemory(id: string, payload: MemoryUpdatePayload) {
  return apiRequest<MemoryItem>(
    `/api/memory/${id}`,
    {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    },
    "Не удалось обновить запись памяти",
  );
}

export function resolveMemoryConflict(id: string, winnerId: string) {
  return apiRequest<{ winner: MemoryItem; loser: MemoryItem }>(
    `/api/memory/${id}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ winner_id: winnerId }),
    },
    "Не удалось разрешить конфликт памяти",
  );
}

export function deleteMemory(id: string) {
  return apiRequestVoid(`/api/memory/${id}`, { method: "DELETE" }, "Не удалось удалить запись памяти");
}
