import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  createMemory,
  deleteMemory,
  exportMemoryPack,
  importMemoryPack,
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

const MEMORY_TEMPLATES = [
  { id: "fact", label: "Факт", kind: "fact", content: "Этот workspace использует ..." },
  { id: "preference", label: "Предпочтение", kind: "preference", content: "Предпочитаю ..." },
  { id: "constraint", label: "Ограничение", kind: "constraint", content: "Всегда ..." },
  { id: "verification", label: "Проверка", kind: "verification_rule", content: "После изменений проверять ..." },
] as const;

const TABS: Array<{ id: MemoryTab; label: string; status: string }> = [
  { id: "active", label: "Активные", status: "active" },
  { id: "candidates", label: "Кандидаты", status: "candidate" },
  { id: "experiences", label: "Опыт", status: "experiences" },
  { id: "conflicts", label: "Конфликты", status: "conflict" },
  { id: "archived", label: "Архив", status: "archived" },
];

const EXPERIENCE_KIND_FILTERS: Array<{ id: ExperienceKindFilter; label: string }> = [
  { id: "all", label: "Все" },
  { id: "playbook", label: "Сценарии" },
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
  playbook: "сценарий",
};

const STATUS_LABELS: Record<string, string> = {
  active: "активна",
  candidate: "кандидат",
  conflict: "конфликт",
  archived: "архив",
  rejected: "отклонена",
};

const SCOPE_LABELS: Record<string, string> = {
  global: "пользовательская",
  workspace: "workspace",
  project: "проект",
  session: "сессия",
};

function kindLabel(kind: string) {
  return KIND_LABELS[kind] ?? kind;
}

function statusLabel(status: string) {
  return STATUS_LABELS[status] ?? status;
}

