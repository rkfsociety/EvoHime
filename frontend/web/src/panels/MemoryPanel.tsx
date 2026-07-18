import { useCallback, useEffect, useMemo, useState } from "react";
import {
  deleteMemory,
  listMemory,
  resolveMemoryConflict,
  updateMemory,
  type MemoryItem,
  type MemoryPrivacyInfo,
} from "../api/memory";

type MemoryTab = "active" | "candidates" | "experiences" | "conflicts" | "archived";

type PlaybookView = {
  trigger: string;
  steps: string[];
  verify?: string;
  rollback_hint?: string;
};

type ExperienceKindFilter = "all" | "playbook" | "success_pattern" | "failure_pattern" | "verification_rule";

const TABS: Array<{ id: MemoryTab; label: string; status: string }> = [
  { id: "active", label: "Активные", status: "active" },
  { id: "candidates", label: "Кандидаты", status: "candidate" },
  { id: "experiences", label: "Опыт", status: "experiences" },
  { id: "conflicts", label: "Конфликты", status: "conflict" },
  { id: "archived", label: "Архив", status: "archived" },
];

const EXPERIENCE_KIND_FILTERS: Array<{ id: ExperienceKindFilter; label: string }> = [
  { id: "all", label: "Все" },
  { id: "playbook", label: "Playbooks" },
  { id: "success_pattern", label: "Успех" },
  { id: "failure_pattern", label: "Провалы" },
  { id: "verification_rule", label: "Проверки" },
];

const KIND_LABELS: Record<string, string> = {
  fact: "факт",
  preference: "предпочтение",
  constraint: "ограничение",
  success_pattern: "паттерн успеха",
  failure_pattern: "паттерн провала",
  verification_rule: "правило проверки",
  playbook: "playbook",
};

function kindLabel(kind: string) {
  return KIND_LABELS[kind] ?? kind;
}

function parsePlaybook(value: unknown): PlaybookView | null {
  if (!value || typeof value !== "object") {
    return null;
  }
  const record = value as Record<string, unknown>;
  const trigger = typeof record.trigger === "string" ? record.trigger.trim() : "";
  const steps = Array.isArray(record.steps)
    ? record.steps.filter((step): step is string => typeof step === "string" && step.trim().length > 0)
    : [];
  if (!trigger || steps.length === 0) {
    return null;
  }
  return {
    trigger,
    steps,
    verify: typeof record.verify === "string" && record.verify.trim() ? record.verify : undefined,
    rollback_hint:
      typeof record.rollback_hint === "string" && record.rollback_hint.trim()
        ? record.rollback_hint
        : undefined,
  };
}

