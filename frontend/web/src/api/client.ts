export type ApiErrorCode =
  | "bad_request"
  | "conflict"
  | "approval_required"
  | "rate_limited"
  | "internal"
  | "unavailable"
  | "unauthorized"
  | "not_found"
  | string;

export type ApiErrorBody = {
  error: string;
  code?: ApiErrorCode;
  retryable?: boolean;
  message?: string;
  tool?: string;
  approval_id?: string;
};

export class ApiError extends Error {
  readonly status: number;
  readonly body: string;
  readonly code: ApiErrorCode | null;
  readonly retryable: boolean;
  readonly details: ApiErrorBody | null;

  constructor(
    status: number,
    body: string,
    fallback: string,
    parsed: ApiErrorBody | null = null,
  ) {
    const message =
      parsed?.message?.trim() ||
      parsed?.error?.trim() ||
      body.trim() ||
      fallback;
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.body = body;
    this.details = parsed;
    this.code = parsed?.code ?? inferCodeFromStatus(status);
    this.retryable =
      typeof parsed?.retryable === "boolean"
        ? parsed.retryable
        : status === 429 || status === 503 || status >= 500;
  }
}

function inferCodeFromStatus(status: number): ApiErrorCode | null {
  switch (status) {
    case 400:
      return "bad_request";
    case 401:
      return "unauthorized";
    case 404:
      return "not_found";
    case 409:
      return "conflict";
    case 429:
      return "rate_limited";
    case 503:
      return "unavailable";
    default:
      return status >= 500 ? "internal" : null;
  }
}

export function parseApiErrorBody(text: string): ApiErrorBody | null {
  const trimmed = text.trim();
  if (!trimmed.startsWith("{")) {
    return null;
  }
  try {
    const value = JSON.parse(trimmed) as unknown;
    if (!value || typeof value !== "object") {
      return null;
    }
    const record = value as Record<string, unknown>;
    const error =
      typeof record.error === "string"
        ? record.error
        : typeof record.message === "string"
          ? record.message
          : "";
    if (!error && record.code == null) {
      return null;
    }
    return {
      error: error || "request failed",
      code: typeof record.code === "string" ? record.code : undefined,
      retryable:
        typeof record.retryable === "boolean" ? record.retryable : undefined,
      message: typeof record.message === "string" ? record.message : undefined,
      tool: typeof record.tool === "string" ? record.tool : undefined,
      approval_id:
        typeof record.approval_id === "string" ? record.approval_id : undefined,
    };
  } catch {
    return null;
  }
}

const API_TOKEN_STORAGE_KEY = "evohime_api_token";

export function getApiToken(): string | null {
  if (typeof window === "undefined") {
    return null;
  }
  const stored = window.localStorage.getItem(API_TOKEN_STORAGE_KEY)?.trim();
  return stored || null;
}

export function setApiToken(token: string | null) {
  if (typeof window === "undefined") {
    return;
  }
  const trimmed = token?.trim() ?? "";
  if (!trimmed) {
    window.localStorage.removeItem(API_TOKEN_STORAGE_KEY);
    return;
  }
  window.localStorage.setItem(API_TOKEN_STORAGE_KEY, trimmed);
}

export function authHeaders(): HeadersInit {
  const token = getApiToken();
  return token ? { Authorization: `Bearer ${token}` } : {};
}

export function withAuth(init?: RequestInit): RequestInit {
  const headers = new Headers(init?.headers);
  const token = getApiToken();
  if (token && !headers.has("Authorization")) {
    headers.set("Authorization", `Bearer ${token}`);
  }
  return { ...init, headers };
}

export function websocketUrl(path: string): string {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  const url = new URL(path, `${protocol}//${window.location.host}`);
  const token = getApiToken();
  if (token) {
    url.searchParams.set("access_token", token);
  }
  return url.toString();
}

function throwApiError(status: number, text: string, fallbackError: string): never {
  throw new ApiError(status, text, fallbackError, parseApiErrorBody(text));
}

export async function apiRequest<T>(
  path: string,
  init?: RequestInit,
  fallbackError = "Запрос к API не удался",
): Promise<T> {
  const response = await fetch(path, withAuth(init));
  const text = await response.text();
  if (!response.ok) {
    throwApiError(response.status, text, fallbackError);
  }
  if (!text) {
    return undefined as T;
  }
  return JSON.parse(text) as T;
}

export async function apiRequestVoid(
  path: string,
  init?: RequestInit,
  fallbackError = "Запрос к API не удался",
): Promise<void> {
  const response = await fetch(path, withAuth(init));
  if (!response.ok) {
    const text = await response.text();
    throwApiError(response.status, text, fallbackError);
  }
}

export async function apiRequestBlob(
  path: string,
  init?: RequestInit,
  fallbackError = "Не удалось получить файл из API",
): Promise<Blob> {
  const response = await fetch(path, withAuth(init));
  if (!response.ok) {
    const text = await response.text();
    throwApiError(response.status, text, fallbackError);
  }
  return response.blob();
}

export function jsonBody(payload: unknown): RequestInit {
  return {
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  };
}
