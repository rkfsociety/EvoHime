import { useCallback, useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

/**
 * Metadata of one memory record as Core reports it. There is deliberately no
 * `statement` field: `memory.pending` and `memory.conflicts` never carry a
 * body, so the panel cannot leak one even by accident.
 */
interface MemoryMetadata {
  readonly id: string
  readonly kind: string
  readonly canonical_subject: string | null
  readonly confirmation_state: string
  readonly privacy_class: string
  readonly source_trust: string
  readonly model_confidence: number
  readonly verification_confidence: number
  readonly validation_status: string
  readonly policy_version: string
  readonly expires_at_ms: string | null
}

interface MemoryConflict {
  readonly pending: MemoryMetadata
  readonly active: MemoryMetadata
  readonly conflict_key: string
  readonly supersession_chain: readonly string[]
}

interface WorkspaceIndexStatus {
  readonly workspace_key: string
  readonly generation: number | null
  readonly status: string
  readonly indexed_files: number
  readonly chunks: number
  readonly excluded: number
  readonly dirty: boolean
  readonly published_at: number | null
  readonly vector_mode: string
  readonly vector_index_id: string | null
}

interface WorkspaceSearchPayload {
  readonly search: {
    readonly query_id: string
    readonly evidence: readonly { readonly relative_path: string; readonly lines: readonly number[] | null }[]
    readonly diagnostics: { readonly mode: string; readonly coverage: number; readonly stop_reason: string }
    readonly uncertainty: string | null
  }
}

interface ChildTimelineItem {
  readonly child_task_id?: string
  readonly role?: string
  readonly state?: string
  readonly revision?: number
  readonly reason_code?: string | null
  readonly lease_live?: boolean
  readonly dead_letter?: boolean
  readonly parent_sequence?: number
  readonly budget?: { readonly max_tokens?: number; readonly max_time_seconds?: number; readonly max_tool_calls?: number }
}

const KIND_LABELS: Record<string, string> = {
  preference: 'предпочтение',
  constraint: 'ограничение',
  decision: 'решение',
  entity: 'факт',
  lesson: 'урок',
  session_summary: 'сводка сессии'
}

const TRUST_LABELS: Record<string, string> = {
  user: 'сказал пользователь',
  tool_output: 'вывод инструмента',
  document: 'документ',
  model_inference: 'вывод модели'
}

function parsePayload<T>(event: CoreEvent | undefined, key: string): T | null {
  if (!event) return null
  try {
    const parsed = JSON.parse(event.payload) as Record<string, unknown>
    return (parsed[key] as T) ?? null
  } catch {
    return null
  }
}

function latest(events: readonly CoreEvent[], eventType: string): CoreEvent | undefined {
  return events.filter((event) => event.eventType === eventType).at(-1)
}

/** Read-only projection of Core-owned memory/child/schedule state. */
export function OperationsPanel({ connection, events }: Props): React.JSX.Element {
  const api = useShellApi()
  const [workspacePath, setWorkspacePath] = useState<string | null>(null)
  const [selected, setSelected] = useState<readonly string[]>([])
  const [message, setMessage] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [embeddingEnabled, setEmbeddingEnabled] = useState(false)
  const [knowledgeQuery, setKnowledgeQuery] = useState('')

  const connected = CONNECTED_STATES.includes(connection)
  const count = (name: string) => events.filter((event) => event.eventType === name).length
  const childEvents = events.filter((event) => event.eventType.startsWith('child.'))
  const childProjection = useMemo(() => childEvents.map((event) => {
    try { return { event, item: JSON.parse(event.payload) as ChildTimelineItem } } catch { return { event, item: {} as ChildTimelineItem } }
  }), [childEvents])
  const activeChildren = childProjection.filter(({ item }) => item.lease_live === true && !item.dead_letter).length
  const deadLetters = childProjection.filter(({ item }) => item.dead_letter === true).length
  const liveLeases = childProjection.filter(({ item }) => item.lease_live === true).length
  const pulseFailed = count('runtime.schedule_failed') + count('runtime.schedule_dead_letter')

  const pendingEvent = latest(events, 'memory.pending')
  const pending = useMemo(
    () => parsePayload<readonly MemoryMetadata[]>(pendingEvent, 'records') ?? [],
    [pendingEvent]
  )
  const counts = useMemo(
    () => parsePayload<Record<string, number>>(pendingEvent, 'counts') ?? {},
    [pendingEvent]
  )
  const conflicts = useMemo(
    () => parsePayload<readonly MemoryConflict[]>(latest(events, 'memory.conflicts'), 'conflicts') ?? [],
    [events]
  )
  const indexStatus = useMemo(
    () => parsePayload<WorkspaceIndexStatus>(latest(events, 'workspace.index_status'), 'status'),
    [events]
  )
  const searchPayload = useMemo(
    () => parsePayload<WorkspaceSearchPayload['search']>(latest(events, 'workspace.knowledge'), 'search'),
    [events]
  )

  useEffect(() => {
    if (!api) return
    void api.invoke('workspace.list', {}).then((outcome) => {
      if (outcome.ok) setWorkspacePath(outcome.value.selected)
    })
  }, [api])

  const refresh = useCallback(() => {
    if (!api || !connected || !workspacePath) return
    const request = { scopeKind: 'project', projectId: 'workspace', workspacePath, limit: 50 }
    void api.invoke('core.listMemoryPending', request)
    void api.invoke('core.getMemoryConflicts', request)
    void api.invoke('core.getIndexStatus', { workspacePath })
  }, [api, connected, workspacePath])

  const updateIndex = useCallback(async (rebuild: boolean) => {
    if (!api || !workspacePath) return
    setMessage(rebuild ? 'Полная пересборка индекса запущена…' : 'Инкрементальная индексация запущена…')
    const outcome = await api.invoke(rebuild ? 'core.rebuildIndex' : 'core.indexWorkspace', {
      workspacePath,
      enableEmbeddings: embeddingEnabled
    })
    setMessage(outcome.ok ? 'Команда индексации передана Core.' : outcome.message)
  }, [api, embeddingEnabled, workspacePath])

  const searchKnowledge = useCallback(async () => {
    if (!api || !workspacePath || knowledgeQuery.trim().length === 0) return
    const outcome = await api.invoke('core.searchWorkspaceKnowledge', {
      workspacePath,
      query: knowledgeQuery,
      hybrid: embeddingEnabled
    })
    setMessage(outcome.ok ? 'Поиск выполняется в Core.' : outcome.message)
  }, [api, embeddingEnabled, knowledgeQuery, workspacePath])

  useEffect(() => {
    refresh()
  }, [refresh])

  // Confirm and reject are approval-gated on the Core side; the shell only
  // forwards the decision the user just made in this panel.
  const decide = useCallback(
    async (command: 'core.confirmMemory' | 'core.rejectMemory') => {
      if (!api || selected.length === 0) return
      const stamp = `${Date.now()}-${selected.join(',')}`
      const outcome = await api.invoke(command, {
        ids: selected,
        approvalId: `memory-${stamp}`,
        idempotencyKey: `memory-${stamp}`
      })
      setMessage(
        outcome.ok
          ? `Решение отправлено в Core для ${selected.length} записей.`
          : outcome.message
      )
      setSelected([])
      refresh()
    },
    [api, refresh, selected]
  )

  // "Изменить" and "только на эту сессию" share one Core command: neither
  // confirms the record, so both leave it in the queue (or, for a
  // session-only note, out of persistent memory entirely).
  const revise = useCallback(
    async (id: string, statement: string, sessionOnly: boolean) => {
      if (!api) return
      const stamp = `${Date.now()}-${id}`
      const outcome = await api.invoke('core.reviseMemoryCandidate', {
        id,
        statement,
        sessionOnly,
        sessionId: sessionOnly ? `shell-${stamp}` : '',
        approvalId: `memory-${stamp}`,
        idempotencyKey: `memory-${stamp}`
      })
      setMessage(
        outcome.ok
          ? sessionOnly
            ? 'Запись оставлена только на эту сессию и не попадёт в постоянную память.'
            : 'Правка отправлена в Core; запись всё ещё ждёт подтверждения.'
          : outcome.message
      )
      setEditing(null)
      setDraft('')
      refresh()
    },
    [api, refresh]
  )

  const resolveConflict = useCallback(
    async (conflict: MemoryConflict) => {
      if (!api) return
      const stamp = `${Date.now()}-${conflict.pending.id}`
      const outcome = await api.invoke('core.supersedeMemory', {
        oldId: conflict.active.id,
        newId: conflict.pending.id,
        reason: 'user_choice',
        approvalId: `memory-${stamp}`,
        idempotencyKey: `memory-${stamp}`
      })
      setMessage(outcome.ok ? 'Замена записи отправлена в Core.' : outcome.message)
      refresh()
    },
    [api, refresh]
  )

  const toggle = (id: string) =>
    setSelected((current) =>
      current.includes(id) ? current.filter((entry) => entry !== id) : [...current, id]
    )

  return (
    <section className="panel operations-panel" aria-label="Память и автоматизация">
      <div className="panel__header">
        <div>
          <h2>Память и автоматизация</h2>
          <p>Только состояние, подтверждённое Core; локальные события не подменяются успехом.</p>
        </div>
        <span className={`status-pill status-pill--${connection}`}>{connection}</span>
      </div>
      <div className="operations-grid">
        <article className={`operations-card ${pending.length ? 'operations-card--warning' : ''}`}>
          <h3>Память: подтверждение</h3>
          <strong>{counts['pending_confirmation'] ?? 0}</strong>
          <span>ждут решения</span>
          <small>
            {counts['confirmed'] ?? 0} активных · {counts['expired'] ?? 0} истекло ·{' '}
            {counts['rejected'] ?? 0} отклонено
          </small>
        </article>
        <article className={`operations-card ${conflicts.length ? 'operations-card--warning' : ''}`}>
          <h3>Конфликты памяти</h3>
          <strong>{conflicts.length}</strong>
          <span>неразрешённых</span>
          <small>Старая запись остаётся активной, пока выбор не сделан</small>
        </article>
        <article className="operations-card">
          <h3>Child jobs</h3>
          <strong>{activeChildren}</strong>
          <span>активных children</span>
          <small>{liveLeases} leases · {deadLetters} dead-letter · {count('child.report.accepted')} принятых отчётов</small>
        </article>
        <article className={`operations-card ${pulseFailed ? 'operations-card--warning' : ''}`}>
          <h3>Pulse</h3>
          <strong>{pulseFailed ? 'Внимание' : 'OK'}</strong>
          <span>{pulseFailed ? 'есть ошибки расписаний' : 'ошибок не обнаружено'}</span>
          <small>{count('runtime.schedule_completed')} completed · {count('runtime.schedule_requeued')} requeued · {count('runtime.schedule_dead_letter')} dead-letter</small>
        </article>
      </div>

      <section className="operations-timeline" aria-label="Локальный индекс workspace">
        <h3>Локальные знания workspace</h3>
        <p>
          {indexStatus
            ? `${indexStatus.indexed_files} файлов · ${indexStatus.chunks} фрагментов · ${indexStatus.excluded} исключено · поколение ${indexStatus.generation ?? '—'} · ${indexStatus.vector_mode}`
            : 'Состояние индекса ещё не получено.'}
          {indexStatus?.dirty ? ' · индекс требует обновления' : ''}
        </p>
        <div className="operations-actions">
          <label>
            <input
              type="checkbox"
              checked={embeddingEnabled}
              onChange={(input) => setEmbeddingEnabled(input.target.checked)}
            />
            локальные embeddings
          </label>
          <button type="button" disabled={!connected || !workspacePath} onClick={() => void updateIndex(false)}>
            Обновить индекс
          </button>
          <button type="button" disabled={!connected || !workspacePath} onClick={() => void updateIndex(true)}>
            Пересобрать полностью
          </button>
          <button
            type="button"
            disabled={!workspacePath}
            onClick={() => {
              if (api && workspacePath) void api.invoke('core.cancelWorkspaceIndex', { workspacePath })
            }}
          >
            Отменить
          </button>
          <button type="button" disabled={!connected || !workspacePath} onClick={refresh}>
            Обновить статус
          </button>
        </div>
        <div className="operations-actions">
          <input
            type="search"
            aria-label="Поиск по локальному индексу"
            placeholder="Найти symbol, путь или факт"
            value={knowledgeQuery}
            onChange={(input) => setKnowledgeQuery(input.target.value)}
          />
          <button type="button" disabled={!workspacePath || knowledgeQuery.trim().length === 0} onClick={() => void searchKnowledge()}>
            Найти
          </button>
        </div>
        {searchPayload ? (
          <p>
            {searchPayload.evidence.length} источников · coverage {searchPayload.diagnostics.coverage.toFixed(2)} · {searchPayload.diagnostics.mode} · {searchPayload.diagnostics.stop_reason}
            {searchPayload.uncertainty ? ` · ${searchPayload.uncertainty}` : ''}
          </p>
        ) : null}
      </section>

      {message ? <p className="empty-state">{message}</p> : null}

      {pending.length > 0 ? (
        <>
          <ol className="operations-timeline" aria-label="Кандидаты в память">
            {pending.map((record) => (
              <li key={record.id}>
                <label>
                  <input
                    type="checkbox"
                    checked={selected.includes(record.id)}
                    onChange={() => toggle(record.id)}
                  />
                  <code>{KIND_LABELS[record.kind] ?? record.kind}</code>
                </label>
                <span>
                  {record.canonical_subject ?? 'без темы'} ·{' '}
                  {TRUST_LABELS[record.source_trust] ?? record.source_trust} · уверенность{' '}
                  {record.model_confidence.toFixed(2)} · проверка {record.validation_status}
                  {record.privacy_class === 'normal' ? '' : ' · содержимое скрыто'}
                </span>
                <div className="operations-actions">
                  {editing === record.id ? (
                    <>
                      <input
                        type="text"
                        aria-label="Новая формулировка"
                        value={draft}
                        onChange={(input) => setDraft(input.target.value)}
                      />
                      <button
                        type="button"
                        disabled={draft.trim().length === 0}
                        onClick={() => void revise(record.id, draft, false)}
                      >
                        Сохранить правку
                      </button>
                      <button type="button" onClick={() => setEditing(null)}>
                        Отмена
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        type="button"
                        onClick={() => {
                          setEditing(record.id)
                          setDraft('')
                        }}
                      >
                        Изменить
                      </button>
                      <button type="button" onClick={() => void revise(record.id, '', true)}>
                        Только на эту сессию
                      </button>
                    </>
                  )}
                </div>
              </li>
            ))}
          </ol>
          <div className="operations-actions">
            <button type="button" disabled={selected.length === 0} onClick={() => void decide('core.confirmMemory')}>
              Сохранить выбранные
            </button>
            <button type="button" disabled={selected.length === 0} onClick={() => void decide('core.rejectMemory')}>
              Отклонить выбранные
            </button>
          </div>
        </>
      ) : (
        <p className="empty-state">Кандидатов в память нет: Core ничего не ждёт от вас.</p>
      )}

      {conflicts.length > 0 ? (
        <ol className="operations-timeline" aria-label="Конфликты памяти">
          {conflicts.map((conflict) => (
            <li key={conflict.pending.id}>
              <code>{conflict.conflict_key}</code>
              <span>
                активная {conflict.active.id} · цепочка {conflict.supersession_chain.join(' → ')}
              </span>
              <button type="button" onClick={() => void resolveConflict(conflict)}>
                Заменить новой записью
              </button>
            </li>
          ))}
        </ol>
      ) : null}

      {childEvents.length > 0 ? (
        <ol className="operations-timeline" aria-label="Последние child события">
          {childProjection.slice(0, 8).map(({ event, item }) => (
            <li key={`${event.sequenceId}-${event.eventType}`}>
              <code>{item.role ?? 'child'} · {item.state ?? event.eventType}</code>
              <span>{item.child_task_id ?? 'идентификатор скрыт'} · rev {item.revision ?? 0}{item.reason_code ? ` · ${item.reason_code}` : ''}{item.dead_letter ? ' · dead-letter' : ''}</span>
            </li>
          ))}
        </ol>
      ) : <p className="empty-state">Child timeline появится после запуска bounded read-only задачи.</p>}
    </section>
  )
}
