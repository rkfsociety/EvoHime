import { translate } from './i18n'
import { useCallback, useEffect, useMemo, useState } from 'react'

import { PROVIDER_KINDS } from '@shared/api'
import type { AmbientProposal, AmbientProposalList, ChatProviderMode, ConnectionState, CoreEvent, ProviderKind, ProviderProfileId, ProviderProfileSummary, ProviderSummary, RepairStatus } from '@shared/api'

import { useShellApi } from './shell-api'
import { ModelPicker } from './ModelPicker'
import { ImageGenerationPanel } from './ImageGenerationPanel'
import { PROVIDER_PROFILE_LABELS } from './provider-labels'
import { UiIcon } from './UiIcon'
import type { UiIconName } from './UiIcon'
import { HiveGraph } from './components/HiveGraph'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly repair?: RepairStatus | null
  readonly resolvedApprovalIds?: readonly string[]
  readonly onApprovalResolved?: (approvalId: string) => void
}

interface HiveDashboardProps extends Pick<Props, 'connection' | 'events' | 'resolvedApprovalIds' | 'onApprovalResolved'> {
  readonly workspace?: string | null
  readonly chatId?: string | null
  readonly onChatOpened?: (chatId: string) => void
  readonly onChatTouched?: () => void
}

interface HiveChatMessage {
  readonly id: string
  readonly role: 'user' | 'queen'
  readonly text: string
  readonly failed?: boolean
}

interface HiveUserPrompt {
  readonly taskId: string
  readonly text: string
  readonly sequenceHint: number
}

