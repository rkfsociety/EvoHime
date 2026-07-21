import { useCallback, useEffect, useState } from "react";
import { scheduledApi, type ScheduledTask } from "../api";
import { formatRelativeAge } from "../lib/format";

// cron crate v0.17 uses 6 fields: sec min hour day-of-month month day-of-week
const CRON_TEMPLATES = [
  {
    icon: "♧",
    label: "Рабочие дни в 8:00",
    cron: "0 0 8 * * 1-5",
    description: "По будням в 08:00",
  },
  {
    icon: "▤",
    label: "Пятница в 16:00",
    cron: "0 0 16 * * 5",
    description: "Еженедельно по пятницам",
  },
  {
    icon: "⌕",
    label: "Рабочие дни в 9:00",
    cron: "0 0 9 * * 1-5",
    description: "По будням в 09:00",
  },
  {
    icon: "▷",
    label: "Ежедневно в полночь",
    cron: "0 0 0 * * *",
    description: "Каждый день в 00:00",
  },
];

type StatusFilter = "all" | "active" | "paused";

type ScheduledPanelProps = {
  onPickPrompt: (prompt: string) => void;
  workspacePath: string;
};

export function ScheduledPanel({ onPickPrompt, workspacePath }: ScheduledPanelProps) {
  const [tasks, setTasks] = useState<ScheduledTask[]>([]);
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Create form
  const [creating, setCreating] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [newPrompt, setNewPrompt] = useState("");
  const [newCron, setNewCron] = useState("0 0 8 * * 1-5");

  const reload = useCallback(() => {
    if (!workspacePath) {
      setTasks([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    void scheduledApi
      .listScheduled(workspacePath)
      .then((items) => {
        setTasks(items);
        setError(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [workspacePath]);

  useEffect(() => {
    reload();
  }, [reload]);

  const visible =
    statusFilter === "all"
      ? tasks
      : tasks.filter((t) => t.status === statusFilter);

  async function create() {
    if (!newTitle.trim() || !newPrompt.trim() || !newCron.trim()) {
      setError("Заполни все поля");
      return;
    }
    setError(null);
    try {
      const task = await scheduledApi.createScheduled(workspacePath, {
        title: newTitle.trim(),
        prompt: newPrompt.trim(),
        cron_expr: newCron.trim(),
      });
      setTasks((current) => [task, ...current]);
      setCreating(false);
      setNewTitle("");
      setNewPrompt("");
      setNewCron("0 8 * * 1-5");
    } catch (e) {
      setError(String(e));
    }
  }

  async function remove(id: string) {
    try {
      await scheduledApi.deleteScheduled(workspacePath, id);
      setTasks((current) => current.filter((t) => t.id !== id));
    } catch (e) {
      setError(String(e));
    }
  }

  async function pause(id: string) {
    try {
      const updated = await scheduledApi.pauseScheduled(workspacePath, id);
      setTasks((current) => current.map((t) => (t.id === updated.id ? updated : t)));
    } catch (e) {
      setError(String(e));
    }
  }

  async function resume(id: string) {
    try {
      const updated = await scheduledApi.resumeScheduled(workspacePath, id);
      setTasks((current) => current.map((t) => (t.id === updated.id ? updated : t)));
    } catch (e) {
      setError(String(e));
    }
  }

  async function trigger(id: string) {
    setError(null);
    try {
      const updated = await scheduledApi.triggerScheduled(workspacePath, id);
      setTasks((current) => current.map((t) => (t.id === updated.id ? updated : t)));
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="scheduledPage">
      <section className="scheduledHero">
        <div>
          <h2>Запланированные задачи</h2>
          <p>
            Автоматически запускай агента по расписанию. Используй стандартные cron-выражения.
          </p>
        </div>
        <div className="scheduledMeta">
          <strong>{loading ? "…" : tasks.length}</strong>
          <span>расписаний</span>
        </div>
      </section>

      <div className="scheduledToolbar">
        <div className="scheduledTabs" role="tablist">
          {(
            [
              ["all", "Все"],
              ["active", "Активные"],
              ["paused", "Приостановленные"],
            ] as const
          ).map(([value, label]) => (
            <button
              key={value}
              type="button"
              role="tab"
              aria-selected={statusFilter === value}
              className={statusFilter === value ? "scheduledTab active" : "scheduledTab"}
              onClick={() => setStatusFilter(value)}
            >
              {label}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="scheduledCreateButton"
          onClick={() => setCreating((v) => !v)}
        >
          {creating ? "Отмена" : "+ Добавить"}
        </button>
      </div>

      {error ? <p className="scheduledError" role="alert">{error}</p> : null}

      {creating ? (
        <form
          className="scheduledCreateForm"
          onSubmit={(e) => {
            e.preventDefault();
            void create();
          }}
        >
          <h4>Новое расписание</h4>
          <input
            required
            maxLength={200}
            value={newTitle}
            onChange={(e) => setNewTitle(e.target.value)}
            placeholder="Название"
            aria-label="Название расписания"
          />
          <textarea
            required
            rows={3}
            maxLength={8000}
            value={newPrompt}
            onChange={(e) => setNewPrompt(e.target.value)}
            placeholder="Промпт для агента"
            aria-label="Промпт"
          />
          <div className="scheduledCronRow">
            <input
              required
              value={newCron}
              onChange={(e) => setNewCron(e.target.value)}
              placeholder="Cron-выражение, например 0 8 * * 1-5"
              aria-label="Cron-выражение"
              className="scheduledCronInput"
            />
            <div className="scheduledCronTemplates">
              {CRON_TEMPLATES.map((t) => (
                <button
                  key={t.cron}
                  type="button"
                  title={t.description}
                  className="scheduledCronChip"
                  onClick={() => setNewCron(t.cron)}
                >
                  {t.icon} {t.label}
                </button>
              ))}
            </div>
          </div>
          <div className="scheduledFormActions">
            <button type="submit" className="primaryButton">
              Создать
            </button>
            <button type="button" onClick={() => setCreating(false)}>
              Отмена
            </button>
          </div>
        </form>
      ) : null}

      <div className="scheduledBody">
        {loading ? (
          <p className="scheduledEmpty">Загрузка…</p>
        ) : visible.length === 0 ? (
          <div className="scheduledEmpty">
            <strong>
              {statusFilter === "all" ? "Расписаний пока нет" : "Нет расписаний в этой категории"}
            </strong>
            <p>
              {statusFilter === "all"
                ? "Добавь первое расписание — агент будет запускаться автоматически."
                : "Переключи фильтр или создай новое расписание."}
            </p>
          </div>
        ) : (
          <div className="scheduledList">
            {visible.map((task) => (
              <article className="scheduledCard" key={task.id}>
                <div className="scheduledCardHeader">
                  <div>
                    <strong className="scheduledCardTitle">{task.title}</strong>
                    <span
                      className={
                        task.status === "active"
                          ? "scheduledBadge scheduledBadgeActive"
                          : "scheduledBadge scheduledBadgePaused"
                      }
                    >
                      {task.status === "active" ? "активно" : "приостановлено"}
                    </span>
                  </div>
                  <code className="scheduledCronBadge">{task.cron_expr}</code>
                </div>
                <p className="scheduledCardPrompt">{task.prompt}</p>
                <div className="scheduledCardMeta">
                  <span>
                    Запусков: <strong>{task.run_count}</strong>
                  </span>
                  {task.last_run_at ? (
                    <span>
                      Последний: <strong>{formatRelativeAge(task.last_run_at)}</strong>
                    </span>
                  ) : null}
                  <span>
                    Следующий: <strong>{formatRelativeAge(task.next_run_at)}</strong>
                  </span>
                </div>
                <div className="scheduledCardActions">
                  <button
                    type="button"
                    onClick={() => void trigger(task.id)}
                    title="Запустить немедленно"
                  >
                    ▷ Запустить
                  </button>
                  {task.status === "active" ? (
                    <button type="button" onClick={() => void pause(task.id)}>
                      ⏸ Пауза
                    </button>
                  ) : (
                    <button type="button" onClick={() => void resume(task.id)}>
                      ▶ Возобновить
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => {
                      onPickPrompt(task.prompt);
                    }}
                    title="Использовать промпт в чате"
                  >
                    В чат
                  </button>
                  <button type="button" onClick={() => void remove(task.id)}>
                    Удалить
                  </button>
                </div>
              </article>
            ))}
          </div>
        )}
      </div>

      {/* Cron reference hint */}
      <section className="scheduledReference">
        <h4>Cron-шпаргалка</h4>
        <div className="scheduledReferenceGrid">
          {CRON_TEMPLATES.map((t) => (
            <div key={t.cron} className="scheduledReferenceItem">
              <code>{t.cron}</code>
              <span>{t.description}</span>
            </div>
          ))}
          <div className="scheduledReferenceItem">
            <code>0 */30 * * * *</code>
            <span>Каждые 30 минут</span>
          </div>
          <div className="scheduledReferenceItem">
            <code>0 0 0 1 * *</code>
            <span>Первого числа каждого месяца</span>
          </div>
        </div>
      </section>
    </div>
  );
}
