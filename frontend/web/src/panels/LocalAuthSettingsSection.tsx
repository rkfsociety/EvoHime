import { useCallback, useEffect, useState } from "react";
import { apiRequest, getApiToken, setApiToken } from "../api/client";

type AuthStatus = {
  token_configured: boolean;
  mode: string;
  hint: string;
};

export function LocalAuthSettingsSection() {
  const [status, setStatus] = useState<AuthStatus | null>(null);
  const [draft, setDraft] = useState(() => getApiToken() ?? "");
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setError(null);
    try {
      setStatus(await apiRequest<AuthStatus>("/api/auth/status"));
    } catch (err) {
      setError(String(err));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const onSave = () => {
    setApiToken(draft);
    setNotice(draft.trim() ? "Токен сохранён в localStorage браузера" : "Токен очищен");
  };

  return (
    <section className="settingsSection localAuthSettings">
      <h3>Доступ к API</h3>
      <p className="settingsHint">
        Stage 7.1: без `EVOHIME_API_TOKEN` loopback открыт; с токеном нужен Bearer на HTTP/WS.
      </p>
      {error ? <p className="settingsError">{error}</p> : null}
      {status ? (
        <p className="settingsHint">
          Режим сервера: <code>{status.mode}</code>
          {status.token_configured ? " (токен задан в env)" : " (токен в env не задан)"}
        </p>
      ) : null}
      <label className="modelProviderKey">
        <span>Bearer token (localStorage)</span>
        <input
          type="password"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          placeholder="Оставь пустым для loopback без токена"
          autoComplete="off"
        />
      </label>
      <div className="settingsInlineBar">
        <button type="button" onClick={onSave}>
          Сохранить токен
        </button>
        <button type="button" onClick={() => void refresh()}>
          Обновить статус
        </button>
      </div>
      {notice ? <p className="settingsHint">{notice}</p> : null}
    </section>
  );
}