interface ApprovalRequestView {
  readonly approvalId: string
  readonly taskId: string
  readonly summary: string
  readonly permission: string
  readonly scope: string
  readonly toolName: string
  /** Оценка стоимости действия из `preview`; `null`, если Core её не дал. */
  readonly costUsd: number | null
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
  readonly queen: { readonly status: string; readonly name: string; readonly reason: string; readonly wakeAtMs: number | null }
  readonly metrics: {
    readonly totalAgentsOnline: number
    readonly activeTasks: number
    readonly tasksToday: number
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
  readonly budget: {
    readonly provider: string
    readonly historyAvailable: boolean
    readonly model: string
    readonly requestsLastMinute: number
    readonly requestsLastHour: number
    readonly requestsLastDay: number
    readonly minuteCap: number
    readonly hourCap: number
    readonly dayCap: number
    readonly minuteExhausted: boolean
    readonly exhausted: boolean
    readonly dailyCostAvailable: boolean
    readonly dailyCostNanoUsd: number | null
  }
  readonly dailyBudgetUsd: number
  readonly evaWorkspacePath: string | null
  readonly pulseStatus: string
  readonly nextWakeAtMs: number | null
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
  // Оболочка и Core выпускаются отдельными модулями, поэтому опубликованный
  // Core может отставать от UI. Известные версии проекции читаются с defaults
  // для отсутствующих секций; неизвестная версия отклоняется целиком.
  const version = root['schema_version']
  if (version !== 1 && version !== 2 && version !== 3 && version !== 4 && version !== 5) return null
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
  const budget = record(root['budget'])
  return {
    queen: {
      status: text(queen['status'], 32),
      name: text(queen['name'], 64),
      reason: text(queen['reason'], 180),
      wakeAtMs: typeof queen['wake_at_ms'] === 'number' && Number.isSafeInteger(queen['wake_at_ms']) ? queen['wake_at_ms'] : null
    },
    metrics: {
      totalAgentsOnline: count(metrics['total_agents_online'], 65_535),
      activeTasks: count(metrics['active_tasks'], 65_535),
      tasksToday: count(metrics['tasks_today'], 1_000_000),
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
    },
    budget: {
      provider: text(budget['provider'], 64),
      historyAvailable: budget['history_available'] === true,
      model: text(budget['model'], 128),
      requestsLastHour: count(budget['requests_last_hour'], 1_000_000),
      requestsLastMinute: count(budget['requests_last_minute'], 1_000_000),
      requestsLastDay: count(budget['requests_last_day'], 1_000_000),
      // A zero cap is Core's "no configured limit", and it must stay zero here.
      minuteCap: count(budget['minute_cap'], 1_000_000),
      hourCap: count(budget['hour_cap'], 1_000_000),
      dayCap: count(budget['day_cap'], 1_000_000),
      minuteExhausted: budget['minute_exhausted'] === true,
      exhausted: budget['history_available'] === true && (budget['minute_exhausted'] === true || budget['hour_exhausted'] === true || budget['day_exhausted'] === true),
      dailyCostAvailable: budget['daily_cost_available'] === true,
      dailyCostNanoUsd: typeof budget['daily_cost_nano_usd'] === 'number' && Number.isSafeInteger(budget['daily_cost_nano_usd']) && budget['daily_cost_nano_usd'] >= 0
        ? budget['daily_cost_nano_usd']
        : null
    },
    dailyBudgetUsd: typeof root['daily_budget_usd'] === 'number' && Number.isFinite(root['daily_budget_usd']) && root['daily_budget_usd'] >= 0
      ? Math.min(root['daily_budget_usd'], 10_000)
      : 0,
    evaWorkspacePath: typeof root['eva_workspace_path'] === 'string' ? root['eva_workspace_path'].slice(0, 4_096) : null,
    pulseStatus: text(root['pulse_status'], 16),
    nextWakeAtMs: typeof root['next_wake_at_ms'] === 'number' && Number.isSafeInteger(root['next_wake_at_ms'])
      ? root['next_wake_at_ms']
      : null
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

/**
 * Канонические карточки департаментов. Core отдаёт роли свободным текстом
 * (`researcher`, `reviewer`, …), поэтому роль сопоставляется с карточкой по
 * списку псевдонимов; незнакомая роль не подменяется догадкой, а добавляется
 * отдельной карточкой с именем роли.
 */
interface HiveDepartmentCard {
  readonly key: string
  readonly label: string
  readonly icon: UiIconName
  readonly aliases: readonly string[]
}

const HIVE_DEPARTMENT_CARDS: readonly HiveDepartmentCard[] = [
  { key: 'analytics', label: 'Аналитика', icon: 'activity', aliases: ['analyst', 'analytics', 'analysis', 'review'] },
  { key: 'development', label: 'Разработка', icon: 'terminal', aliases: ['developer', 'engineer', 'coder', 'implementer', 'dev'] },
  { key: 'testing', label: 'Тестирование', icon: 'check-circle', aliases: ['qa', 'tester', 'test', 'verifier', 'validation'] },
  { key: 'planning', label: 'Планирование', icon: 'compass', aliases: ['planner', 'architect', 'plan'] },
  { key: 'research', label: 'Исследования', icon: 'target', aliases: ['researcher', 'research', 'scout', 'investigator'] },
  { key: 'integrations', label: 'Интеграции', icon: 'link', aliases: ['integrator', 'integration', 'connector', 'adapter'] },
  { key: 'documentation', label: 'Документация', icon: 'folder', aliases: ['documenter', 'documentation', 'docs', 'writer'] },
  { key: 'support', label: 'Поддержка', icon: 'message', aliases: ['support', 'assistant', 'responder', 'helper'] }
]

interface DepartmentView {
  readonly key: string
  readonly label: string
  readonly icon: UiIconName
  readonly activeCount: number
  readonly maxCapacity: number
}

/** Сопоставляет роли Core с карточками; порядок карточек стабилен. */
function matchDepartmentCard(role: string): HiveDepartmentCard | null {
  const normalized = role.trim().toLowerCase()
  if (normalized.length === 0) return null
  return HIVE_DEPARTMENT_CARDS.find((card) =>
    card.aliases.some((alias) => normalized === alias || normalized.includes(alias))
  ) ?? null
}

function departmentLabel(role: string): string {
  return matchDepartmentCard(role)?.label ?? role
}

function departmentView(departments: HiveStateView['departments']): readonly DepartmentView[] {
  const buckets = new Map<string, { active: number; max: number }>()
  const extra: HiveStateView['departments'][number][] = []
  for (const department of departments) {
    const card = matchDepartmentCard(department.role)
    if (!card) {
      extra.push(department)
      continue
    }
    const bucket = buckets.get(card.key) ?? { active: 0, max: 0 }
    // Несколько ролей могут попасть в одну карточку: нагрузка складывается,
    // ёмкость берётся по максимуму — это один и тот же потолок Core.
    buckets.set(card.key, {
      active: bucket.active + department.activeCount,
      max: Math.max(bucket.max, department.maxCapacity)
    })
  }
  const cards: DepartmentView[] = HIVE_DEPARTMENT_CARDS.map((card) => {
    const bucket = buckets.get(card.key)
    return {
      key: card.key,
      label: card.label,
      icon: card.icon,
      activeCount: bucket?.active ?? 0,
      maxCapacity: bucket?.max ?? 0
    }
  })
  for (const department of extra) {
    cards.push({
      key: `role:${department.role}`,
      label: department.role || 'unknown',
      icon: 'box',
      activeCount: department.activeCount,
      maxCapacity: department.maxCapacity
    })
  }
  return cards
}

const HIVE_INTERVALS = [1000, 2000, 5000, 10000] as const
const HIVE_PREFERENCE_KEY = 'evohime.hive.dashboard.v1'

interface HivePreference {
  readonly pulse: boolean
  readonly intervalMs: number
}

/** Единственный владелец этого состояния — оболочка: это режим отображения. */
function loadHivePreference(): HivePreference {
  try {
    const stored: unknown = JSON.parse(window.localStorage.getItem(HIVE_PREFERENCE_KEY) ?? 'null')
    if (stored === null || typeof stored !== 'object' || Array.isArray(stored)) {
      return { pulse: true, intervalMs: 2000 }
    }
    const record = stored as Record<string, unknown>
    const interval = typeof record['intervalMs'] === 'number' && (HIVE_INTERVALS as readonly number[]).includes(record['intervalMs'])
      ? record['intervalMs']
      : 2000
    return { pulse: record['pulse'] !== false, intervalMs: interval }
  } catch {
    return { pulse: true, intervalMs: 2000 }
  }
}

function saveHivePreference(preference: HivePreference): void {
  try {
    window.localStorage.setItem(HIVE_PREFERENCE_KEY, JSON.stringify(preference))
  } catch {
    // Storage may be unavailable; the in-memory mode stays active.
  }
}

const PROVIDER_KIND_LABELS: Record<ProviderKind, string> = {
  literouter: 'LiteRouter',
  openai_compatible: 'OpenAI-compatible API',
  openai_responses: 'OpenAI Responses',
  ollama: 'Ollama (локально)'
}

interface ProviderOption {
  readonly key: string
  readonly provider: ProviderKind
  readonly profileId?: ProviderProfileId
  readonly label: string
  readonly configured: boolean
}

/** Список провайдеров строится по тому, что реально сохранено в оболочке. */
function providerOptions(summary: ProviderSummary | null): readonly ProviderOption[] {
  if (!summary) return []
  const options: ProviderOption[] = []
  const seen = new Set<string>()
  const push = (provider: ProviderKind, profileId: ProviderProfileId | undefined, profile: ProviderProfileSummary): void => {
    const key = profileId ? `${provider}:${profileId}` : provider
    if (seen.has(key)) return
    seen.add(key)
    const name = profileId ? PROVIDER_PROFILE_LABELS[profileId] : PROVIDER_KIND_LABELS[provider]
    const free = profile.tier === 'free' || profile.model.toLowerCase().includes(':free')
    options.push({
      key,
      provider,
      ...(profileId ? { profileId } : {}),
      label: `${name}${free ? ' (Free)' : ''}`,
      configured: profile.configured
    })
  }
  for (const provider of PROVIDER_KINDS) push(provider, undefined, summary.profiles[provider])
  for (const [profileId, profile] of Object.entries(summary.compatibleProfiles ?? {})) {
    if (!profile) continue
    push('openai_compatible', profileId as ProviderProfileId, profile)
  }
  return options
}

function currentProviderKey(summary: ProviderSummary | null): string {
  if (!summary) return ''
  return summary.provider === 'openai_compatible' && summary.profileId
    ? `openai_compatible:${summary.profileId}`
    : summary.provider
}

function formatMoney(nanoUsd: number): string {
  const usd = nanoUsd / 1_000_000_000
  if (!Number.isFinite(usd) || usd <= 0) return '$0.00'
  if (usd < 0.01) return `$${usd.toFixed(9).replace(/0+$/, '').replace(/\.$/, '')}`
  return `$${usd.toFixed(2)}`
}

function formatClock(atMs: number): string {
  const at = new Date(atMs)
  if (!Number.isFinite(at.getTime())) return '—'
  return at.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })
}

function agentMonogram(id: string): string {
  const tail = id.split(/[:\-_/]/).filter(Boolean).at(-1) ?? id
  return tail.slice(-2).toUpperCase() || 'AI'
}

const QUEEN_REASON_LABELS: Record<string, string> = {
  provider_minute_cap_reached: 'лимит запросов за минуту достигнут',
  provider_hour_cap_reached: 'лимит запросов за час достигнут',
  provider_day_cap_reached: 'лимит запросов за сутки достигнут',
  provider_request_cap_reached: 'лимит запросов провайдера достигнут'
}

type HiveLogLevel = 'success' | 'warning' | 'info'

interface HiveLogEntry {
  readonly id: string
  readonly at: string
  readonly level: HiveLogLevel
  readonly text: string
}

const HIVE_LOG_ICONS: Record<HiveLogLevel, UiIconName> = {
  success: 'check-circle',
  warning: 'alert-triangle',
  info: 'circle'
}

/** Понятные подписи для частых событий; незнакомый тип показывается как есть. */
const HIVE_LOG_LABELS: Record<string, string> = {
  'task.started': 'Задача запущена',
  'task.completed': 'Задача завершена',
  'task.failed': 'Задача остановлена с ошибкой',
  'task.stopped': 'Задача остановлена',
  'approval.required': 'Требуется подтверждение действия',
  'tool.started': 'Инструмент запущен',
  'tool.output': 'Инструмент вернул вывод',
  'child.workflow': 'Шаг дочерней задачи',
  'runtime.schedule_completed': 'Pulse: расписание выполнено',
  'runtime.schedule_failed': 'Pulse: ошибка расписания',
  'runtime.schedule_dead_letter': 'Pulse: задание в dead-letter'
}

function hiveLogLevel(eventType: string): HiveLogLevel {
  if (eventType === 'approval.required') return 'warning'
  if (/(failed|denied|dead_letter|error|cancelled|timed_out|stopped)/.test(eventType)) return 'warning'
  if (/(completed|succeeded|accepted|resolved)/.test(eventType)) return 'success'
  return 'info'
}

function hiveLogEntries(
  events: readonly CoreEvent[],
  observedAt: ReadonlyMap<number, string>,
  limit: number
): readonly HiveLogEntry[] {
  return events
    .filter((event) => event.eventType !== 'hive_state.snapshot')
    .slice(0, limit)
    .map((event) => ({
      id: `seq-${event.sequenceId}`,
      at: observedAt.get(event.sequenceId) ?? '—',
      level: hiveLogLevel(event.eventType),
      text: `${HIVE_LOG_LABELS[event.eventType] ?? event.eventType}${event.taskId ? ` · ${event.taskId}` : ''}`
    }))
}

/** Круговой индикатор эффективности: только SVG, без сторонних библиотек. */
function EfficiencyRing({ percent }: { readonly percent: number }): React.JSX.Element {
  const bounded = Math.max(0, Math.min(100, Math.round(percent)))
  const radius = 34
  const circumference = 2 * Math.PI * radius
  return (
    <svg className="hive-ring" viewBox="0 0 88 88" role="img" aria-label={`${translate('Эффективность')} ${bounded}%`}>
      <circle className="hive-ring__track" cx="44" cy="44" r={radius} />
      <circle
        className="hive-ring__value"
        cx="44"
        cy="44"
        r={radius}
        strokeDasharray={circumference}
        strokeDashoffset={circumference * (1 - bounded / 100)}
        transform="rotate(-90 44 44)"
      />
      <text className="hive-ring__label" x="44" y="49" textAnchor="middle">{bounded}%</text>
    </svg>
  )
}

function parseApprovalRequest(event: CoreEvent): ApprovalRequestView | null {
  try {
    const root = JSON.parse(event.payload) as Record<string, unknown>
    const text = (key: string, fallback = ''): string => typeof root[key] === 'string' ? (root[key] as string).slice(0, 240) : fallback
    const approvalId = text('approval_id')
    if (!approvalId) return null
    // Core publishes `preview` as a structured object, so the human-readable
    // summary must be read from its own field instead of stringified.
    const preview = root['preview'] !== null && typeof root['preview'] === 'object' && !Array.isArray(root['preview'])
      ? root['preview'] as Record<string, unknown>
      : {}
    const previewText = (key: string): string => typeof preview[key] === 'string' ? (preview[key] as string).slice(0, 240) : ''
    // Оценку стоимости Core пока не публикует; берём только явно переданное
    // значение и никогда не выводим стоимость из догадки.
    const cost = preview['cost_usd'] ?? preview['costUsd'] ?? preview['estimated_cost_usd']
    const costUsd = typeof cost === 'number' && Number.isFinite(cost) && cost >= 0 ? cost : null
    return {
      approvalId,
      taskId: event.taskId,
      summary: text('summary') || previewText('summary') || 'Ева запросила подтверждение действия.',
      permission: text('permission') || 'требуется разрешение',
      scope: text('scope') || 'область не указана',
      toolName: text('tool_name') || 'операция Core',
      costUsd
    }
  } catch {
    return null
  }
}

/** Плашка human-in-the-loop: жёлто-оранжевая, с решением пользователя. */
function ApprovalRequest({ request, busy, onResolve }: { readonly request: ApprovalRequestView; readonly busy: boolean; readonly onResolve: (granted: boolean) => void }): React.JSX.Element {
  return (
    <aside className="hive-approval" role="alert" aria-label={translate('Запрос подтверждения')}>
      <div className="hive-approval__icon" aria-hidden="true"><UiIcon name="alert-triangle" size={22} /></div>
      <div className="hive-approval__body">
        <p className="hive-approval__eyebrow">Human-in-the-loop</p>
        <strong>{translate('Требуется ваше подтверждение')}</strong>
        <span className="hive-approval__summary">{translate(request.summary)}</span>
        <span className="hive-approval__meta">{translate(request.toolName)} · {translate(request.permission)} · {translate(request.scope)}</span>
      </div>
      <div className="hive-approval__cost">
        <span>{translate('Стоимость')}</span>
        <strong>{request.costUsd === null ? '—' : `~$${request.costUsd.toFixed(2)}`}</strong>
      </div>
      <div className="hive-approval__actions">
        <button type="button" className="hive-button hive-button--approve" onClick={() => onResolve(true)} disabled={busy}>{translate('Разрешить')}</button>
        <button type="button" className="hive-button hive-button--reject" onClick={() => onResolve(false)} disabled={busy}>{translate('Отклонить')}</button>
      </div>
    </aside>
  )
}

const EMPTY_BUDGET: HiveStateView['budget'] = {
  provider: '',
  historyAvailable: false,
  model: '',
  requestsLastMinute: 0,
  requestsLastHour: 0,
  requestsLastDay: 0,
  minuteCap: 0,
  hourCap: 0,
  dayCap: 0,
  minuteExhausted: false,
  exhausted: false,
  dailyCostAvailable: false,
  dailyCostNanoUsd: null
}

/** Formats the exact reset moment Core computed, never a fixed default. */
function formatWakeAt(wakeAtMs: number | null, nowMs: number): string {
  if (wakeAtMs === null || !Number.isSafeInteger(wakeAtMs)) return translate('сброс не вычислен Core')
  if (wakeAtMs <= nowMs) return translate('лимит уже должен был сброситься')
  const resetAt = new Date(wakeAtMs)
  if (!Number.isFinite(resetAt.getTime())) return translate('сброс не вычислен Core')
  return `${translate('сброс в')} ${resetAt.toLocaleTimeString()}`
}

function parseEventPayloadObject(rawPayload: string): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(rawPayload) as unknown
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null
    const record = parsed as Record<string, unknown>
    const nested = record['payload']
    if (nested && typeof nested === 'object' && !Array.isArray(nested)) {
      return nested as Record<string, unknown>
    }
    return record
  } catch {
    return null
  }
}

