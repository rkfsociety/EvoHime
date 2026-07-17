import { useCallback, useEffect, useMemo, useState } from "react";
import {
  deleteMemory,
  listMemory,
  updateMemory,
  type MemoryItem,
  type MemoryPrivacyInfo,
} from "../api/memory";

type MemoryTab = "active" | "candidates" | "experiences" | "conflicts" | "archived";

const TABS: Array<{ id: MemoryTab; label: string; status: string }> = [
  { id: "active", label: "Active", status: "active" },
  { id: "candidates", label: "Candidates", status: "candidate" },
  { id: "experiences", label: "Experiences", status: "experiences" },
  { id: "conflicts", label: "Conflicts", status: "conflict" },
  { id: "archived", label: "Archived", status: "archived" },
];

export function MemoryPanel() {
  const [tab, setTab] = useState<MemoryTab>("active");
  const [items, setItems] = useState<MemoryItem[]>([]);
  const [privacy, setPrivacy] = useState<MemoryPrivacyInfo | null>(null);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const status = TABS.find((item) => item.id === tab)?.status ?? "active";
      const response = await listMemory({ status, q: query.trim() || undefined, limit: 150 });
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

  const counts = useMemo(() => ({ total: items.length }), [items.length]);

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

  return (
    <div className="actionsPanel memoryPanel">
      <div className="panelToolbar">
        <div>
          <strong>Память</strong>
          <span>Override и прозрачность — не очередь обязательных approve</span>
        </div>
        <div className="actionMetrics">
          <span>{counts.total} на вкладке</span>
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

      <div className="panelToolbar">
        <input
          className="memorySearch"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Поиск по содержимому…"
        />
      </div>

      {error ? <div className="panelError">{error}</div> : null}

      {loading ? (
        <div className="emptyPanelState">
          <strong>Загрузка…</strong>
        </div>
      ) : items.length === 0 ? (
        <div className="emptyPanelState">
          <strong>Записей нет</strong>
          <span>На этой вкладке пока пусто — агент накопит факты и опыт сам.</span>
        </div>
      ) : (
        <div className="memoryList">
          {items.map((item) => {
            const busy = busyId === item.id;
            const editing = editingId === item.id;
            return (
              <article className="memoryItem" key={item.id} data-pinned={item.pinned}>
                <div className="memoryItemHeader">
                  <strong>
                    {item.scope}/{item.kind}
                    {item.pinned ? " · pinned" : ""}
                  </strong>
                  <span>
                    {item.status} · conf {item.confidence.toFixed(2)}
                  </span>
                </div>
                {editing ? (
                  <textarea
                    className="memoryEdit"
                    value={draft}
                    onChange={(event) => setDraft(event.target.value)}
                    rows={4}
                  />
                ) : (
                  <p>{item.content}</p>
                )}
                <div className="memoryMeta">
                  <code>{item.scope_key}</code>
                  <time dateTime={item.updated_at}>{item.updated_at}</time>
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