function scopeLabel(scope: string) {
  return SCOPE_LABELS[scope] ?? scope;
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

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
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
  const [loadingMore, setLoadingMore] = useState(false);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [addOpen, setAddOpen] = useState(false);
  const [addTemplate, setAddTemplate] = useState("fact");
  const [addKind, setAddKind] = useState("fact");
  const [addScope, setAddScope] = useState("global");
  const [addScopeKey, setAddScopeKey] = useState("local");
  const [addContent, setAddContent] = useState("");
  const [addBusy, setAddBusy] = useState(false);
  const [undoItem, setUndoItem] = useState<MemoryItem | null>(null);
  const [undoBusy, setUndoBusy] = useState(false);
  const undoTimer = useRef<number | null>(null);
  const importInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    return () => {
      if (undoTimer.current !== null) {
        window.clearTimeout(undoTimer.current);
      }
    };
  }, []);

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
      const response = await listMemory({ status, q: query || undefined, limit: 50 });
      setItems(response.items);
      setPrivacy(response.privacy);
      setNextCursor(response.next_cursor ?? null);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Не удалось загрузить память");
      setItems([]);
    } finally {
      setLoading(false);
    }
  }, [query, tab]);

  async function loadMore() {
    if (!nextCursor || loadingMore) {
      return;
    }
    setLoadingMore(true);
    setError(null);
    try {
      const status = TABS.find((item) => item.id === tab)?.status ?? "active";
      const response = await listMemory({
        status,
        q: query || undefined,
        limit: 50,
        cursor: nextCursor,
      });
      setItems((current) => {
        const existing = new Set(current.map((item) => item.id));
        return [...current, ...response.items.filter((item) => !existing.has(item.id))];
      });
      setPrivacy(response.privacy);
      setNextCursor(response.next_cursor ?? null);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Не удалось догрузить память");
    } finally {
      setLoadingMore(false);
    }
  }

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const visibleItems = useMemo(() => {
    if (tab !== "experiences" || kindFilter === "all") {
      return items;
    }
    return items.filter((item) => item.kind === kindFilter);
  }, [items, kindFilter, tab]);

  async function exportMemoryPackFile(format: "json" | "zip") {
    setError(null);
    setNotice(null);
    try {
      const blob = await exportMemoryPack(format);
      downloadBlob(blob, `evohime-memory-pack.${format}`);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Не удалось экспортировать пакет памяти");
    }
  }

  async function importMemoryPackFile(file: File) {
    setError(null);
    setNotice(null);
    try {
      const body = await file.arrayBuffer();
      const contentType = file.type || (file.name.toLowerCase().endsWith(".zip") ? "application/zip" : "application/json");
      const response = await importMemoryPack(body, contentType);
      await refresh();
      const details = response.errors.length > 0 ? ` Ошибок: ${response.errors.length}.` : "";
      setNotice(`Импорт завершён: добавлено ${response.inserted}, дублей ${response.duplicates}, конфликтов ${response.conflicts}, отклонено ${response.rejected}.${details}`);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Не удалось импортировать пакет памяти");
    }
  }

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

  async function deleteItem(item: MemoryItem) {
    if (!window.confirm("Удалить эту запись памяти? Её можно будет восстановить в течение 8 секунд.")) {
      return;
    }
    setBusyId(item.id);
    setError(null);
    try {
      await deleteMemory(item.id);
      setUndoItem(item);
      if (undoTimer.current !== null) {
        window.clearTimeout(undoTimer.current);
      }
      undoTimer.current = window.setTimeout(() => {
        setUndoItem(null);
        undoTimer.current = null;
      }, 8000);
      await refresh();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Не удалось удалить запись памяти");
    } finally {
      setBusyId(null);
    }
  }

  async function restoreDeletedItem() {
    if (!undoItem || undoBusy) {
      return;
    }
    const item = undoItem;
    setUndoBusy(true);
    setError(null);
    try {
      const response = await createMemory({
        content: item.content,
        scope: item.scope,
        scope_key: item.scope_key,
        kind: item.kind,
        confidence: item.confidence,
        importance: item.importance,
        pinned: item.pinned,
      });
      if (response.outcome === "duplicate") {
        setError("Запись уже существует, восстановление не потребовалось.");
      } else if (response.outcome === "rejected" || response.outcome === "conflict" || !response.item) {
        setError(response.reason ?? "Не удалось восстановить запись памяти");
      } else {
        if (item.status !== "candidate" || item.pinned !== response.item.pinned) {
          await updateMemory(response.item.id, { status: item.status, pinned: item.pinned });
        }
        setUndoItem(null);
        if (undoTimer.current !== null) {
          window.clearTimeout(undoTimer.current);
          undoTimer.current = null;
        }
        await refresh();
      }
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Не удалось восстановить запись памяти");
    } finally {
      setUndoBusy(false);
    }
  }

  function applyTemplate(templateId: string) {
    const template = MEMORY_TEMPLATES.find((item) => item.id === templateId) ?? MEMORY_TEMPLATES[0];
    setAddTemplate(template.id);
    setAddKind(template.kind);
    setAddContent(template.content);
  }

  async function submitMemory() {
    if (!addContent.trim()) {
      setError("Заполните содержание записи");
      return;
    }
    setAddBusy(true);
    setError(null);
    try {
      const response = await createMemory({
        content: addContent,
        kind: addKind,
        scope: addScope,
        scope_key: addScopeKey,
      });
      if (response.outcome === "duplicate") {
        setError(`Такая память уже существует: ${response.existing_id?.slice(0, 8) ?? "известная запись"}`);
      } else if (response.outcome === "rejected") {
        setError(response.reason ?? "Запись отклонена");
      } else {
        setAddOpen(false);
        setAddContent("");
        setTab(response.outcome === "conflict" ? "conflicts" : "candidates");
        await refresh();
        if (response.outcome === "conflict") {
          setError("Запись добавлена как конфликт — выберите актуальную версию во вкладке конфликтов");
        }
      }
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Не удалось добавить запись памяти");
    } finally {
      setAddBusy(false);
    }
  }

  return (
    <div className="actionsPanel memoryPanel">
      <div className="panelToolbar">
        <div>
          <strong>Память</strong>
          <span>Ручное управление и прозрачность — без обязательных подтверждений</span>
        </div>
        <div className="actionMetrics">
          <span>
            {visibleItems.length}
            {visibleItems.length !== items.length ? ` / ${items.length}` : ""} на вкладке
          </span>
          <button
            type="button"
            onClick={() => void exportMemoryPackFile("json")}
            disabled={loading}
          >
            Экспорт JSON
          </button>
          <button type="button" onClick={() => void exportMemoryPackFile("zip")} disabled={loading}>
            Экспорт pack ZIP
          </button>
          <button type="button" onClick={() => importInputRef.current?.click()} disabled={loading}>
            Импорт pack
          </button>
          <input
            ref={importInputRef}
            type="file"
            accept=".json,.zip,application/json,application/zip"
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0];
              event.target.value = "";
              if (file) void importMemoryPackFile(file);
            }}
          />
          <button type="button" onClick={() => void refresh()} disabled={loading}>
            Обновить
          </button>
          <button type="button" onClick={() => setAddOpen((open) => !open)}>
            {addOpen ? "Закрыть форму" : "Добавить память"}
          </button>
        </div>
      </div>

      {addOpen ? (
        <div className="memoryAddForm">
          <div className="memoryAddHeader">
            <strong>Новая запись памяти</strong>
            <span>Запись пройдёт редактирование, дедупликацию и проверку конфликтов.</span>
          </div>
          <div className="memoryAddFields">
            <label>
              <span>Шаблон</span>
              <select value={addTemplate} onChange={(event) => applyTemplate(event.target.value)}>
                {MEMORY_TEMPLATES.map((template) => (
                  <option value={template.id} key={template.id}>{template.label}</option>
                ))}
              </select>
            </label>
            <label>
              <span>Тип</span>
              <select value={addKind} onChange={(event) => setAddKind(event.target.value)}>
                {MEMORY_TEMPLATES.map((template) => (
                  <option value={template.kind} key={template.kind}>{template.label}</option>
                ))}
              </select>
            </label>
            <label>
              <span>Область</span>
              <select value={addScope} onChange={(event) => setAddScope(event.target.value)}>
                <option value="global">Пользовательская</option>
                <option value="workspace">Workspace</option>
                <option value="project">Проект</option>
                <option value="session">Сессия</option>
              </select>
            </label>
            <label>
              <span>Ключ области</span>
              <input value={addScopeKey} onChange={(event) => setAddScopeKey(event.target.value)} />
            </label>
          </div>
          <textarea
            className="memoryEdit"
            value={addContent}
            onChange={(event) => setAddContent(event.target.value)}
            rows={4}
            placeholder="Например: проект использует native PostgreSQL для локальной разработки"
          />
          <div className="memoryActions">
            <button type="button" disabled={addBusy} onClick={() => void submitMemory()}>
              {addBusy ? "Добавление..." : "Добавить запись"}
            </button>
            <button type="button" disabled={addBusy} onClick={() => setAddOpen(false)}>
              Отмена
            </button>
          </div>
        </div>
      ) : null}

      {privacy ? (
        <div className="memoryPrivacy">
          <strong>Конфиденциальность</strong>
          <p>
            Редактирование: {privacy.redaction_enabled ? "всегда включено" : "выключено"}. {privacy.policy}
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
      {notice ? <div className="panelNotice">{notice}</div> : null}

      {loading ? (
        <div className="emptyPanelState">
          <strong>Загрузка…</strong>
        </div>
      ) : visibleItems.length === 0 ? (
        <div className="emptyPanelState">
          <strong>Записей нет</strong>
          <span>
            {tab === "experiences"
              ? "Пока нет опыта и сценариев — они появятся после задач с паттернами успеха и провала."
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
                            <span className="memoryScopeLabel">{scopeLabel(candidate.scope)}</span>
                          </strong>
                          <span>увер. {candidate.confidence.toFixed(2)}</span>
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
                      {scopeLabel(item.scope)}
                      {item.pinned ? " · закреплено" : ""}
                    </span>
                  </strong>
                  <span>
                    {statusLabel(item.status)} · увер. {item.confidence.toFixed(2)}
                    {typeof item.use_count === "number" ? ` · использовано ${item.use_count}` : ""}
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
                  {item.source_label ? <span title="источник">{item.source_label}</span> : null}
                  {item.supersedes ? (
                    <span title="заменяет">→ {item.supersedes.slice(0, 8)}</span>
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
                    {item.pinned ? "Открепить" : "Закрепить"}
                  </button>
                  {item.status !== "active" ? (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void runAction(item.id, () => updateMemory(item.id, { status: "active" }))}
                    >
                      Активировать
                    </button>
                  ) : null}
                  {item.status !== "rejected" ? (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void runAction(item.id, () => updateMemory(item.id, { status: "rejected" }))}
                    >
                      Отклонить
                    </button>
                  ) : null}
                  {item.status !== "archived" ? (
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void runAction(item.id, () => updateMemory(item.id, { status: "archived" }))}
                    >
                      В архив
                    </button>
                  ) : null}
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => void deleteItem(item)}
                  >
                    Удалить
                  </button>
                </div>
              </article>
            );
          })}
        </div>
      )}

      {nextCursor && !loading ? (
        <div className="memoryLoadMore">
          <button type="button" onClick={() => void loadMore()} disabled={loadingMore}>
            {loadingMore ? "Загрузка..." : "Загрузить ещё"}
          </button>
        </div>
      ) : null}

      {undoItem ? (
        <div className="memoryUndo" role="status">
          <span>Запись удалена. Восстановить?</span>
          <button type="button" onClick={() => void restoreDeletedItem()} disabled={undoBusy}>
            {undoBusy ? "Восстановление..." : "Отменить удаление"}
          </button>
        </div>
      ) : null}
    </div>
  );
}