function readEventString(payload: Record<string, unknown> | null, key: string): string {
  if (!payload) return ''
  const value = payload[key]
  return typeof value === 'string' ? value.trim() : ''
}

function extractHiveChatMessages(
  events: readonly CoreEvent[],
  userPrompts: readonly HiveUserPrompt[],
  limit = 5
): readonly HiveChatMessage[] {
  const items: Array<HiveChatMessage & { readonly order: number }> = []
  const promptTaskIds = new Set<string>()
  const queenByTask = new Map<string, { text: string; order: number; failed: boolean }>()
  const minSequenceByTask = new Map<string, number>()
  let maxSequence = 0

  for (const event of events) {
    if (event.sequenceId > maxSequence) maxSequence = event.sequenceId
    if (!event.taskId) continue
    const prev = minSequenceByTask.get(event.taskId)
    if (prev === undefined || event.sequenceId < prev) {
      minSequenceByTask.set(event.taskId, event.sequenceId)
    }
  }

  for (let index = 0; index < userPrompts.length; index += 1) {
    const prompt = userPrompts[index]
    if (!prompt || prompt.text.length === 0) continue
    if (prompt.taskId.length > 0) promptTaskIds.add(prompt.taskId)
    const taskMinSeq = prompt.taskId ? minSequenceByTask.get(prompt.taskId) : undefined
    const order = taskMinSeq !== undefined ? taskMinSeq - 0.5 : maxSequence + 1 + index * 0.01
    items.push({
      id: `user-${prompt.taskId || prompt.sequenceHint}`,
      role: 'user',
      text: prompt.text,
      order
    })
  }

  // `events` приходят newest-first; обходим с конца, чтобы собрать ответы хронологически.
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const event = events[index]
    if (!event) continue
    const payload = parseEventPayloadObject(event.payload)
    const taskKey = event.taskId || `seq-${event.sequenceId}`

    if (event.eventType === 'task.started') {
      const promptText = readEventString(payload, 'prompt')
      if (promptText.length > 0 && !promptTaskIds.has(taskKey)) {
        promptTaskIds.add(taskKey)
        items.push({
          id: `user-${taskKey}`,
          role: 'user',
          text: promptText,
          order: event.sequenceId - 0.5
        })
      }
      continue
    }

    if (event.eventType === 'agent.message.delta') {
      const chunk = typeof payload?.['content'] === 'string' ? payload['content'] : ''
      if (chunk.length === 0) continue
      const existing = queenByTask.get(taskKey)
      queenByTask.set(taskKey, {
        text: `${existing?.text ?? ''}${chunk}`,
        order: event.sequenceId,
        failed: false
      })
      continue
    }

    if (event.eventType === 'task.completed') {
      const finalMessage = readEventString(payload, 'final_message') || readEventString(payload, 'summary')
      const existing = queenByTask.get(taskKey)
      if (finalMessage.length > 0 && (!existing || existing.text.trim().length === 0)) {
        queenByTask.set(taskKey, { text: finalMessage, order: event.sequenceId, failed: false })
      } else if (existing) {
        queenByTask.set(taskKey, { ...existing, order: event.sequenceId })
      }
      continue
    }

    if (event.eventType === 'task.failed') {
      const errorCode = readEventString(payload, 'error_code') || readEventString(payload, 'error')
      queenByTask.set(taskKey, {
        text: errorCode.length > 0 ? errorCode : 'Задача завершилась ошибкой.',
        order: event.sequenceId,
        failed: true
      })
      continue
    }

    if (event.eventType === 'task.stopped') {
      const existing = queenByTask.get(taskKey)
      if (!existing || existing.text.trim().length === 0) {
        queenByTask.set(taskKey, {
          text: 'Задача остановлена.',
          order: event.sequenceId,
          failed: false
        })
      }
    }
  }

  for (const [taskKey, queen] of queenByTask.entries()) {
    const text = queen.text.trim()
    if (text.length === 0) continue
    items.push({
      id: `queen-${taskKey}`,
      role: 'queen',
      text,
      failed: queen.failed,
      order: queen.order
    })
  }

  return items
    .sort((left, right) => left.order - right.order)
    .slice(-limit)
    .map(({ id, role, text, failed }) => ({ id, role, text, ...(failed ? { failed } : {}) }))
}

