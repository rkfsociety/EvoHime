export class ApiError extends Error {
  readonly status: number;
  readonly body: string;

  constructor(status: number, body: string, fallback: string) {
    super(body.trim() || fallback);
    this.name = "ApiError";
    this.status = status;
    this.body = body;
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
  const protocol = window.location.protocol === "https:" ? "wss" : "ws";
  const token = getApiToken();
  const base = `${protocol}://${window.location.host}${path}`;
  if (!token) {
    return base;
  }
  const join = path.includes("?") ? "&" : "?";
  return `${base}${join}access_token=${encodeURIComponent(token)}`;
}

export async function apiRequest<T>(
  path: string,
  init?: RequestInit,
  fallbackError = "Запрос к API не удался",
): Promise<T> {
  const response = await fetch(path, withAuth(init));
  const text = await response.text();
  if (!response.ok) {
    throw new ApiError(response.status, text, fallbackError);
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
    throw new ApiError(response.status, text, fallbackError);
  }
}

export function jsonBody(payload: unknown): RequestInit {
  return {
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  };
}
