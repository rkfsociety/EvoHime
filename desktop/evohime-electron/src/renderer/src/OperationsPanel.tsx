import { useCallback, useEffect, useMemo, useState } from 'react'

import type { AmbientProposal, AmbientProposalList, ConnectionState, CoreEvent, RepairStatus } from '@shared/api'

import { useShellApi } from './shell-api'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly repair?: RepairStatus | null
}

function RepairCard({ status }: { readonly status: RepairStatus }): React.JSX.Element {
  const api = useShellApi()
  const [message, setMessage] = useState('')
  const active = ['preparing', 'diagnosing', 'committing', 'pushing', 'waiting_ci'].includes(status.phase)

  const command = async (name: 'repair.start' | 'repair.cancel' | 'repair.commit' | 'repair.push' | 'repair.refreshCI', payload: unknown): Promise<void> => {
    if (!api) return
    const outcome = await api.invoke(name, payload as never)
    setMessage(outcome.ok ? outcome.value.summary : outcome.message)
  }

  const retryable = status.phase === 'available' || (status.phase === 'failed' && status.errorCount >= 3)
  const action = retryable
    ? { label: status.phase === 'failed' ? 'Повторить' : 'Починить', name: 'repair.start' as const, payload: { workspacePath: '' }, disabled: false }
    : status.phase === 'ready_to_commit'
      ? { label: 'Применить и закоммитить', name: 'repair.commit' as const, payload: {}, disabled: false }
      : status.phase === 'ready_to_push'
        ? { label: 'Отправить в GitHub', name: 'repair.push' as const, payload: {}, disabled: false }
        : status.phase === 'waiting_ci'
          ? { label: 'Проверить GitHub Actions', name: 'repair.refreshCI' as const, payload: {}, disabled: false }
          : null

  return (
    <article className={`operations-card${status.error ? ' operations-card--warning' : ''}`}>
      <h3>Самоисправление</h3>
      <strong>{status.errorCount}</strong>
      <span>ошибок для анализа</span>
      <small>{status.summary}</small>
      {status.commit ? <small>commit {status.commit.slice(0, 12)} · CI: {status.ciState}</small> : null}
      {status.evidence?.slice(-4).map((entry) => (
        <small key={`${entry.phase}-${entry.atMs}`}>{entry.phase}: {entry.result} · {entry.detail}</small>
      ))}
      {action ? <button type="button" disabled={action.disabled || active} onClick={() => void command(action.name, action.payload)}>{action.label}</button> : null}
      {status.phase === 'ready_to_update' ? <button type="button" onClick={() => void api?.invoke('update.prepare', {})}>Подготовить обновление</button> : null}
      {active ? <button type="button" onClick={() => void command('repair.cancel', {})}>Остановить</button> : null}
      {message ? <small>{message}</small> : null}
    </article>
  )
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

interface RetainedChildProjection {
  readonly child_id?: string
  readonly role?: string
  readonly stable_name?: string
  readonly lifecycle?: string
  readonly revision?: number
  readonly registry_version?: number
  readonly last_active_at_ms?: number
  readonly retained_until_ms?: number
  readonly pending_count?: number
  readonly invalidation_reason?: string
  readonly last_delivery_outcome?: string
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
  model_inference: 'вывод модели',
  ambient: 'услышано'
}

const PROPOSAL_KIND_LABELS: Record<string, string> = {
  suggestion: 'предложенная задача',
  reminder: 'напоминание'
}

/** Источник кандидата в фильтре очереди. */
type SourceFilter = 'all' | 'ambient' | 'dialog'

const SOURCE_FILTERS: readonly { readonly value: SourceFilter; readonly label: string }[] = [
  { value: 'all', label: 'Все источники' },
  { value: 'dialog', label: 'Из диалога' },
  { value: 'ambient', label: 'Услышано' }
]

function parsePayload<T>(event: CoreEvent | undefined, key: string): T | null {
  if (!event) return null
  try {
    const parsed = JSON.parse(event.payload) as Record<string, unknown>
    return (parsed[key] as T) ?? null
  } catch {
    return null
  }
}

// `events` holds the newest event first (App.tsx prepends on receipt), so
// the latest match is the FIRST one found here — not the last.
function latest(events: readonly CoreEvent[], eventType: string): CoreEvent | undefined {
  return events.find((event) => event.eventType === eventType)
}

/** Read-only projection of Core-owned memory/child/schedule state. */
export function OperationsPanel({ connection, events, repair }: Props): React.JSX.Element {
  const api = useShellApi()
  const [workspacePath, setWorkspacePath] = useState<string | null>(null)
  const [selected, setSelected] = useState<readonly string[]>([])
  const [message, setMessage] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [embeddingEnabled, setEmbeddingEnabled] = useState(false)
  const [sourceFilter, setSourceFilter] = useState<SourceFilter>('all')
  const [proposals, setProposals] = useState<AmbientProposalList | null>(null)
  const [deciding, setDeciding] = useState<string | null>(null)
  const [knowledgeQuery, setKnowledgeQuery] = useState('')

  const connected = CONNECTED_STATES.includes(connection)
  const eventSummary = useMemo(() => {
    const counts = new Map<string, number>()
    const childProjection: { readonly event: CoreEvent; readonly item: ChildTimelineItem }[] = []
    let activeChildren = 0
    let deadLetters = 0
    let liveLeases = 0
    const retainedChildren: RetainedChildProjection[] = []
    for (const event of events) {
      counts.set(event.eventType, (counts.get(event.eventType) ?? 0) + 1)
      if (event.eventType === 'retained_child' || event.eventType === 'retained_child.list') {
        try {
          const payload = JSON.parse(event.payload) as { children?: RetainedChildProjection[] }
          if (payload.children) retainedChildren.push(...payload.children)
          else retainedChildren.push(payload as RetainedChildProjection)
        } catch { /* malformed Core payload is ignored, never rendered as authority */ }
      }
      if (!event.eventType.startsWith('child.')) continue
      let item: ChildTimelineItem
      try { item = JSON.parse(event.payload) as ChildTimelineItem } catch { item = {} }
      childProjection.push({ event, item })
      if (item.lease_live === true) liveLeases += 1
      if (item.dead_letter === true) deadLetters += 1
      if (item.lease_live === true && item.dead_letter !== true) activeChildren += 1
    }
    return { counts, childProjection, retainedChildren, activeChildren, deadLetters, liveLeases }
  }, [events])
  const count = (name: string): number => eventSummary.counts.get(name) ?? 0
  const { childProjection, retainedChildren, activeChildren, deadLetters, liveLeases } = eventSummary
  const pulseFailed = count('runtime.schedule_failed') + count('runtime.schedule_dead_letter')
  const toolCalls = count('tool.started')
  const toolOutputs = count('tool.output')
  const approvalRequests = count('approval.required')

  const pendingEvent = latest(events, 'memory.pending')
  const pending = useMemo(
    () => parsePayload<readonly MemoryMetadata[]>(pendingEvent, 'records') ?? [],
    [pendingEvent]
  )
  const counts = useMemo(
    () => parsePayload<Record<string, number>>(pendingEvent, 'counts') ?? {},
    [pendingEvent]
  )
  // Фильтр только скрывает строки: решение всё равно принимает пользователь
  // по каждой записи, и скрытая строка не может быть подтверждена вслепую —
  // выбор с неё снимается вместе с ней.
  const visiblePending = useMemo(
    () =>
      pending.filter((record) =>
        sourceFilter === 'all'
          ? true
          : sourceFilter === 'ambient'
            ? record.source_trust === 'ambient'
            : record.source_trust !== 'ambient'
      ),
    [pending, sourceFilter]
  )
  const ambientCount = useMemo(
    () => pending.filter((record) => record.source_trust === 'ambient').length,
    [pending]
  )
  // Показываются только ждущие решения карточки. Решённое и просроченное ядро
  // и так не отдаёт, но полагаться на это молча нельзя.
  const openProposals = useMemo(
    () => (proposals?.proposals ?? []).filter((proposal) => proposal.state === 'proposed'),
    [proposals]
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

  useEffect(() => {
    if (!api || !connected) return
    void api.invoke('core.listRetainedChildren', { limit: 16 })
  }, [api, connected])

  const deleteRetainedChild = useCallback(async (child: RetainedChildProjection): Promise<void> => {
    if (!api || !child.child_id || child.registry_version === undefined) return
    const outcome = await api.invoke('core.deleteRetainedChild', {
      childId: child.child_id,
      expectedRegistryVersion: child.registry_version
    })
    setMessage(outcome.ok ? `Сохранённый child ${child.child_id} удалён.` : outcome.message)
    if (outcome.ok) void api.invoke('core.listRetainedChildren', { limit: 16 })
  }, [api])

  // Предложения не привязаны к воркспейсу: речь у стола не принадлежит
  // рабочему каталогу, поэтому список запрашивается отдельно от очереди
  // памяти и не ждёт выбранной папки.
  const refreshProposals = useCallback(async () => {
    if (!api || !connected) return
    const outcome = await api.invoke('ambient.listProposals', { limit: 50 })
    if (!outcome.ok) setMessage(outcome.message)
  }, [api, connected])

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

  const proposalListEvent = latest(events, 'ambient.proposals')
  useEffect(() => {
    if (!proposalListEvent) return
    try {
      setProposals(JSON.parse(proposalListEvent.payload) as AmbientProposalList)
    } catch {
      setProposals(null)
    }
  }, [proposalListEvent])

  // Каждая durable-запись `ambient.proposal` — сигнал «список изменился», а не
  // сам список: текста карточки в ней нет, поэтому её нельзя отрисовать, но по
  // ней можно перечитать.
  const proposalSignal = events.filter((event) => event.eventType === 'ambient.proposal').length
  useEffect(() => {
    void refreshProposals()
  }, [refreshProposals, proposalSignal])

  useEffect(() => {
    const visible = new Set(visiblePending.map((record) => record.id))
    setSelected((current) => {
      const kept = current.filter((id) => visible.has(id))
      return kept.length === current.length ? current : kept
    })
  }, [visiblePending])

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

  // Решение по карточке. Ключ идемпотентности считается один раз на карточку и
  // на решение: повторный клик по той же кнопке возвращает первое решение, а
  // не создаёт вторую задачу.
  const decideProposal = useCallback(
    async (proposal: AmbientProposal, choice: 'accept' | 'decline' | 'mute') => {
      if (!api || deciding !== null) return
      setDeciding(proposal.proposal_id)
      const outcome = await api.invoke('ambient.resolveProposal', {
        proposalId: proposal.proposal_id,
        accepted: choice === 'accept',
        mute: choice === 'mute',
        idempotencyKey: `proposal-${proposal.proposal_id}-${choice}`
      })
      setMessage(
        outcome.ok
          ? choice === 'accept'
            ? 'Решение отправлено в Core: запись появится в списке задач.'
            : choice === 'mute'
              ? 'Больше не предлагать такое: решение отправлено в Core.'
              : 'Предложение отклонено.'
          : outcome.message
      )
      setDeciding(null)
      await refreshProposals()
    },
    [api, deciding, refreshProposals]
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
        {repair ? <RepairCard status={repair} /> : null}
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
        <article className={`operations-card ${toolCalls !== toolOutputs ? 'operations-card--warning' : ''}`}>
          <h3>Инструменты</h3>
          <strong>{toolCalls}</strong>
          <span>вызовов в текущем replay</span>
          <small>{toolOutputs} результатов · {approvalRequests} запросов approval</small>
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

      <section className="operations-timeline" aria-label="Предложения по услышанному">
        <h3>Предложения по услышанному</h3>
        <p>
          Ева может предложить, но не может сделать. Любое из этих действий выполняется только
          твоим кликом.
          {proposals
            ? ` Потолок: не больше ${proposals.max_per_hour} в час и ${proposals.max_per_day} в сутки.`
            : ''}
        </p>
        {openProposals.length === 0 ? (
          <p className="empty-state">Предложений нет: Ева ничего не предлагает.</p>
        ) : (
          <ol className="operations-timeline" aria-label="Карточки предложений">
            {openProposals.map((proposal) => (
              <li key={proposal.proposal_id}>
                <code>{PROPOSAL_KIND_LABELS[proposal.kind] ?? proposal.kind}</code>
                <span className="operations-badge operations-badge--ambient">услышано</span>
                <span>
                  {proposal.title}
                  {proposal.occurrences > 1 ? ` · упомянуто ${proposal.occurrences} раза` : ''}
                  {proposal.source_episode_id ? '' : ' · источник удалён'}
                </span>
                <div className="operations-actions">
                  <button
                    type="button"
                    disabled={deciding !== null}
                    onClick={() => void decideProposal(proposal, 'accept')}
                  >
                    {proposal.kind === 'reminder' ? 'Напомнить' : 'Создать задачу'}
                  </button>
                  <button
                    type="button"
                    disabled={deciding !== null}
                    onClick={() => void decideProposal(proposal, 'decline')}
                  >
                    Не надо
                  </button>
                  <button
                    type="button"
                    disabled={deciding !== null}
                    onClick={() => void decideProposal(proposal, 'mute')}
                  >
                    Больше не предлагать такое
                  </button>
                </div>
              </li>
            ))}
          </ol>
        )}
      </section>

      {pending.length > 0 ? (
        <>
          <div className="operations-actions">
            <label htmlFor="memory-source-filter">Источник</label>
            <select
              id="memory-source-filter"
              value={sourceFilter}
              onChange={(input) => setSourceFilter(input.target.value as SourceFilter)}
            >
              {SOURCE_FILTERS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
            <small>услышано: {ambientCount} из {pending.length}</small>
          </div>
          <ol className="operations-timeline" aria-label="Кандидаты в память">
            {visiblePending.map((record) => (
              <li key={record.id}>
                <label>
                  <input
                    type="checkbox"
                    checked={selected.includes(record.id)}
                    onChange={() => toggle(record.id)}
                  />
                  <code>{KIND_LABELS[record.kind] ?? record.kind}</code>
                </label>
                {record.source_trust === 'ambient' ? (
                  <span className="operations-badge operations-badge--ambient">услышано</span>
                ) : null}
                <span>
                  {record.canonical_subject ?? 'без темы'} ·{' '}
                  {TRUST_LABELS[record.source_trust] ?? record.source_trust} · уверенность{' '}
                  {record.model_confidence.toFixed(2)} · проверка {record.validation_status}
                  {record.privacy_class === 'normal' ? '' : ' · содержимое скрыто'}
                  {record.source_trust === 'ambient' ? ' · говорящий не подтверждён' : ''}
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

      {childProjection.length > 0 ? (
        <ol className="operations-timeline" aria-label="Последние child события">
          {childProjection.slice(0, 8).map(({ event, item }) => (
            <li key={`${event.sequenceId}-${event.eventType}`}>
              <code>{item.role ?? 'child'} · {item.state ?? event.eventType}</code>
              <span>{item.child_task_id ?? 'идентификатор скрыт'} · rev {item.revision ?? 0}{item.reason_code ? ` · ${item.reason_code}` : ''}{item.dead_letter ? ' · dead-letter' : ''}</span>
            </li>
          ))}
        </ol>
      ) : <p className="empty-state">Child timeline появится после запуска bounded read-only задачи.</p>}

      {retainedChildren.length > 0 ? (
        <ol className="operations-timeline" aria-label="Сохранённые child контексты">
          {retainedChildren.slice(0, 16).map((child, index) => (
            <li key={`${child.child_id ?? 'child'}-${index}`}>
              <code>{child.stable_name || child.child_id || 'child'} · {child.role || 'role'}</code>
              <span>{child.lifecycle || 'unknown'} · rev {child.revision ?? 0} · pending {child.pending_count ?? 0}{child.last_delivery_outcome ? ` · ${child.last_delivery_outcome}` : ''}{child.invalidation_reason ? ` · ${child.invalidation_reason}` : ''}</span>
              {child.lifecycle !== 'deleted' && child.child_id && child.registry_version !== undefined ? <button type="button" onClick={() => void deleteRetainedChild(child)}>Удалить контекст</button> : null}
            </li>
          ))}
        </ol>
      ) : null}
    </section>
  )
}