function isQueenThinking(
  events: readonly CoreEvent[],
  pendingTaskIds: ReadonlySet<string>,
  sendingMessage: boolean,
  activeApprovalTaskIds: ReadonlySet<string>
): boolean {
  if (sendingMessage) return true
  const terminalTypes = new Set(['task.completed', 'task.failed', 'task.stopped'])

  for (const taskId of pendingTaskIds) {
    if (activeApprovalTaskIds.has(taskId)) continue
    const finished = events.some((event) => event.taskId === taskId && terminalTypes.has(event.eventType))
    if (!finished) return true
  }

  const activeTaskSeq = new Map<string, number>()
  const terminalTaskSeq = new Map<string, number>()
  for (const event of events) {
    if (!event.taskId) continue
    if (terminalTypes.has(event.eventType)) {
      const current = terminalTaskSeq.get(event.taskId) ?? -1
      if (event.sequenceId > current) terminalTaskSeq.set(event.taskId, event.sequenceId)
    } else if (event.eventType === 'task.started' || event.eventType === 'agent.message.delta') {
      const current = activeTaskSeq.get(event.taskId) ?? -1
      if (event.sequenceId > current) activeTaskSeq.set(event.taskId, event.sequenceId)
    }
  }

  for (const [taskId, startedSeq] of activeTaskSeq.entries()) {
    if (activeApprovalTaskIds.has(taskId)) continue
    const terminalSeq = terminalTaskSeq.get(taskId) ?? -1
    if (startedSeq > terminalSeq) return true
  }

  return false
}

/**
 * Единый дашборд «Улей»: департаменты, Королева Ева, метрики и бюджет,
 * активные агенты и логи. Состояние читается из `HiveStateSnapshot`, решения
 * уходят в Core, режим авто-обновления принадлежит оболочке.
 */
