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

export async function apiRequest<T>(
  path: string,
  init?: RequestInit,
  fallbackError = "Запрос к API не удался",
): Promise<T> {
  const response = await fetch(path, init);
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
  const response = await fetch(path, init);
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
