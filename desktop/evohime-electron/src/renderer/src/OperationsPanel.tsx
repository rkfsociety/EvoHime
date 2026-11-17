import { translate } from './i18n'
import { useCallback, useEffect, useMemo, useState } from 'react'

import type { AmbientProposal, AmbientProposalList, ChatProviderMode, ConnectionState, CoreEvent, RepairStatus } from '@shared/api'

import { useShellApi } from './shell-api'
import { ModelPicker } from './ModelPicker'
import { ImageGenerationPanel } from './ImageGenerationPanel'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly repair?: RepairStatus | null
}

const REPAIR_PROVIDER_LABELS: Record<ChatProviderMode, string> = {
  literouter: 'LiteRouter',
  openai_compatible: 'OpenAI API',
  openai_responses: 'OpenAI Responses',
  ollama: 'Ollama',
  codex_cli: 'Codex CLI'
}

const REPAIR_SUMMARY_LIMIT = 220
const REPAIR_EVIDENCE_LIMIT = 140

function boundedDiagnostic(value: string, limit = REPAIR_SUMMARY_LIMIT): string {
  if (value.length <= limit) return value
  return `${value.slice(0, limit).trimEnd()}…`
}

function RepairCard({ status, connection, events }: { readonly status: RepairStatus; readonly connection: ConnectionState; readonly events: readonly CoreEvent[] }): React.JSX.Element {
  const api = useShellApi()
  const [message, setMessage] = useState('')
  const provider: ChatProviderMode = 'codex_cli'
  const [model, setModel] = useState('')
  const active = ['preparing', 'diagnosing', 'committing', 'pushing', 'waiting_ci'].includes(status.phase)
  const connected = CONNECTED_STATES.includes(connection)
  const selectionReady = connected && model.trim().length > 0

  const command = async (name: 'repair.start' | 'repair.cancel' | 'repair.commit' | 'repair.push' | 'repair.refreshCI', payload: unknown): Promise<void> => {
    if (!api) return
    const outcome = await api.invoke(name, payload as never)
    setMessage(outcome.ok ? outcome.value.summary : outcome.message)
  }

  const retryable = status.phase === 'available' || (status.phase === 'failed' && status.errorCount >= 3)
  const action = retryable
    ? { label: status.phase === 'failed' ? 'Повторить' : 'Починить', name: 'repair.start' as const, payload: { workspacePath: '', provider, model }, disabled: !selectionReady }
    : status.phase === 'ready_to_commit'
      ? { label: 'Применить и закоммитить', name: 'repair.commit' as const, payload: {}, disabled: false }
      : status.phase === 'ready_to_push'
        ? { label: 'Отправить в GitHub', name: 'repair.push' as const, payload: {}, disabled: false }
        : status.phase === 'waiting_ci'
          ? { label: 'Проверить GitHub Actions', name: 'repair.refreshCI' as const, payload: {}, disabled: false }
          : null

  return (
    <article className={`operations-card operations-card--repair${status.error ? ' operations-card--warning' : ''}`}>
      <div className="operations-card__topline">
        <span className="operations-card__eyebrow">Repair queue</span>
        <span className="operations-card__phase">{translate(status.phase)}</span>
      </div>
      <h3>{translate("Самоисправление")}</h3>
      <div className="operations-card__metric">
        <strong>{translate(status.errorCount)}</strong>
        <span>{translate("ошибок для анализа")}</span>
      </div>
      {status.summary ? (
        <div className="operations-card__diagnostic">
          <small>{translate(boundedDiagnostic(status.summary))}</small>
          {status.summary.length > REPAIR_SUMMARY_LIMIT ? (
            <details>
              <summary>{translate("Показать полный отчёт")}</summary>
              <pre>{translate(status.summary)}</pre>
            </details>
          ) : null}
        </div>
      ) : null}
      {status.error ? <small className="operations-card__error">{translate(boundedDiagnostic(status.error))}</small> : null}
      <div className="repair-selection" aria-label={translate("Провайдер и модель самоисправления")}>
        <span className="repair-selection__title">{translate("Чем анализировать")}</span>
        <div className="repair-selection__controls">
          <select aria-label={translate("Провайдер самоисправления")} value={provider} disabled>
            <option value="codex_cli">Codex CLI</option>
          </select>
          <ModelPicker
            connection={connection}
            events={events}
            provider={provider}
            use="agent"
            onModelChange={setModel}
            disabled={active}
          />
        </div>
        {model ? <small>{translate("Выбрано:")} {translate(REPAIR_PROVIDER_LABELS[provider])} · {translate(model)}</small> : <small>{translate("Выбери доступную модель — без неё запуск запрещён.")}</small>}
      </div>
      {status.provider && status.model ? <small>{translate("Последний run:")} {translate(REPAIR_PROVIDER_LABELS[status.provider])} · {translate(status.model)}</small> : null}
      {status.commit ? <small>commit {translate(status.commit.slice(0, 12))} · CI: {translate(status.ciState)}</small> : null}
      {status.evidence && status.evidence.length > 0 ? (
        <div className="operations-card__evidence">
          {status.evidence.slice(-4).map((entry) => (
            <small key={`${entry.phase}-${entry.atMs}`} title={entry.detail}>
              {translate(entry.phase)}: {translate(entry.result)} · {translate(boundedDiagnostic(entry.detail, REPAIR_EVIDENCE_LIMIT))}
            </small>
          ))}
        </div>
      ) : null}
      <div className="operations-card__actions">
        {action ? <button type="button" disabled={action.disabled || active} onClick={() => void command(action.name, action.payload)}>{translate(action.label)}</button> : null}
        {status.phase === 'ready_to_update' ? <button type="button" onClick={() => void api?.invoke('update.prepare', {})}>{translate("Подготовить обновление")}</button> : null}
        {active ? <button type="button" onClick={() => void command('repair.cancel', {})}>{translate("Остановить")}</button> : null}
      </div>
      {message ? <small>{translate(message)}</small> : null}
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
  readonly authority?: string
  readonly durability?: string
  readonly confidence?: number
  readonly expires_at_ms: string | null
}

interface MemoryConflict {
  readonly pending: MemoryMetadata
  readonly active: MemoryMetadata
  readonly conflict_key: string
  readonly supersession_chain: readonly string[]
}

interface MemoryExtractionDiagnostic {
  readonly stage: 'source' | 'extractor' | 'candidate' | 'finalization' | 'recovery'
  readonly status: 'attempted' | 'skipped' | 'captured' | 'rejected' | 'duplicate' | 'conflict' | 'superseded' | 'deferred' | 'recovered' | 'committed' | 'stale' | 'failed'
  readonly origin: 'dialog' | 'ambient' | 'recovery'
  readonly reason_code: string | null
  readonly source_id: string | null
  readonly backlog: number
  readonly conflict_count: number
  readonly suppressed_reentry_count: number
}

const MEMORY_EXTRACTION_STATUS_LABELS: Record<MemoryExtractionDiagnostic['status'], string> = {
  attempted: 'выполняется',
  skipped: 'пропущено',
  captured: 'источник сохранён',
  rejected: 'кандидат отклонён',
  duplicate: 'дубликат',
  conflict: 'конфликт',
  superseded: 'заменено новой версией',
  deferred: 'отложено',
  recovered: 'восстановлено',
  committed: 'зафиксировано',
  stale: 'источник устарел',
  failed: 'ошибка'
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
  readonly depth?: number
  readonly budget?: { readonly max_tokens?: number; readonly max_time_seconds?: number; readonly max_tool_calls?: number }
  readonly report_status?: string | null
  readonly report_summary?: string | null
  readonly spawned_subagents?: number
  readonly planned_ceiling?: number
  readonly extensions_granted?: number
  readonly lazy_waiting?: number
  readonly in_flight_subagents?: number
  readonly extension_requested?: boolean
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

/** Normalized, bounded view of Core's `hive_state.snapshot` payload. */
interface HiveStateView {
  readonly queen: { readonly status: string; readonly name: string }
  readonly metrics: {
    readonly totalAgentsOnline: number
    readonly activeTasks: number
    readonly averageTaskTimeSec: number
    readonly efficiencyPercent: number
  }
  readonly departments: readonly {
    readonly role: string
    readonly activeCount: number
    readonly maxCapacity: number
  }[]
  readonly activeAgents: readonly {
    readonly id: string
    readonly role: string
    readonly currentAction: string
    readonly depth: number
    readonly spawnedSubagents: number
    readonly ceiling: number
  }[]
  readonly taskFlow: { readonly processing: number; readonly analyzing: number; readonly generatingPlan: number }
  readonly limits: { readonly maxSubagentsPerWorker: number; readonly maxParallelSubagents: number }
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

function parseMemoryExtractionDiagnostic(event: CoreEvent): MemoryExtractionDiagnostic | null {
  let parsed: unknown
  try {
    parsed = JSON.parse(event.payload)
  } catch {
    return null
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return null
  const payload = (parsed as Record<string, unknown>)['MemoryExtractionDiagnostic']
  if (payload === null || typeof payload !== 'object' || Array.isArray(payload)) return null
  const value = payload as Record<string, unknown>
  const stages: readonly string[] = ['source', 'extractor', 'candidate', 'finalization', 'recovery']
  const statuses: readonly string[] = ['attempted', 'skipped', 'captured', 'rejected', 'duplicate', 'conflict', 'superseded', 'deferred', 'recovered', 'committed', 'stale', 'failed']
  const origins: readonly string[] = ['dialog', 'ambient', 'recovery']
  const counter = (key: string): number | null => {
    const count = value[key]
    return typeof count === 'number' && Number.isSafeInteger(count) && count >= 0 && count <= 1_000_000
      ? count
      : null
  }
  const backlog = counter('backlog')
  const conflictCount = counter('conflict_count')
  const suppressedReentryCount = counter('suppressed_reentry_count')
  if (
    typeof value['stage'] !== 'string' || !stages.includes(value['stage']) ||
    typeof value['status'] !== 'string' || !statuses.includes(value['status']) ||
    typeof value['origin'] !== 'string' || !origins.includes(value['origin']) ||
    backlog === null || conflictCount === null || suppressedReentryCount === null
  ) return null
  const reason = value['reason_code']
  const sourceId = value['source_id']
  if (reason !== null && (typeof reason !== 'string' || reason.length > 64 || !/^[a-z0-9_]+$/.test(reason))) return null
  if (sourceId !== null && (typeof sourceId !== 'string' || sourceId.length > 128 || !/^[a-zA-Z0-9:_-]+$/.test(sourceId))) return null
  return {
    stage: value['stage'] as MemoryExtractionDiagnostic['stage'],
    status: value['status'] as MemoryExtractionDiagnostic['status'],
    origin: value['origin'] as MemoryExtractionDiagnostic['origin'],
    reason_code: reason as string | null,
    source_id: sourceId as string | null,
    backlog,
    conflict_count: conflictCount,
    suppressed_reentry_count: suppressedReentryCount
  }
}

/**
 * Parses Core's hive snapshot defensively.
 *
 * The payload crosses IPC from Core, but the renderer still clamps every number
 * and string: a malformed or oversized projection must render as safe defaults
 * instead of escaping into the DOM.
 */
function parseHiveState(event: CoreEvent | undefined): HiveStateView | null {
  if (!event) return null
  let parsed: unknown
  try {
    parsed = JSON.parse(event.payload)
  } catch {
    return null
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return null
  const root = parsed as Record<string, unknown>
  const record = (value: unknown): Record<string, unknown> =>
    value !== null && typeof value === 'object' && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {}
  const text = (value: unknown, max = 128): string =>
    typeof value === 'string' ? value.slice(0, max) : ''
  const count = (value: unknown, max: number): number =>
    typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? Math.min(value, max) : 0
  const list = (value: unknown, max: number): readonly Record<string, unknown>[] =>
    Array.isArray(value)
      ? value.filter((item) => item !== null && typeof item === 'object' && !Array.isArray(item)).slice(0, max) as Record<string, unknown>[]
      : []
  const queen = record(root['queen'])
  const metrics = record(root['metrics'])
  const taskFlow = record(root['task_flow'])
  const limits = record(root['limits'])
  return {
    queen: { status: text(queen['status'], 32), name: text(queen['name'], 64) },
    metrics: {
      totalAgentsOnline: count(metrics['total_agents_online'], 65_535),
      activeTasks: count(metrics['active_tasks'], 65_535),
      averageTaskTimeSec: count(metrics['average_task_time_sec'], 86_400),
      efficiencyPercent: count(metrics['efficiency_percent'], 100)
    },
    departments: list(root['departments'], 16).map((item) => ({
      role: text(item['role']),
      activeCount: count(item['active_count'], 65_535),
      maxCapacity: count(item['max_capacity'], 65_535)
    })),
    activeAgents: list(root['active_agents'], 64).map((item) => ({
      id: text(item['id']),
      role: text(item['role']),
      currentAction: text(item['current_action'], 64),
      depth: count(item['depth'], 255),
      spawnedSubagents: count(item['spawned_subagents'], 65_535),
      ceiling: count(item['ceiling'], 65_535)
    })),
    taskFlow: {
      processing: count(taskFlow['processing'], 255),
      analyzing: count(taskFlow['analyzing'], 255),
      generatingPlan: count(taskFlow['generating_plan'], 255)
    },
    limits: {
      maxSubagentsPerWorker: count(limits['max_subagents_per_worker'], 65_535),
      maxParallelSubagents: count(limits['max_parallel_subagents'], 65_535)
    }
  }
}

/** Percentage of a hive counter against its ceiling, clamped to 0–100. */
function hivePercent(value: number, ceiling: number): number {
  if (ceiling <= 0) return 0
  return Math.max(0, Math.min(100, Math.round((value / ceiling) * 100)))
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
  const projectionReady = connection === 'connected'
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

  const hiveEvent = latest(events, 'hive_state.snapshot')
  const hive = useMemo(() => parseHiveState(hiveEvent), [hiveEvent])

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
  const extractionDiagnostics = useMemo(
    () => events
      .filter((event) => event.eventType === 'memory.extraction')
      .map(parseMemoryExtractionDiagnostic)
      .filter((value): value is MemoryExtractionDiagnostic => value !== null),
    [events]
  )
  const latestExtraction = extractionDiagnostics[0] ?? null
  const observedReentries = extractionDiagnostics.reduce(
    (total, item) => total + item.suppressed_reentry_count,
    0
  )
  const lastExtractionFailure = extractionDiagnostics.find((item) => item.status === 'failed') ?? null
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
    void api.invoke('core.listRefinementCandidates', { ownerScope: 'workspace', limit: 32 })
  }, [api, connected])

  // Snapshot polling is read-only and bounded by the fixed interval. Core
  // answers with `hive_state.snapshot`, which the event list already carries.
  useEffect(() => {
    if (!api || !connected) return
    const request = (): void => {
      void api.invoke('core.getHiveState', {})
    }
    request()
    const timer = window.setInterval(request, 2_000)
    return () => window.clearInterval(timer)
  }, [api, connected])

  const refinementList = useMemo(
    () => latest(events, 'refinement.list')?.refinementList?.candidates ?? [],
    [events]
  )

  const refinementAction = useCallback(async (candidateId: string, revision: number, version: number, action: 'approve' | 'reject' | 'activate' | 'rollback') => {
    if (!api) return
    const outcome = await api.invoke('core.refinementAction', {
      candidateId,
      revision,
      expectedVersion: version,
      action,
      approvalToken: action === 'activate' ? `user-${Date.now()}` : '',
      idempotencyKey: `refinement-${candidateId}-${revision}-${action}-${version}`
    })
    setMessage(outcome.ok ? `Действие refinement «${action}» отправлено в Core.` : outcome.message)
    if (outcome.ok) void api.invoke('core.listRefinementCandidates', { ownerScope: 'workspace', limit: 32 })
  }, [api])

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
    <section className="panel operations-panel" aria-label={translate("Память и автоматизация")}>
      <div className="panel__header operations-panel__header">
        <div>
          <p className="panel__eyebrow">Operations / Core state</p>
          <h2>{translate("Память и автоматизация")}</h2>
          <p>{translate("Только состояние, подтверждённое Core; локальные события не подменяются успехом.")}</p>
        </div>
        <span className={`status-pill status-pill--${connection}`}>{translate(connection)}</span>
      </div>
      <div className="operations-grid operations-grid--featured">
        {repair ? <RepairCard status={repair} connection={connection} events={events} /> : null}
        <ImageGenerationPanel connection={connection} events={events} />
      </div>
      <div className="operations-grid operations-grid--metrics">
        <article className={`operations-card ${projectionReady && pending.length ? 'operations-card--warning' : ''}`}>
          <div className="operations-card__topline">
            <span className="operations-card__eyebrow">Memory queue</span>
            <span className="operations-card__phase">{translate(projectionReady ? (pending.length ? 'pending' : 'clear') : 'offline')}</span>
          </div>
          <h3>{translate("Память: подтверждение")}</h3>
          <div className="operations-card__metric">
            <strong>{translate(projectionReady ? (counts['pending_confirmation'] ?? 0) : '—')}</strong>
            <span>{translate(projectionReady ? translate("ждут решения") : translate("состояние не подтверждено"))}</span>
          </div>
          <small>
            {translate(projectionReady
              ? `${counts['confirmed'] ?? 0} активных · ${counts['expired'] ?? 0} истекло · ${counts['rejected'] ?? 0} отклонено`
              : translate("Core недоступен — ожидается актуальная проекция"))}
          </small>
        </article>
        <article className={`operations-card ${projectionReady && conflicts.length ? 'operations-card--warning' : ''}`}>
          <div className="operations-card__topline">
            <span className="operations-card__eyebrow">Conflicts</span>
            <span className="operations-card__phase">{translate(projectionReady ? (conflicts.length ? 'conflict' : 'clear') : 'offline')}</span>
          </div>
          <h3>{translate("Конфликты памяти")}</h3>
          <div className="operations-card__metric">
            <strong>{translate(projectionReady ? conflicts.length : '—')}</strong>
            <span>{translate(projectionReady ? translate("неразрешённых") : translate("состояние не подтверждено"))}</span>
          </div>
          <small>{translate(projectionReady ? translate("Старая запись остаётся активной, пока выбор не сделан") : translate("Core недоступен — ожидается актуальная проекция"))}</small>
        </article>
        <article className={`operations-card ${latestExtraction && ['failed', 'deferred', 'conflict', 'stale'].includes(latestExtraction.status) ? 'operations-card--warning' : ''}`}>
          <div className="operations-card__topline">
            <span className="operations-card__eyebrow">Extraction</span>
            <span className="operations-card__phase">{translate(projectionReady ? (latestExtraction?.status ?? 'idle') : 'offline')}</span>
          </div>
          <h3>{translate("Извлечение памяти")}</h3>
          <div className="operations-card__metric">
            <strong>{translate(projectionReady ? (latestExtraction ? MEMORY_EXTRACTION_STATUS_LABELS[latestExtraction.status] : '—') : '—')}</strong>
            <span>{translate(projectionReady ? (latestExtraction ? `${latestExtraction.stage} · ${latestExtraction.origin}` : translate("состояние ещё не получено")) : translate("состояние не подтверждено"))}</span>
          </div>
          <small>{translate(projectionReady && latestExtraction ? `${latestExtraction.backlog} в очереди восстановления · ${latestExtraction.conflict_count} конфликтов в последнем проходе · ${observedReentries} подавлено как повторный запуск` : translate("Core недоступен — ожидается актуальная проекция"))}</small>
          {projectionReady && lastExtractionFailure ? <small>{translate("Последний сбой:")} {translate(lastExtractionFailure.reason_code ?? 'unknown')} · {translate(lastExtractionFailure.stage)}</small> : null}
        </article>
        <article className="operations-card">
          <div className="operations-card__topline">
            <span className="operations-card__eyebrow">Child runtime</span>
            <span className="operations-card__phase">{translate(projectionReady ? (activeChildren > 0 ? 'active' : 'idle') : 'offline')}</span>
          </div>
          <h3>Child jobs</h3>
          <div className="operations-card__metric">
            <strong>{translate(projectionReady ? activeChildren : '—')}</strong>
            <span>{translate(projectionReady ? translate("активных children") : translate("состояние не подтверждено"))}</span>
          </div>
          <small>{translate(projectionReady ? `${liveLeases} leases · ${deadLetters} dead-letter · ${count('child.report.accepted')} принятых отчётов` : translate("Core недоступен — ожидается актуальная проекция"))}</small>
        </article>
        <article className={`operations-card ${!projectionReady || pulseFailed ? 'operations-card--warning' : ''}`}>
          <div className="operations-card__topline">
            <span className="operations-card__eyebrow">Scheduler</span>
            <span className="operations-card__phase">{translate(!projectionReady ? 'syncing' : pulseFailed ? 'warning' : 'healthy')}</span>
          </div>
          <h3>Pulse</h3>
          <div className="operations-card__metric">
            <strong>{translate(!projectionReady ? (CONNECTED_STATES.includes(connection) ? translate("Синхронизация") : translate("Недоступно")) : pulseFailed ? translate("Внимание") : 'OK')}</strong>
            <span>
              {translate(!projectionReady
                ? translate("состояние Pulse не подтверждено")
                : pulseFailed
                  ? translate("есть ошибки расписаний")
                  : translate("ошибок не обнаружено"))}
            </span>
          </div>
          <small>
            {translate(projectionReady
              ? `${count('runtime.schedule_completed')} completed · ${count('runtime.schedule_requeued')} requeued · ${count('runtime.schedule_dead_letter')} dead-letter`
              : translate("Core недоступен — ожидается актуальная проекция"))}
          </small>
        </article>
        <article className={`operations-card ${toolCalls !== toolOutputs ? 'operations-card--warning' : ''}`}>
          <div className="operations-card__topline">
            <span className="operations-card__eyebrow">Tools</span>
            <span className="operations-card__phase">{translate(projectionReady ? (toolCalls !== toolOutputs ? 'pending' : 'synced') : 'offline')}</span>
          </div>
          <h3>{translate("Инструменты")}</h3>
          <div className="operations-card__metric">
            <strong>{translate(projectionReady ? toolCalls : '—')}</strong>
            <span>{translate(projectionReady ? translate("вызовов в текущем replay") : translate("состояние не подтверждено"))}</span>
          </div>
          <small>{translate(projectionReady ? `${toolOutputs} результатов · ${approvalRequests} запросов approval` : translate("Core недоступен — ожидается актуальная проекция"))}</small>
        </article>
      </div>

      <section className="operations-section operations-section--hive" aria-label={translate("Состояние улья")}>
        <div className="operations-section__header">
          <div>
            <p className="panel__eyebrow">Hive state</p>
            <h3>{translate("Состояние улья")}</h3>
          </div>
          <span className="operations-section__hint">
            {translate(hive ? `обновление 2 с · лимит ${hive.limits.maxSubagentsPerWorker}/${hive.limits.maxParallelSubagents}` : projectionReady ? 'ожидание первого снимка' : 'Core недоступен')}
          </span>
        </div>
        <div className="hive-metrics">
          <div className="hive-metric">
            <span>{translate("Королева")}</span>
            <strong>{hive?.queen.name || '—'}</strong>
            <small>{hive ? translate(hive.queen.status) : translate("нет данных")}</small>
          </div>
          <div className="hive-metric">
            <span>{translate("Всего агентов")}</span>
            <strong>{hive?.metrics.totalAgentsOnline ?? '—'}</strong>
            <small>{translate("в сети")}</small>
          </div>
          <div className="hive-metric">
            <span>{translate("Задач в работе")}</span>
            <strong>{hive?.metrics.activeTasks ?? '—'}</strong>
            <small>{hive ? translate(`эффективность ${hive.metrics.efficiencyPercent}%`) : translate("нет данных")}</small>
          </div>
        </div>
        <div className="hive-subsection">
          <h4>{translate("Департаменты")}</h4>
          {hive && hive.departments.length > 0 ? (
            <div className="hive-departments">
              {hive.departments.map((department) => (
                <article key={department.role} className="hive-department">
                  <div className="hive-department__topline">
                    <strong>{department.role || 'unknown'}</strong>
                    <span>{department.activeCount} / {department.maxCapacity}</span>
                  </div>
                  <div
                    className="hive-progress"
                    role="progressbar"
                    aria-label={department.role}
                    aria-valuemin={0}
                    aria-valuemax={Math.max(department.maxCapacity, 1)}
                    aria-valuenow={department.activeCount}
                  >
                    <span style={{ width: `${hivePercent(department.activeCount, department.maxCapacity)}%` }} />
                  </div>
                </article>
              ))}
            </div>
          ) : <p>{translate("Департаменты появятся после первого события Core.")}</p>}
        </div>
        <div className="hive-subsection">
          <h4>{translate("Активные агенты")}</h4>
          {hive && hive.activeAgents.length > 0 ? (
            <ul className="hive-agents">
              {hive.activeAgents.map((agent) => (
                <li key={agent.id}>
                  <div className="hive-agent__topline">
                    <strong>{agent.role || 'unknown'}</strong>
                    <span>depth {agent.depth}</span>
                  </div>
                  <small>{agent.currentAction || translate("ожидание")}</small>
                  <div
                    className="hive-progress"
                    role="progressbar"
                    aria-label={`${agent.role} subagents`}
                    aria-valuemin={0}
                    aria-valuemax={Math.max(agent.ceiling, 1)}
                    aria-valuenow={agent.spawnedSubagents}
                  >
                    <span style={{ width: `${hivePercent(agent.spawnedSubagents, agent.ceiling)}%` }} />
                  </div>
                  <small>{agent.spawnedSubagents} / {agent.ceiling}</small>
                </li>
              ))}
            </ul>
          ) : <p>{translate("Активных агентов нет.")}</p>}
        </div>
        <div className="hive-subsection">
          <h4>{translate("Поток задач")}</h4>
          <div className="hive-flow">
            {([
              [translate("Processing"), hive?.taskFlow.processing ?? 0, hive?.metrics.totalAgentsOnline ?? 0],
              [translate("Analyzing"), hive?.taskFlow.analyzing ?? 0, hive?.metrics.totalAgentsOnline ?? 0],
              [translate("Plan"), hive?.taskFlow.generatingPlan ?? 0, hive?.metrics.totalAgentsOnline ?? 0]
            ] as const).map(([label, value, total]) => (
              <div key={label} className="hive-flow__row">
                <span>{label}</span>
                <div
                  className="hive-progress"
                  role="progressbar"
                  aria-label={label}
                  aria-valuemin={0}
                  aria-valuemax={Math.max(total, 1)}
                  aria-valuenow={value}
                >
                  <span style={{ width: `${hivePercent(value, total)}%` }} />
                </div>
                <strong>{value}</strong>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section className="operations-section operations-section--knowledge" aria-label={translate("Локальный индекс workspace")}>
        <div className="operations-section__header">
          <div>
            <p className="panel__eyebrow">Workspace intelligence</p>
            <h3>{translate("Локальные знания workspace")}</h3>
          </div>
          <span className="operations-section__hint">{translate("индекс Core")}</span>
        </div>
        <p>
          {translate(indexStatus
            ? `${indexStatus.indexed_files} файлов · ${indexStatus.chunks} фрагментов · ${indexStatus.excluded} исключено · поколение ${indexStatus.generation ?? '—'} · ${indexStatus.vector_mode}`
            : translate("Состояние индекса ещё не получено."))}
          {translate(indexStatus?.dirty ? translate(" · индекс требует обновления") : '')}
        </p>
        <div className="operations-actions">
          <label className="operations-checkbox">
            <input
              type="checkbox"
              checked={embeddingEnabled}
              onChange={(input) => setEmbeddingEnabled(input.target.checked)}
            />
            {translate("локальные embeddings")}
          </label>
          <button type="button" disabled={!connected || !workspacePath} onClick={() => void updateIndex(false)}>
            {translate("Обновить индекс")}</button>
          <button type="button" disabled={!connected || !workspacePath} onClick={() => void updateIndex(true)}>
            {translate("Пересобрать полностью")}</button>
          <button
            type="button"
            disabled={!workspacePath}
            onClick={() => {
              if (api && workspacePath) void api.invoke('core.cancelWorkspaceIndex', { workspacePath })
            }}
          >
            {translate("Отменить")}</button>
          <button type="button" disabled={!connected || !workspacePath} onClick={refresh}>
            {translate("Обновить статус")}</button>
        </div>
        <div className="operations-actions operations-actions--search">
          <input
            type="search"
            aria-label={translate("Поиск по локальному индексу")}
            placeholder={translate("Найти symbol, путь или факт")}
            value={knowledgeQuery}
            onChange={(input) => setKnowledgeQuery(input.target.value)}
          />
          <button type="button" disabled={!workspacePath || knowledgeQuery.trim().length === 0} onClick={() => void searchKnowledge()}>
            {translate("Найти")}</button>
        </div>
        {searchPayload ? (
          <p>
            {translate(searchPayload.evidence.length)} {translate("источников · coverage")} {translate(searchPayload.diagnostics.coverage.toFixed(2))} · {translate(searchPayload.diagnostics.mode)} · {translate(searchPayload.diagnostics.stop_reason)}
            {translate(searchPayload.uncertainty ? ` · ${searchPayload.uncertainty}` : '')}
          </p>
        ) : null}
      </section>

      {message ? <p className="empty-state">{translate(message)}</p> : null}

      <section className="operations-section" aria-label={translate("Кандидаты continual refinement")}>
        <div className="operations-section__header">
          <div>
            <p className="panel__eyebrow">Learning loop</p>
            <h3>Continual refinement</h3>
          </div>
          <span className="operations-section__hint">bounded metadata</span>
        </div>
        <p>{translate("Core показывает только bounded metadata. Содержимое и transcript в UI не передаются.")}</p>
        {refinementList.length === 0 ? <p className="empty-state">{translate("Кандидатов refinement нет.")}</p> : (
          <ol className="operations-timeline">
            {refinementList.map((candidate) => (
              <li key={`${candidate.candidateId}-${candidate.revision}`}>
                <code>{translate(candidate.kind)} · {translate(candidate.ownerScope)}</code>
                <span>{candidate.title} · evidence {translate(candidate.evidenceCount)} · confidence {translate(candidate.confidence)}% · {translate(candidate.status)}</span>
                <small>hash {translate(candidate.contentHash.slice(0, 12))} · policy {translate(candidate.policySnapshotHash.slice(0, 12))}</small>
                {candidate.errorCode ? <small>{translate("ошибка:")}{translate(candidate.errorCode)}</small> : null}
                <div className="operations-actions">
                  {candidate.status === 'proposed' ? <button type="button" onClick={() => void refinementAction(candidate.candidateId, candidate.revision, candidate.version, 'approve')}>{translate("Одобрить")}</button> : null}
                  {candidate.status === 'approved' ? <button type="button" onClick={() => void refinementAction(candidate.candidateId, candidate.revision, candidate.version, 'activate')}>{translate("Активировать")}</button> : null}
                  {candidate.status === 'active' ? <button type="button" onClick={() => void refinementAction(candidate.candidateId, candidate.revision, candidate.version, 'rollback')}>{translate("Откатить")}</button> : null}
                  {candidate.status !== 'active' && candidate.status !== 'rejected' ? <button type="button" onClick={() => void refinementAction(candidate.candidateId, candidate.revision, candidate.version, 'reject')}>{translate("Отклонить")}</button> : null}
                </div>
              </li>
            ))}
          </ol>
        )}
      </section>

      <section className="operations-section" aria-label={translate("Предложения по услышанному")}>
        <div className="operations-section__header">
          <div>
            <p className="panel__eyebrow">Ambient suggestions</p>
            <h3>{translate("Предложения по услышанному")}</h3>
          </div>
          <span className="operations-section__hint">{translate("только после клика")}</span>
        </div>
        <p>
          {translate("Ева может предложить, но не может сделать. Любое из этих действий выполняется только твоим кликом.")}{translate(proposals
            ? ` Потолок: не больше ${proposals.max_per_hour} в час и ${proposals.max_per_day} в сутки.`
            : '')}
        </p>
        {openProposals.length === 0 ? (
          <p className="empty-state">{translate("Предложений нет: Ева ничего не предлагает.")}</p>
        ) : (
          <ol className="operations-timeline" aria-label={translate("Карточки предложений")}>
            {openProposals.map((proposal) => (
              <li key={proposal.proposal_id}>
                <code>{translate(PROPOSAL_KIND_LABELS[proposal.kind] ?? proposal.kind)}</code>
                <span className="operations-badge operations-badge--ambient">{translate("услышано")}</span>
                <span>
                  {proposal.title}
                  {translate(proposal.occurrences > 1 ? ` · упомянуто ${proposal.occurrences} раза` : '')}
                  {translate(proposal.source_episode_id ? '' : translate(" · источник удалён"))}
                </span>
                <div className="operations-actions">
                  <button
                    type="button"
                    disabled={deciding !== null}
                    onClick={() => void decideProposal(proposal, 'accept')}
                  >
                    {translate(proposal.kind === 'reminder' ? translate("Напомнить") : translate("Создать задачу"))}
                  </button>
                  <button
                    type="button"
                    disabled={deciding !== null}
                    onClick={() => void decideProposal(proposal, 'decline')}
                  >
                    {translate("Не надо")}</button>
                  <button
                    type="button"
                    disabled={deciding !== null}
                    onClick={() => void decideProposal(proposal, 'mute')}
                  >
                    {translate("Больше не предлагать такое")}</button>
                </div>
              </li>
            ))}
          </ol>
        )}
      </section>

      <section className="operations-section" aria-label={translate("Очередь памяти и конфликты")}>
        <div className="operations-section__header">
          <div>
            <p className="panel__eyebrow">Memory review</p>
            <h3>{translate("Очередь подтверждения и конфликты")}</h3>
          </div>
          <span className="operations-section__hint">{translate("только метаданные")}</span>
        </div>
        {pending.length > 0 ? (
          <>
            <div className="operations-actions">
              <label htmlFor="memory-source-filter">{translate("Источник")}</label>
              <select
                id="memory-source-filter"
                value={sourceFilter}
                onChange={(input) => setSourceFilter(input.target.value as SourceFilter)}
              >
                {SOURCE_FILTERS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {translate(option.label)}
                  </option>
                ))}
              </select>
              <small>{translate("услышано:")} {translate(ambientCount)} {translate("из")} {translate(pending.length)}</small>
            </div>
            <ol className="operations-timeline" aria-label={translate("Кандидаты в память")}>
              {visiblePending.map((record) => (
                <li key={record.id}>
                  <label className="operations-checkbox">
                    <input
                      type="checkbox"
                      checked={selected.includes(record.id)}
                      onChange={() => toggle(record.id)}
                    />
                    <code>{translate(KIND_LABELS[record.kind] ?? record.kind)}</code>
                  </label>
                  {record.source_trust === 'ambient' ? (
                    <span className="operations-badge operations-badge--ambient">{translate("услышано")}</span>
                  ) : null}
                  <span>
                    {translate(record.canonical_subject ?? translate("без темы"))} ·{translate(' ')}
                    {translate(TRUST_LABELS[record.source_trust] ?? record.source_trust)} {translate("· уверенность")}{translate(' ')}
                    {translate((record.model_confidence ?? 0).toFixed(2))} {translate("· проверка")} {translate(record.validation_status)}
                    {translate(' · ')}{translate(record.authority ?? 'user_asserted')} · {translate(record.durability ?? 'durable')} · governance {translate((record.confidence ?? 1).toFixed(2))}
                    {translate(record.privacy_class === 'normal' ? '' : translate(" · содержимое скрыто"))}
                    {translate(record.source_trust === 'ambient' ? translate(" · говорящий не подтверждён") : '')}
                  </span>
                  <div className="operations-actions">
                    {editing === record.id ? (
                      <>
                        <input
                          type="text"
                          aria-label={translate("Новая формулировка")}
                          value={draft}
                          onChange={(input) => setDraft(input.target.value)}
                        />
                        <button
                          type="button"
                          disabled={draft.trim().length === 0}
                          onClick={() => void revise(record.id, draft, false)}
                        >
                          {translate("Сохранить правку")}</button>
                        <button type="button" onClick={() => setEditing(null)}>
                          {translate("Отмена")}</button>
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
                          {translate("Изменить")}</button>
                        <button type="button" onClick={() => void revise(record.id, '', true)}>
                          {translate("Только на эту сессию")}</button>
                      </>
                    )}
                  </div>
                </li>
              ))}
            </ol>
            <div className="operations-actions">
              <button type="button" disabled={selected.length === 0} onClick={() => void decide('core.confirmMemory')}>
                {translate("Сохранить выбранные")}</button>
              <button type="button" disabled={selected.length === 0} onClick={() => void decide('core.rejectMemory')}>
                {translate("Отклонить выбранные")}</button>
            </div>
          </>
        ) : (
          <p className="empty-state">{translate("Кандидатов в память нет: Core ничего не ждёт от вас.")}</p>
        )}

        {conflicts.length > 0 ? (
          <ol className="operations-timeline" aria-label={translate("Конфликты памяти")}>
            {conflicts.map((conflict) => (
              <li key={conflict.pending.id}>
                <code>{translate(conflict.conflict_key)}</code>
                <span>
                  {translate("активная")} {translate(conflict.active.id)} {translate("· цепочка")} {translate(conflict.supersession_chain.join(' → '))}
                </span>
                <button type="button" onClick={() => void resolveConflict(conflict)}>
                  {translate("Заменить новой записью")}</button>
              </li>
            ))}
          </ol>
        ) : null}
      </section>

      <section className="operations-section" aria-label={translate("События и контексты child-задач")}>
        <div className="operations-section__header">
          <div>
            <p className="panel__eyebrow">Child runtime</p>
            <h3>{translate("Дочерние задачи и контексты")}</h3>
          </div>
          <span className="operations-section__hint">bounded timeline</span>
        </div>
        {childProjection.length > 0 ? (
          <ol className="operations-timeline" aria-label={translate("Последние child события")}>
            {childProjection.slice(0, 8).map(({ event, item }) => (
              <li key={`${event.sequenceId}-${event.eventType}`}>
                <code>{translate(item.role ?? 'child')} · {translate(item.state ?? event.eventType)}{item.report_status ? ` · ${translate(item.report_status)}` : ''}</code>
                <span>{translate(item.child_task_id ?? translate("идентификатор скрыт"))} · depth={item.depth ?? 1}{translate(item.depth === 2 ? ' · hop 2' : '')} · rev {translate(item.revision ?? 0)}{translate(item.reason_code ? ` · ${item.reason_code}` : '')}{translate(item.dead_letter ? ' · dead-letter' : '')}</span>
                {item.report_summary ? <small>{translate(item.report_summary)}</small> : null}
                {item.planned_ceiling ? (
                  <small>
                    {translate(`подагенты ${item.spawned_subagents ?? 0} / ${item.planned_ceiling}`)}
                    {item.extensions_granted ? ` · ${translate('расширений')} ${item.extensions_granted}` : ''}
                    {item.in_flight_subagents ? ` · ${translate('в работе')} ${item.in_flight_subagents}` : ''}
                    {item.lazy_waiting ? ` · ${translate('ожиданий')} ${item.lazy_waiting}` : ''}
                  </small>
                ) : null}
                {item.extension_requested ? (
                  <small>{translate('ожидает решения')} · {translate("расширение grant'а запрошено")}</small>
                ) : null}
              </li>
            ))}
          </ol>
        ) : <p className="empty-state">{translate("Child timeline появится после запуска bounded read-only задачи.")}</p>}

        {retainedChildren.length > 0 ? (
          <ol className="operations-timeline" aria-label={translate("Сохранённые child контексты")}>
            {retainedChildren.slice(0, 16).map((child, index) => (
              <li key={`${child.child_id ?? 'child'}-${index}`}>
                <code>{translate(child.stable_name || child.child_id || 'child')} · {translate(child.role || 'role')}</code>
                <span>{translate(child.lifecycle || 'unknown')} · rev {translate(child.revision ?? 0)} · pending {translate(child.pending_count ?? 0)}{translate(child.last_delivery_outcome ? ` · ${child.last_delivery_outcome}` : '')}{translate(child.invalidation_reason ? ` · ${child.invalidation_reason}` : '')}</span>
                {child.lifecycle !== 'deleted' && child.child_id && child.registry_version !== undefined ? <button type="button" onClick={() => void deleteRetainedChild(child)}>{translate("Удалить контекст")}</button> : null}
              </li>
            ))}
          </ol>
        ) : null}
      </section>
    </section>
  )
}