export function HiveDashboard({
  connection,
  events,
  workspace = null,
  chatId = null,
  onChatOpened,
  onChatTouched,
  resolvedApprovalIds = [],
  onApprovalResolved
}: HiveDashboardProps): React.JSX.Element {
  const api = useShellApi()
  const [resolvingApprovalIds, setResolvingApprovalIds] = useState<ReadonlySet<string>>(() => new Set())
  const [resolvedLocally, setResolvedLocally] = useState<readonly string[]>([])
  const [commandError, setCommandError] = useState<string | null>(null)
  const [preference, setPreference] = useState<HivePreference>(() => loadHivePreference())
  const [allLogs, setAllLogs] = useState(false)
  const [provider, setProvider] = useState<ProviderSummary | null>(null)
  const [providerBusy, setProviderBusy] = useState(false)
  const [providerMessage, setProviderMessage] = useState<string | null>(null)
  const [evaWorkspacePath, setEvaWorkspacePath] = useState('')
  const [evaWorkspaceDraft, setEvaWorkspaceDraft] = useState('')
  const [evaWorkspaceError, setEvaWorkspaceError] = useState<string | null>(null)
  const [evaWorkspaceBusy, setEvaWorkspaceBusy] = useState(false)
  const [observedAt, setObservedAt] = useState<ReadonlyMap<number, string>>(() => new Map())
  const [chatDraft, setChatDraft] = useState('')
  const [sendingMessage, setSendingMessage] = useState(false)
  const [pendingTaskIds, setPendingTaskIds] = useState<ReadonlySet<string>>(() => new Set())
  const [localPrompts, setLocalPrompts] = useState<readonly HiveUserPrompt[]>([])
  const [historyPrompts, setHistoryPrompts] = useState<readonly HiveUserPrompt[]>([])
  const connected = CONNECTED_STATES.includes(connection)
  const hive = useMemo(() => parseHiveState(latest(events, 'hive_state.snapshot')), [events])
  useEffect(() => {
    if (!hive) return
    const configured = hive.evaWorkspacePath ?? ''
    setEvaWorkspacePath(configured)
    setEvaWorkspaceDraft(configured)
  }, [hive?.evaWorkspacePath])
  const hiveSettingsResult = latest(events, 'hive_settings.saved')
  useEffect(() => {
    if (!hiveSettingsResult) return
    try {
      const result = JSON.parse(hiveSettingsResult.payload) as { accepted?: boolean; eva_workspace_path?: string | null; error_code?: string }
      if (!result.accepted && !result.error_code && !result.eva_workspace_path) {
        setEvaWorkspacePath('')
        setEvaWorkspaceDraft('')
        setEvaWorkspaceError('Путь не задан; инструменты саморазвития выключены.')
        return
      }
      if (!result.accepted || result.error_code) {
        setEvaWorkspacePath('')
        setEvaWorkspaceDraft('')
        setEvaWorkspaceError(result.error_code || 'Путь к репозиторию отклонён Core.')
        return
      }
      setEvaWorkspacePath(result.eva_workspace_path ?? '')
      setEvaWorkspaceDraft(result.eva_workspace_path ?? '')
      setEvaWorkspaceError(null)
    } catch {
      setEvaWorkspaceError('Core вернул некорректный ответ о пути репозитория.')
    }
  }, [hiveSettingsResult])

  // Метка времени ставится в момент, когда оболочка получила событие: Core не
  // публикует wall-clock для каждого события, а выдумывать время нельзя.
  useEffect(() => {
    setObservedAt((current) => {
      let next = new Map(current)
      if (next.size > 2000) {
        next = new Map()
        for (const event of events) {
          const seen = current.get(event.sequenceId)
          if (seen) next.set(event.sequenceId, seen)
        }
      }
      let changed = next.size !== current.size
      for (const event of events) {
        if (next.has(event.sequenceId)) continue
        next.set(event.sequenceId, new Date().toLocaleTimeString('ru-RU', { hour12: false }))
        changed = true
      }
      return changed ? next : current
    })
  }, [events])

  const requestSnapshot = useCallback((): void => {
    if (!api) return
    void api.invoke('core.getHiveState', {}).catch(() => undefined)
  }, [api])

  // Snapshot polling is read-only and bounded by the chosen interval. EvaPulse
  // here is the dashboard's live mode: it never starts or stops the hive.
  useEffect(() => {
    if (!api || !connected || !preference.pulse) return
    requestSnapshot()
    const timer = window.setInterval(requestSnapshot, preference.intervalMs)
    return () => window.clearInterval(timer)
  }, [api, connected, preference.intervalMs, preference.pulse, requestSnapshot])

  useEffect(() => {
    if (!api) return
    void api.invoke('provider.get', {}).then((outcome) => {
      if (outcome.ok) setProvider(outcome.value)
    })
  }, [api])

  useEffect(() => {
    if (!api || !chatId) {
      setHistoryPrompts([])
      return
    }
    let cancelled = false
    void api.invoke('chat.open', { chatId }).then((outcome) => {
      if (cancelled || !outcome.ok || !outcome.value) return
      const loaded = outcome.value.messages.map((message, index) => ({
        taskId: message.taskId,
        text: message.prompt,
        sequenceHint: index + 1
      }))
      setHistoryPrompts(loaded)
    }).catch(() => undefined)
    return () => {
      cancelled = true
    }
  }, [api, chatId])

  useEffect(() => {
    if (pendingTaskIds.size === 0) return
    const terminalTypes = new Set(['task.completed', 'task.failed', 'task.stopped'])
    setPendingTaskIds((current) => {
      let changed = false
      const next = new Set(current)
      for (const taskId of current) {
        if (events.some((event) => event.taskId === taskId && terminalTypes.has(event.eventType))) {
          next.delete(taskId)
          changed = true
        }
      }
      return changed ? next : current
    })
  }, [events, pendingTaskIds.size])

  const activeApprovals = useMemo(() => {
    const terminalTypes = new Set(['task.completed', 'task.failed', 'task.stopped'])
    const resolved = new Set([...resolvedApprovalIds, ...resolvedLocally])
    const byId = new Map<string, { request: ApprovalRequestView; sequence: number }>()
    const newestFirst = [...events].sort((left, right) => right.sequenceId - left.sequenceId)
    for (const event of newestFirst) {
      if (event.eventType !== 'approval.required') continue
      if (events.some((candidate) => candidate.taskId === event.taskId && candidate.sequenceId > event.sequenceId && terminalTypes.has(candidate.eventType))) continue
      const request = parseApprovalRequest(event)
      if (!request || resolved.has(request.approvalId)) continue
      if (!byId.has(request.approvalId)) byId.set(request.approvalId, { request, sequence: event.sequenceId })
    }
    return [...byId.values()].map((item) => item.request)
  }, [events, resolvedApprovalIds, resolvedLocally])

  const approvalResolved = useCallback(async (approval: ApprovalRequestView, granted: boolean) => {
    if (!api || resolvingApprovalIds.has(approval.approvalId)) return
    setResolvingApprovalIds((current) => new Set(current).add(approval.approvalId))
    setCommandError(null)
    try {
      const outcome = await api.invoke('core.resolveApproval', {
        approvalId: approval.approvalId,
        granted,
        idempotencyKey: `approval:${approval.approvalId}:${granted ? 'grant' : 'reject'}`,
        ...(granted ? {} : { rejectionReason: 'rejected_by_user' })
      })
      if (outcome.ok) {
        setResolvedLocally((current) => current.includes(approval.approvalId) ? current : [...current, approval.approvalId])
        onApprovalResolved?.(approval.approvalId)
      } else {
        setCommandError(outcome.message)
      }
    } catch {
      setCommandError('Не удалось отправить решение в Core; запрос остаётся ожидающим.')
    } finally {
      setResolvingApprovalIds((current) => {
        const next = new Set(current)
        next.delete(approval.approvalId)
        return next
      })
    }
  }, [api, onApprovalResolved, resolvingApprovalIds])

  const providerOptionList = useMemo(() => providerOptions(provider), [provider])
  const selectedProviderKey = currentProviderKey(provider)

  const selectProvider = useCallback(async (key: string): Promise<void> => {
    if (!api || providerBusy) return
    const option = providerOptionList.find((candidate) => candidate.key === key)
    if (!option || !option.configured) return
    setProviderBusy(true)
    setProviderMessage(null)
    try {
      const outcome = await api.invoke('provider.select', option.profileId
        ? { provider: option.provider, profileId: option.profileId }
        : { provider: option.provider })
      if (outcome.ok) {
        setProvider(outcome.value.summary)
        setProviderMessage(outcome.value.restarted ? 'Core перезапущен с выбранным провайдером.' : null)
      } else {
        setProviderMessage(outcome.message)
      }
    } catch {
      setProviderMessage('Не удалось выбрать провайдера: оболочка не ответила.')
    } finally {
      setProviderBusy(false)
    }
  }, [api, providerBusy, providerOptionList])

  const updatePreference = useCallback((next: HivePreference): void => {
    setPreference(next)
    saveHivePreference(next)
  }, [])

  const saveEvaWorkspacePath = useCallback(async (): Promise<void> => {
    if (!api) return
    setEvaWorkspaceBusy(true)
    setEvaWorkspaceError(null)
    try {
      const submittedPath = evaWorkspaceDraft.trim()
      const outcome = await api.invoke('core.setEvaWorkspacePath', { path: submittedPath })
      if (!outcome.ok) {
        setEvaWorkspacePath('')
        setEvaWorkspaceDraft('')
        setEvaWorkspaceError(outcome.message)
        return
      }
      if (submittedPath.length === 0) {
        setEvaWorkspacePath('')
        setEvaWorkspaceDraft('')
        setEvaWorkspaceError('Путь не задан; инструменты саморазвития выключены.')
        return
      }
      setEvaWorkspaceError('Путь отправлен в Core для проверки…')
    } catch {
      setEvaWorkspaceError('Не удалось сохранить путь репозитория: Core не ответил.')
    } finally {
      setEvaWorkspaceBusy(false)
    }
  }, [api, evaWorkspaceDraft, events])

  // The switch is a Core runtime control, not only a local display option:
  // false parks the daemon, true lets it run a cycle immediately. The local
  // preference still owns the snapshot polling interval in this window.
  const togglePulse = useCallback((enabled: boolean): void => {
    updatePreference({ ...preference, pulse: enabled })
    if (!api) return
    setCommandError(null)
    void api.invoke('core.togglePulse', { enabled }).then((outcome) => {
      if (!outcome.ok) setCommandError(outcome.message)
    }).catch(() => setCommandError('Не удалось переключить EvaPulse: оболочка не ответила.'))
  }, [api, preference, updatePreference])

  const sendHiveMessage = useCallback(async (): Promise<void> => {
    const text = chatDraft.trim()
    if (!api || !connected || sendingMessage || text.length === 0) return
    const nextTaskId = typeof globalThis.crypto?.randomUUID === 'function'
      ? `hive-${globalThis.crypto.randomUUID()}`
      : `hive-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`
    const clientMessageId = typeof globalThis.crypto?.randomUUID === 'function'
      ? globalThis.crypto.randomUUID()
      : `msg-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`
    setSendingMessage(true)
    setCommandError(null)
    try {
      let targetChatId = chatId
      if (targetChatId === null) {
        const created = await api.invoke('chat.create', { workspacePath: workspace ?? null })
        if (!created.ok) {
          setCommandError(created.message)
          return
        }
        targetChatId = created.value.id
        onChatOpened?.(targetChatId)
      }

      setLocalPrompts((current) => [
        ...current,
        { taskId: nextTaskId, text, sequenceHint: current.length + 1 }
      ])
      setPendingTaskIds((current) => new Set(current).add(nextTaskId))
      setChatDraft('')

      const outcome = await api.invoke('core.startTask', {
        taskId: nextTaskId,
        prompt: text,
        workspacePath: workspace ?? '',
        conversationId: targetChatId,
        clientMessageId,
        preferredRouteHint: 'cloud',
        executionKind: 'dialogue'
      })
      if (!outcome.ok) {
        setPendingTaskIds((current) => {
          const next = new Set(current)
          next.delete(nextTaskId)
          return next
        })
        setCommandError(outcome.message)
        return
      }

      const stored = await api.invoke('chat.appendPrompt', {
        chatId: targetChatId,
        taskId: nextTaskId,
        clientMessageId,
        prompt: text
      })
      if (stored.ok && stored.value) {
        setHistoryPrompts(
          stored.value.messages.map((message, index) => ({
            taskId: message.taskId,
            text: message.prompt,
            sequenceHint: index + 1
          }))
        )
      }
      onChatTouched?.()
    } catch {
      setPendingTaskIds((current) => {
        const next = new Set(current)
        next.delete(nextTaskId)
        return next
      })
      setCommandError('Не удалось отправить сообщение Королеве Еве.')
    } finally {
      setSendingMessage(false)
    }
  }, [api, chatDraft, chatId, connected, onChatOpened, onChatTouched, sendingMessage, workspace])

  const budget = hive?.budget ?? EMPTY_BUDGET
  const departments = useMemo(() => departmentView(hive?.departments ?? []), [hive])
  const agentsOnline = hive?.metrics.totalAgentsOnline ?? 0
  const activeTasks = hive?.metrics.activeTasks ?? 0
  const efficiency = hive?.metrics.efficiencyPercent ?? 0
  const hiveCapacity = Math.max(
    0,
    ...departments.map((department) => department.maxCapacity),
    hive?.limits.maxSubagentsPerWorker ?? 0
  )
  const capacityLabel = hiveCapacity > 0 ? hiveCapacity : agentsOnline
  const paused = hive?.queen.status === 'budget_exhausted' && budget.historyAvailable
  const queenActive = hive?.queen.status === 'active'
  const waitingApproval = activeApprovals.length > 0
  const queenStatusLabel = paused
    ? 'Спит'
    : waitingApproval
      ? 'Ждёт подтверждения'
      : queenActive
        ? 'Активна'
        : 'Спит'
  const wakeAtMs = hive?.queen.wakeAtMs ?? null
  const pulseNextWakeAtMs = hive?.nextWakeAtMs ?? null
  const pulseStatus = hive?.pulseStatus ?? ''
  const pulseStatusLabel = pulseStatus === 'active'
    ? 'EvaPulse активен'
    : pulseStatus === 'sleeping'
      ? 'EvaPulse спит'
      : pulseStatus === 'disabled'
        ? 'EvaPulse выключен'
        : 'EvaPulse: нет данных Core'
  const averageTaskTimeSec = hive?.metrics.averageTaskTimeSec ?? 0
  const averageTimeLabel = `${averageTaskTimeSec}с`
  const dailyBudgetUsd = hive?.dailyBudgetUsd ?? 0
  const spentUsd = budget.dailyCostAvailable && budget.dailyCostNanoUsd !== null ? budget.dailyCostNanoUsd / 1_000_000_000 : null
  const spendPercent = spentUsd === null || dailyBudgetUsd <= 0 ? 0 : Math.min(100, Math.round((spentUsd / dailyBudgetUsd) * 100))
  const remainingUsd = Math.max(0, dailyBudgetUsd - (spentUsd ?? 0))
  const spentLabel = budget.dailyCostAvailable && budget.dailyCostNanoUsd !== null ? formatMoney(budget.dailyCostNanoUsd) : '—'
  const requestSummary = [
    `${budget.historyAvailable ? budget.requestsLastMinute : '—'}${budget.minuteCap > 0 ? ` / ${budget.minuteCap}` : ''} ${translate('за минуту')}`,
    `${budget.historyAvailable ? budget.requestsLastHour : '—'}${budget.hourCap > 0 ? ` / ${budget.hourCap}` : ''} ${translate('за час')}`,
    `${budget.historyAvailable ? budget.requestsLastDay : '—'}${budget.dayCap > 0 ? ` / ${budget.dayCap}` : ''} ${translate('за сутки')}`
  ].join(' · ')
  const configuredCaps = budget.minuteCap > 0 || budget.hourCap > 0 || budget.dayCap > 0
  const snapshotBudgetLabel = budget.provider === 'literouter' || budget.provider === 'openrouter'
    ? `LiteRouter${budget.model.toLowerCase().includes(':free') ? ' (Free)' : ''}`
    : budget.provider.toLowerCase().includes('deepseek')
      ? 'DeepSeek API'
      : budget.provider || translate('провайдер не выбран')
  const realLogs = hiveLogEntries(events, observedAt, allLogs ? 40 : 6)
  const logs = realLogs
  const combinedPrompts = useMemo(() => {
    const byTask = new Map<string, HiveUserPrompt>()
    for (const item of historyPrompts) {
      byTask.set(item.taskId || `hist-${item.sequenceHint}`, item)
    }
    for (const item of localPrompts) {
      if (!byTask.has(item.taskId)) {
        byTask.set(item.taskId || `local-${item.sequenceHint}`, item)
      }
    }
    return [...byTask.values()]
  }, [historyPrompts, localPrompts])
  const hiveChatMessages = useMemo(
    () => extractHiveChatMessages(events, combinedPrompts, 5),
    [combinedPrompts, events]
  )
  const activeApprovalTaskIds = useMemo(
    () => new Set(activeApprovals.map((approval) => approval.taskId).filter(Boolean)),
    [activeApprovals]
  )
  const queenTyping = useMemo(
    () => isQueenThinking(events, pendingTaskIds, sendingMessage, activeApprovalTaskIds),
    [activeApprovalTaskIds, events, pendingTaskIds, sendingMessage]
  )

  return (
    <section className="hive-console" aria-label={translate('Улей (Hive)')}>
      <header className="hive-console__header">
        <div className="hive-console__title">
          <p className="hive-console__eyebrow">Hive / Eva</p>
          <h1>{translate('Улей (Hive)')}</h1>
          <p>{translate('Единый центр управления Ульем и Королевой Евой')}</p>
        </div>
        <div className="hive-console__controls">
          <span className={`status-pill status-pill--${connection}`}>{translate(connection)}</span>
          <div className="hive-switch">
            <span className="hive-switch__label" id="hive-pulse-label">{translate('Авто-режим (EvaPulse)')}</span>
            <button
              type="button"
              role="switch"
              aria-checked={preference.pulse}
              aria-labelledby="hive-pulse-label"
              className={`hive-switch__control${preference.pulse ? ' hive-switch__control--on' : ''}`}
              onClick={() => togglePulse(!preference.pulse)}
            >
              <span aria-hidden="true" />
            </button>
            <small className="hive-switch__status" aria-live="polite">{translate(pulseStatusLabel)}</small>
          </div>
          <label className="hive-interval">
            <span>{translate('Обновление')}</span>
            <select
              aria-label={translate('Интервал обновления')}
              value={preference.intervalMs}
              onChange={(changeEvent) => updatePreference({ ...preference, intervalMs: Number(changeEvent.target.value) })}
            >
              {HIVE_INTERVALS.map((value) => (
                <option key={value} value={value}>{value / 1000}с</option>
              ))}
            </select>
          </label>
          {!preference.pulse ? (
            <button type="button" className="hive-console__refresh" onClick={requestSnapshot} disabled={!connected}>
              {translate('Обновить сейчас')}
            </button>
          ) : null}
        </div>
      </header>

      {activeApprovals.map((approval) => (
        <ApprovalRequest
          key={approval.approvalId}
          request={approval}
          busy={resolvingApprovalIds.has(approval.approvalId)}
          onResolve={(granted) => void approvalResolved(approval, granted)}
        />
      ))}
      {commandError ? <p role="alert" className="hive-console__error">{translate(commandError)}</p> : null}

      <div className="hive-console__grid">
        <section className="hive-panel hive-panel--departments" aria-label={translate('Департаменты улья')}>
          <div className="hive-panel__head">
            <h2>{translate('Департаменты улья')}</h2>
            <span>{translate('Активно:')} {agentsOnline} / {capacityLabel}</span>
          </div>
          <div className="hive-departments">
            {departments.map((department) => {
              const percent = hivePercent(department.activeCount, department.maxCapacity)
              return (
                <article key={department.key} className="hive-department">
                  <div className="hive-department__head">
                    <span className="hive-department__icon" aria-hidden="true"><UiIcon name={department.icon} size={16} /></span>
                    <strong>{translate(department.label)}</strong>
                  </div>
                  <div className="hive-department__count">
                    <span>{department.activeCount} / {department.maxCapacity}</span>
                    <em>{percent}%</em>
                  </div>
                  <div
                    className="hive-progress"
                    role="progressbar"
                    aria-label={translate(department.label)}
                    aria-valuemin={0}
                    aria-valuemax={Math.max(department.maxCapacity, 1)}
                    aria-valuenow={department.activeCount}
                  >
                    <span style={{ width: `${percent}%` }} />
                  </div>
                </article>
              )
            })}
          </div>
        </section>

        <section className={`hive-queen${queenActive ? '' : ' hive-queen--sleeping'}`} aria-label={translate('Королева Ева')}>
          <div className="hive-queen__stage">
            <HiveGraph
              queenName={hive?.queen.name || 'EvoHime'}
              queenActive={queenActive}
              activeAgents={hive?.activeAgents ?? []}
            />
            <div className="hive-queen__body hive-queen__body--overlay">
              <div className="hive-queen__header-row">
                <div>
                  <p className="hive-queen__eyebrow">Eva core · {hive?.queen.name || 'EvoHime'}</p>
                  <h2>{translate('Королева Ева')}</h2>
                  <p className="hive-queen__role">{translate('Главный оркестратор улья')}</p>
                </div>
                <p className={`hive-queen__status hive-queen__status--${queenActive ? 'active' : 'sleeping'}`}>
                  <span className="hive-dot" aria-hidden="true" />
                  {translate(queenStatusLabel)}
                </p>
              </div>
              <p className="hive-queen__note">
                {paused
                  ? `${translate(QUEEN_REASON_LABELS[hive?.queen.reason ?? ''] ?? hive?.queen.reason ?? 'лимит провайдера')} · ${formatWakeAt(wakeAtMs, Date.now())}`
                  : translate('Координирует агентов и выполняет задачи')}
              </p>
            </div>
          </div>
          <dl className="hive-queen__footer">
            <div>
              <dt>{translate('Воркеры')}</dt>
              <dd>{agentsOnline} / {capacityLabel}</dd>
            </div>
            <div>
              <dt>{translate('Следующий сон')}</dt>
              <dd>{pulseNextWakeAtMs !== null ? formatClock(pulseNextWakeAtMs) : paused && wakeAtMs !== null ? formatClock(wakeAtMs) : '—'}</dd>
            </div>
            <div>
              <dt>{translate('EvaPulse')}</dt>
              <dd>{translate(pulseStatusLabel)}</dd>
            </div>
          </dl>
        </section>

        <aside className="hive-panel hive-panel--metrics" aria-label={translate('Метрики улья')}>
          <div className="hive-panel__head">
            <h2>{translate('Метрики улья')}</h2>
          </div>
          <div className="hive-metrics">
            <EfficiencyRing percent={efficiency} />
            <span className="hive-metrics__caption">{translate('Эффективность')}</span>
          </div>
          <ul className="hive-stat-list">
            <li><span>{translate('Агентов онлайн')}</span><strong>{agentsOnline}</strong></li>
            <li><span>{translate('Задач в работе')}</span><strong>{activeTasks}</strong></li>
            <li>
              <span>{translate('Задач сегодня')}</span>
              <strong>{hive?.metrics.tasksToday ?? 0}</strong>
            </li>
            <li>
              <span>{translate('Среднее время')}</span>
              <strong>{averageTimeLabel}</strong>
            </li>
          </ul>

          <div className="hive-budget-card">
            <div className="hive-budget-card__head">
              <h3>{translate('Бюджет и провайдер')}</h3>
            </div>
            <div className="hive-budget-card__spent">
              <span>{translate('Потрачено сегодня')}</span>
              <strong>{spentLabel} / ${dailyBudgetUsd.toFixed(2)}</strong>
            </div>
            <div
              className="hive-progress"
              role="progressbar"
              aria-label={translate('Расход дневного бюджета')}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={spendPercent}
            >
              <span style={{ width: `${spendPercent}%` }} />
            </div>
            <label className="hive-provider">
              <span>{translate('Текущий провайдер')}</span>
              <select
                aria-label={translate('Провайдер улья')}
                value={selectedProviderKey}
                disabled={providerBusy || providerOptionList.length === 0}
                onChange={(changeEvent) => void selectProvider(changeEvent.target.value)}
              >
                {providerOptionList.length === 0
                  ? <option value="">{translate(snapshotBudgetLabel)}</option>
                  : providerOptionList.map((option) => (
                    <option key={option.key} value={option.key} disabled={!option.configured}>
                      {option.label}{option.configured ? '' : ` · ${translate('ключ не настроен')}`}
                    </option>
                  ))}
              </select>
            </label>
            <p className="hive-budget-card__limit">
              {translate('Лимит:')} ${dailyBudgetUsd.toFixed(2)}{translate('/день')} • {translate('Остаток:')} ${remainingUsd.toFixed(2)}
            </p>
            <p className="hive-budget-card__requests">
              {translate('Лимит запросов провайдера')}: {requestSummary}
            </p>
            <small>
              {budget.model ? `${translate('Модель:')} ${budget.model}` : translate('Модель ещё не известна')}
              {!configuredCaps ? ` · ${translate('лимиты запросов не настроены')}` : ''}
              {!budget.historyAvailable ? ` · ${translate('история отправок недоступна')}` : ''}
            </small>
            {budget.exhausted ? <p className="hive-budget-card__notice">{translate('Лимит запросов исчерпан — Core приостановит отправку до сброса окна.')}</p> : null}
            {providerMessage ? <small className="hive-budget-card__message">{translate(providerMessage)}</small> : null}
          </div>
        </aside>
      </div>

      <section className="hive-panel" aria-label={translate('Настройка саморазвития')}>
        <div className="hive-panel__head">
          <h2>{translate('Саморазвитие Евы')}</h2>
          <span className="hive-badge">
            {translate(evaWorkspacePath ? 'включено' : 'выключено')}
          </span>
        </div>
        <p>{translate('Укажи отдельный изолированный Git checkout. Ева сможет исследовать и исправлять только его; Core не изменяет установленный продукт.')}</p>
        <label className="hive-provider">
          <span>{translate('Путь к Git-репозиторию')}</span>
          <input
            type="text"
            aria-label={translate('Путь к Git-репозиторию саморазвития')}
            value={evaWorkspaceDraft}
            onChange={(changeEvent) => setEvaWorkspaceDraft(changeEvent.target.value)}
            placeholder="C:\\EvoHime-self-development"
          />
        </label>
        <div className="hive-console__controls">
          <button type="button" disabled={!connected || evaWorkspaceBusy} onClick={() => void saveEvaWorkspacePath()}>
            {translate(evaWorkspaceBusy ? 'Сохранение…' : 'Сохранить путь')}
          </button>
          <small>{translate(evaWorkspacePath ? 'Инструменты eva_workspace_* доступны Королеве Еве.' : 'Путь не задан — инструменты саморазвития недоступны.')}</small>
        </div>
        {evaWorkspaceError ? <p role="alert" className="hive-console__error">{translate(evaWorkspaceError)}</p> : null}
      </section>

      <div className="hive-console__bottom">
        <section className="hive-panel hive-panel--agents" aria-label={translate('Активные агенты')}>
          <div className="hive-panel__head">
            <h2>{translate('Активные агенты')} ({agentsOnline})</h2>
            <span>{translate('глубина и подагенты')}</span>
          </div>
          {hive && hive.activeAgents.length > 0 ? (
            <ul className="hive-agent-list">
              {hive.activeAgents.slice(0, 8).map((agent) => (
                <li key={agent.id}>
                  <span className="hive-agent__avatar" aria-hidden="true">{agentMonogram(agent.id)}</span>
                  <div className="hive-agent__body">
                    <div className="hive-agent__topline">
                      <strong>{agent.id || 'worker'}</strong>
                      <span className="hive-role-badge">{translate(departmentLabel(agent.role))}</span>
                    </div>
                    <small>{translate(agent.currentAction) || translate('ожидание')}</small>
                  </div>
                  <span className="hive-agent__meta">{agent.spawnedSubagents} / {agent.ceiling}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="hive-empty">
              {connected
                ? translate('Активных агентов нет.')
                : translate('Core недоступен — ожидается актуальная проекция.')}
            </p>
          )}
        </section>

        <section className="hive-panel hive-panel--logs" aria-label={translate('Логи улья')}>
          <div className="hive-panel__head">
            <h2>{translate('Логи улья')}</h2>
            <div className="hive-panel__actions">
              {logs.length === 0 ? <span className="hive-badge">{translate('нет событий')}</span> : null}
              <button
                type="button"
                className="hive-link-button"
                aria-pressed={allLogs}
                onClick={() => setAllLogs((value) => !value)}
              >
                {translate(allLogs ? 'Свернуть' : 'Все логи')}
              </button>
            </div>
          </div>
          <ol className="hive-log-list">
            {logs.map((entry) => (
              <li key={entry.id} className={`hive-log-row hive-log-row--${entry.level}`}>
                <span className="hive-log-row__icon" aria-hidden="true"><UiIcon name={HIVE_LOG_ICONS[entry.level]} size={14} /></span>
                <time className="hive-log-row__time">{entry.at}</time>
                <span className="hive-log-row__text">{translate(entry.text)}</span>
              </li>
            ))}
          </ol>
          {logs.length === 0 ? <p className="hive-empty">{translate('Событий пока нет.')}</p> : null}
        </section>
      </div>

      <section className="hive-panel hive-chat" aria-label={translate('Диалог с Королевой Евой')}>
        <div className="hive-panel__head">
          <h2>{translate('Диалог с Королевой Евой')}</h2>
          <span className="hive-badge">
            {translate(queenTyping ? 'в процессе' : 'на связи')}
          </span>
        </div>

        <div className="hive-chat__messages" role="log" aria-live="polite">
          {hiveChatMessages.length === 0 && activeApprovals.length === 0 && !queenTyping ? (
            <p className="hive-empty">{translate('Напиши Королеве Еве — здесь отобразятся последние сообщения диалога.')}</p>
          ) : null}
          {hiveChatMessages.map((message) => (
            <div
              key={message.id}
              className={`hive-chat__message hive-chat__message--${message.role}${message.failed ? ' hive-chat__message--failed' : ''}`}
            >
              <span className="hive-chat__author">
                {translate(message.role === 'user' ? 'Вы' : 'Королева Ева')}
              </span>
              <p className="hive-chat__text">{message.text}</p>
            </div>
          ))}
          {activeApprovals.map((approval) => (
            <div key={`chat-approval-${approval.approvalId}`} className="hive-chat__approval" data-testid="hive-chat-approval">
              <div className="hive-chat__approval-body">
                <span className="hive-chat__author">{translate('Запрос подтверждения')}</span>
                <p className="hive-chat__text">{approval.summary}</p>
                <small>{approval.toolName} · {approval.permission} · {approval.scope}</small>
              </div>
              <div className="hive-approval__actions">
                <button
                  type="button"
                  className="hive-button hive-button--approve"
                  disabled={resolvingApprovalIds.has(approval.approvalId)}
                  onClick={() => void approvalResolved(approval, true)}
                >
                  {translate('Разрешить')}
                </button>
                <button
                  type="button"
                  className="hive-button hive-button--reject"
                  disabled={resolvingApprovalIds.has(approval.approvalId)}
                  onClick={() => void approvalResolved(approval, false)}
                >
                  {translate('Отклонить')}
                </button>
              </div>
            </div>
          ))}
          {queenTyping ? (
            <div className="hive-chat__typing" data-testid="queen-typing-indicator" role="status" aria-live="polite">
              <span>{translate('Королева печатает')}</span>
              <span className="hive-chat__dots" aria-hidden="true">
                <span />
                <span />
                <span />
              </span>
            </div>
          ) : null}
        </div>

        <form
          className="hive-chat__composer"
          onSubmit={(submitEvent) => {
            submitEvent.preventDefault()
            void sendHiveMessage()
          }}
        >
          <input
            type="text"
            className="hive-chat__input"
            aria-label={translate('Сообщение Королеве Еве')}
            placeholder={translate('Напиши задачу или вопрос Королеве Еве...')}
            value={chatDraft}
            onChange={(changeEvent) => setChatDraft(changeEvent.target.value)}
            disabled={!connected || sendingMessage}
          />
          <button
            type="submit"
            className="hive-chat__send"
            disabled={!connected || sendingMessage || chatDraft.trim().length === 0}
          >
            {translate('Отправить')}
          </button>
        </form>
      </section>
    </section>
  )
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