function formatUpdatedAt(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("ru-RU", {
    day: "2-digit",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function downloadMemoryExport(items: MemoryItem[], tab: MemoryTab) {
  const blob = new Blob([JSON.stringify({ tab, exported_at: new Date().toISOString(), items }, null, 2)], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `evohime-memory-${tab}.json`;
  anchor.click();
  URL.revokeObjectURL(url);
}

export function MemoryPanel() {
  const [tab, setTab] = useState<MemoryTab>("active");
  const [kindFilter, setKindFilter] = useState<ExperienceKindFilter>("all");
  const [items, setItems] = useState<MemoryItem[]>([]);
  const [privacy, setPrivacy] = useState<MemoryPrivacyInfo | null>(null);
  const [queryDraft, setQueryDraft] = useState("");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  useEffect(() => {
    const handle = window.setTimeout(() => setQuery(queryDraft.trim()), 300);
    return () => window.clearTimeout(handle);
  }, [queryDraft]);

  useEffect(() => {
    setKindFilter("all");
  }, [tab]);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const status = TABS.find((item) => item.id === tab)?.status ?? "active";
      const response = await listMemory({ status, q: query || undefined, limit: 150 });
      setItems(response.items);
      setPrivacy(response.privacy);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Не удалось загрузить память");
      setItems([]);
    } finally {
      setLoading(false);
    }
  }, [query, tab]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const visibleItems = useMemo(() => {
    if (tab !== "experiences" || kindFilter === "all") {
      return items;
    }
    return items.filter((item) => item.kind === kindFilter);
  }, [items, kindFilter, tab]);

  async function runAction(id: string, action: () => Promise<unknown>) {
    setBusyId(id);
    setError(null);
    try {
      await action();
      await refresh();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Действие не удалось");
    } finally {
      setBusyId(null);
    }
  }

  function startEdit(item: MemoryItem) {
    setEditingId(item.id);
    setDraft(item.content);
  }

  async function saveEdit(id: string) {
    await runAction(id, () => updateMemory(id, { content: draft }));
    setEditingId(null);
  }

  async function resolveConflict(conflictId: string, winnerId: string) {
    await runAction(conflictId, () => resolveMemoryConflict(conflictId, winnerId));
  }

  return (
    <div className="actionsPanel memoryPanel">
      <div className="panelToolbar">
        <div>
          <strong>Память</strong>
          <span>Override и прозрачность — не очередь обязательных approve</span>
        </div>
        <div className="actionMetrics">
          <span>
            {visibleItems.length}
            {visibleItems.length !== items.length ? ` / ${items.length}` : ""} на вкладке
          </span>
          <button
            type="button"
            onClick={() => downloadMemoryExport(visibleItems, tab)}
            disabled={loading || visibleItems.length === 0}
          >
            Экспорт JSON
          </button>
          <button type="button" onClick={() => void refresh()} disabled={loading}>
            Обновить
          </button>
        </div>
      </div>

      {privacy ? (
        <div className="memoryPrivacy">
          <strong>Privacy</strong>
          <p>
            Redaction: {privacy.redaction_enabled ? "всегда включена" : "выключена"}. {privacy.policy}
          </p>
        </div>
      ) : null}

      <div className="memoryTabs">
        {TABS.map((item) => (
          <button
            key={item.id}
            type="button"
            className={tab === item.id ? "memoryTab active" : "memoryTab"}
            onClick={() => setTab(item.id)}
          >
            {item.label}
          </button>
        ))}
      </div>

      {tab === "experiences" ? (
        <div className="memoryKindFilters">
          {EXPERIENCE_KIND_FILTERS.map((filter) => (
            <button
              key={filter.id}
              type="button"
              className={kindFilter === filter.id ? "memoryKindFilter active" : "memoryKindFilter"}
              onClick={() => setKindFilter(filter.id)}
            >
              {filter.label}
            </button>
          ))}
        </div>
      ) : null}

      <div className="panelToolbar">
        <input
          className="memorySearch"
          value={queryDraft}
          onChange={(event) => setQueryDraft(event.target.value)}
          placeholder="Поиск по содержимому…"
        />
      </div>

      {error ? <div className="panelError">{error}</div> : null}

      {loading ? (
        <div className="emptyPanelState">
          <strong>Загрузка…</strong>
        </div>
      ) : visibleItems.length === 0 ? (
        <div className="emptyPanelState">
          <strong>Записей нет</strong>
          <span>
            {tab === "experiences"
              ? "Пока нет опыта и playbooks — они появятся после задач с паттернами успеха/провала."
              : "На этой вкладке пока пусто — агент накопит факты и опыт сам."}
          </span>
        </div>
      ) : (
        <div className="memoryList">
          {visibleItems.map((item) => {
            const relatedConflict = tab === "conflicts" && item.supersedes
              ? visibleItems.find((candidate) => candidate.id === item.supersedes)
              : undefined;
            if (tab === "conflicts" && !item.supersedes && visibleItems.some((candidate) => candidate.supersedes === item.id)) {
              return null;
            }
            const busy = busyId === item.id;
            const editing = editingId === item.id;
            const playbook = item.kind === "playbook" ? parsePlaybook(item.content_json) : null;
            if (tab === "conflicts" && item.supersedes && relatedConflict) {
              return (
                <article className="memoryConflictPair" key={item.id}>
                  <div className="memoryConflictHeader">
                    <strong>Конфликт памяти</strong>
                    <span>{kindLabel(item.kind)} · выберите актуальную запись</span>
                  </div>
                  <div className="memoryConflictColumns">
                    {[item, relatedConflict].map((candidate) => (
                      <div className="memoryConflictCard" key={candidate.id}>
                        <div className="memoryItemHeader">
                          <strong>
                            <span className="memoryKindBadge">{candidate.id === item.id ? "Новая" : "Текущая"}</span>
                            <span className="memoryScopeLabel">{candidate.scope}</span>
                          </strong>
                          <span>conf {candidate.confidence.toFixed(2)}</span>
                        </div>
                        <p>{candidate.content}</p>
                        <div className="memoryMeta">
                          <code>{candidate.scope_key}</code>
                          <time dateTime={candidate.updated_at}>{formatUpdatedAt(candidate.updated_at)}</time>
                        </div>
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => void resolveConflict(item.id, candidate.id)}
                        >
                          Оставить эту запись
                        </button>
                      </div>
                    ))}
                  </div>
                </article>
              );
            }
            return (
              <article
                className="memoryItem"
                key={item.id}
                data-pinned={item.pinned}
                data-kind={item.kind}
              >
                <div className="memoryItemHeader">
                  <strong>
                    <span className="memoryKindBadge">{kindLabel(item.kind)}</span>
                    <span className="memoryScopeLabel">
                      {item.scope}
                      {item.pinned ? " · pinned" : ""}
                    </span>
                  </strong>
                  <span>
                    {item.status} · conf {item.confidence.toFixed(2)}
                    {typeof item.use_count === "number" ? ` · used ${item.use_count}` : ""}
                    {typeof item.helpful_count === "number" && item.helpful_count > 0
                      ? ` · +${item.helpful_count}`
                      : ""}
                    {typeof item.harmful_count === "number" && item.harmful_count > 0
                      ? ` · -${item.harmful_count}`
                      : ""}
                  </span>
                </div>
                {editing ? (
                  <textarea
                    className="memoryEdit"
                    value={draft}
                    onChange={(event) => setDraft(event.target.value)}
                    rows={4}
                  />
                ) : playbook ? (
                  <div className="memoryPlaybook">
                    <p className="memoryPlaybookTrigger">
                      <strong>Когда:</strong> {playbook.trigger}
                    </p>
                    <ol>
                      {playbook.steps.map((step, index) => (
                        <li key={`${index}-${step}`}>{step}</li>
                      ))}
                    </ol>
                    {playbook.verify ? (
                      <p>
                        <strong>Проверка:</strong> {playbook.verify}
                      </p>
                    ) : null}
                    {playbook.rollback_hint ? (
                      <p>
                        <strong>Откат:</strong> {playbook.rollback_hint}
                      </p>
                    ) : null}
                    <p className="memoryPlaybookFallback">{item.content}</p>
                  </div>
                ) : (
                  <p>{item.content}</p>
                )}
                <div className="memoryMeta">
                  <code>{item.scope_key}</code>
                  {item.source_label ? <span title="source">{item.source_label}</span> : null}
                  {item.supersedes ? (
                    <span title="supersedes">→ {item.supersedes.slice(0, 8)}</span>
                  ) : null}
                  <time dateTime={item.updated_at}>{formatUpdatedAt(item.updated_at)}</time>
                </div>
                <div className="memoryActions">
                  {editing ? (
                    <>
                      <button type="button" disabled={busy} onClick={() => void saveEdit(item.id)}>
                        Сохранить
                      </button>
                      <button type="button" disabled={busy} onClick={() => setEditingId(null)}>
                        Отмена
                      </button>
                    </>
                  ) : (
                    <button type="button" disabled={busy} onClick={() => startEdit(item)}>
                      Править
                    </button>
                  )}
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => void runAction(item.id, () => updateMemory(item.id, { pinned: !item.pinned }))}
                  >
                    {item.pinned ? "Unpin" : "Pin"}
                  </button>
                  {item.status !== "active" ? (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void runAction(item.id, () => updateMemory(item.id, { status: "active" }))}
                    >
                      Activate
                    </button>
                  ) : null}
                  {item.status !== "rejected" ? (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void runAction(item.id, () => updateMemory(item.id, { status: "rejected" }))}
                    >
                      Reject
                    </button>
                  ) : null}
                  {item.status !== "archived" ? (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void runAction(item.id, () => updateMemory(item.id, { status: "archived" }))}
                    >
                      Archive
                    </button>
                  ) : null}
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => void runAction(item.id, () => deleteMemory(item.id))}
                  >
                    Delete
                  </button>
                </div>
              </article>
            );
          })}
        </div>
      )}
    </div>
  );
}
