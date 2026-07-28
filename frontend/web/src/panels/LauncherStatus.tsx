import { useCallback, useEffect, useRef, useState } from "react";
import {
  fetchLauncherStatus,
  isLauncherManaged,
  type LauncherStatusResponse,
} from "../api/launcher";

const POLL_INTERVAL_MS = 3000;

export function LauncherStatusSection() {
  const managed = isLauncherManaged();
  const [status, setStatus] = useState<LauncherStatusResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const pollTimer = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const next = await fetchLauncherStatus();
      setStatus(next);
      setError(null);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!managed) {
      return;
    }
    void refresh();
    pollTimer.current = setInterval(() => void refresh(), POLL_INTERVAL_MS);
    return () => {
      if (pollTimer.current) {
        clearInterval(pollTimer.current);
      }
    };
  }, [managed, refresh]);

  if (!managed) {
    return (
      <section className="settingsSection launcherStatusSection">
        <h3>EvoHime Launcher</h3>
        <p className="settingsHint">
          Приложение запущено не через EvoHime Launcher — статус компонентов,
          обновления и история недоступны в этом режиме.
        </p>
      </section>
    );
  }

  return (
    <section className="settingsSection launcherStatusSection">
      <div className="settingsInlineBar">
        <div>
          <h3>EvoHime Launcher</h3>
          <p className="settingsHint">
            Статус компонентов и обновления опрашиваются каждые{" "}
            {POLL_INTERVAL_MS / 1000} сек.
          </p>
        </div>
        <button type="button" onClick={() => void refresh()} disabled={loading}>
          {loading ? "Обновление…" : "Обновить"}
        </button>
      </div>

      {error ? <p className="settingsError">{error}</p> : null}

      {status ? (
        <>
          <ul className="launcherComponentList">
            {status.components.map((component) => (
              <li key={component.name} className="launcherComponentRow">
                <span
                  className={
                    component.online
                      ? "launcherStatusDot online"
                      : "launcherStatusDot offline"
                  }
                  aria-hidden="true"
                />
                <span>{component.name}</span>
                <span className="settingsHint">
                  {component.online ? "работает" : "не отвечает"}
                </span>
              </li>
            ))}
          </ul>

          {status.update_available ? (
            <p className="launcherUpdateBanner">
              🔴 Доступно обновление. Откройте окно EvoHime Launcher в трее,
              чтобы применить его — панель браузера обновления не запускает.
            </p>
          ) : (
            <p className="settingsHint">Обновлений нет — установлена последняя версия.</p>
          )}
        </>
      ) : null}
    </section>
  );
}
