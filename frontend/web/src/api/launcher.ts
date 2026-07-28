// API-клиент для REST-эндпоинта статуса Launcher'а (раздел VII/XV плана
// Installer/Launcher/Update).
//
// В отличие от остальных клиентов в этой папке (`apiRequest` из
// `./client`), этот бьёт по АБСОЛЮТНОМУ адресу на другом порту (3001,
// независимый static file server Launcher'а — раздел II плана) и
// использует другой токен (`window.__EVOHIME_TOKEN__`, инъецированный
// Launcher'ом в index.html, а не `evohime_api_token` из localStorage,
// который относится к основному серверу на 3000).

const LAUNCHER_STATUS_BASE_URL = "http://localhost:3001";

export interface LauncherComponentStatus {
  name: string;
  online: boolean;
}

export interface LauncherStatusResponse {
  components: LauncherComponentStatus[];
  update_available: boolean;
}

export class LauncherUnavailableError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "LauncherUnavailableError";
  }
}

function launcherToken(): string | null {
  const token = window.__EVOHIME_TOKEN__?.trim();
  if (!token || token === "__EVOHIME_TOKEN_PLACEHOLDER__") {
    return null;
  }
  return token;
}

/** `true`, если приложение реально запущено через Launcher (плейсхолдер
 * токена был заменён на реальное значение при раздаче index.html). */
export function isLauncherManaged(): boolean {
  return launcherToken() !== null;
}

export async function fetchLauncherStatus(): Promise<LauncherStatusResponse> {
  const token = launcherToken();
  if (!token) {
    throw new LauncherUnavailableError(
      "Launcher недоступен: приложение запущено не через EvoHime Launcher",
    );
  }

  let response: Response;
  try {
    response = await fetch(`${LAUNCHER_STATUS_BASE_URL}/status`, {
      headers: { Authorization: `Bearer ${token}` },
    });
  } catch {
    throw new LauncherUnavailableError(
      "Не удалось связаться с Launcher'ом (порт 3001 недоступен)",
    );
  }

  if (!response.ok) {
    throw new LauncherUnavailableError(
      `Launcher вернул ошибку ${response.status}`,
    );
  }

  return (await response.json()) as LauncherStatusResponse;
}
