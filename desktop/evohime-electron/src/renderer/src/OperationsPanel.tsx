import { translate } from './i18n'
import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react'

import { HIVE_SYSTEM_CHAT_ID } from '@shared/api'
import type { AmbientProposal, AmbientProposalList, ChatProviderMode, ConnectionState, ConversationEventProjection, CoreEvent, ModelTier, ProviderLimits, ProviderSummary, RepairStatus } from '@shared/api'

import { useShellApi } from './shell-api'
import { useProviderState } from './provider-state'
import { ModelPicker } from './ModelPicker'
import { ImageGenerationPanel } from './ImageGenerationPanel'
import { AnalysisKernelPanel } from './AnalysisKernelPanel'
import { UiIcon } from './UiIcon'
import type { UiIconName } from './UiIcon'
import { HiveCityScene } from './components/3d'
import { catalogErrorMessage, safeCatalogErrorCode } from './catalog-error'
import {
  applyConversationEvents,
  applyInitialConversationHistory,
  conversationEvents,
  createConversationProjection,
  prependConversationEvents,
  resumeAtRetainedBoundary
} from './conversation-projection'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly repair?: RepairStatus | null
  readonly resolvedApprovalIds?: readonly string[]
  readonly onApprovalResolved?: (approvalId: string) => void
}

interface HiveDashboardProps extends Pick<Props, 'connection' | 'events' | 'resolvedApprovalIds' | 'onApprovalResolved'> {
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
    readonly limits: ProviderLimits
    readonly minuteExhausted: boolean
    readonly exhausted: boolean
    readonly dailyCostAvailable: boolean
    readonly dailyCostNanoUsd: number | null
    readonly dailyTokenQuota: number | null
    readonly dailyTokensRemaining: number | null
    readonly quotaResetUtcMidnight: boolean
    readonly fallbackProvider: string | null
    readonly fallbackModel: string | null
    readonly fallbackTokensRemaining: number | null
    readonly fallbackDailyTokenQuota: number | null
    readonly fallbackQuotaResetUtcMidnight: boolean
  }
  readonly dailyBudgetUsd: number
  readonly evaWorkspacePath: string | null
  readonly watchedRepositories: readonly {
    readonly path: string
    readonly status: 'idle' | 'scanning' | 'working' | 'error'
    readonly lastScannedAtMs: number | null
    readonly currentTask: string | null
  }[]
  readonly hiveProviderProfile: string | null
  readonly hiveModel: string | null
  readonly hiveFallbackProviderProfile: string | null
  readonly hiveFallbackModel: string | null
  readonly hiveProviderLimits: ProviderLimits
  readonly hiveFallbackProviderLimits: ProviderLimits
  readonly hiveDailyTokenQuota: number
  readonly hiveFallbackDailyTokenQuota: number
  readonly hiveTokenQuotaConfigured: boolean
  readonly hiveFallbackTokenQuotaConfigured: boolean
  readonly hiveQuotaResetUtcMidnight: boolean
  readonly hiveFallbackQuotaResetUtcMidnight: boolean
  readonly proactiveEvolutionEnabled: boolean
  readonly observationStatus: string
  readonly nextWakeAtMs: number | null
}

/** Normalized view of the independent `hive.observation_status` projection. */
interface HiveObservationStateView {
  readonly status: string
  readonly enabled: boolean
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
  if (version !== 1 && version !== 2 && version !== 3 && version !== 4 && version !== 5 && version !== 6 && version !== 7 && version !== 8) return null
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
  const rawLimits = record(budget['limits'])
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
      limits: {
        rpm: rawLimits['rpm'] !== undefined ? count(rawLimits['rpm'], 1_000_000) : count(budget['minute_cap'], 1_000_000),
        rph: rawLimits['rph'] !== undefined ? count(rawLimits['rph'], 1_000_000) : count(budget['hour_cap'], 1_000_000),
        rpd: rawLimits['rpd'] !== undefined ? count(rawLimits['rpd'], 1_000_000) : count(budget['day_cap'], 1_000_000)
      },
      minuteExhausted: budget['minute_exhausted'] === true,
      exhausted: budget['history_available'] === true && (budget['minute_exhausted'] === true || budget['hour_exhausted'] === true || budget['day_exhausted'] === true),
      dailyCostAvailable: budget['daily_cost_available'] === true,
      dailyCostNanoUsd: typeof budget['daily_cost_nano_usd'] === 'number' && Number.isSafeInteger(budget['daily_cost_nano_usd']) && budget['daily_cost_nano_usd'] >= 0
        ? budget['daily_cost_nano_usd']
        : null,
      dailyTokenQuota: typeof budget['daily_token_quota'] === 'number' && Number.isSafeInteger(budget['daily_token_quota']) && budget['daily_token_quota'] >= 0
        ? budget['daily_token_quota']
        : null,
      dailyTokensRemaining: typeof budget['daily_tokens_remaining'] === 'number' && Number.isSafeInteger(budget['daily_tokens_remaining']) && budget['daily_tokens_remaining'] >= 0
        ? budget['daily_tokens_remaining']
        : null,
      quotaResetUtcMidnight: budget['quota_reset_utc_midnight'] === true,
      fallbackProvider: typeof budget['fallback_provider'] === 'string' && budget['fallback_provider'].length > 0
        ? text(budget['fallback_provider'], 64)
        : null,
      fallbackModel: typeof budget['fallback_model'] === 'string' && budget['fallback_model'].trim().length > 0
        ? text(budget['fallback_model'], 128)
        : null,
      fallbackTokensRemaining: typeof budget['fallback_tokens_remaining'] === 'number' && Number.isSafeInteger(budget['fallback_tokens_remaining']) && budget['fallback_tokens_remaining'] >= 0
        ? budget['fallback_tokens_remaining']
        : null,
      fallbackDailyTokenQuota: typeof budget['fallback_daily_token_quota'] === 'number' && Number.isSafeInteger(budget['fallback_daily_token_quota']) && budget['fallback_daily_token_quota'] >= 0
        ? budget['fallback_daily_token_quota']
        : null,
      fallbackQuotaResetUtcMidnight: budget['fallback_quota_reset_utc_midnight'] === true
    },
    dailyBudgetUsd: typeof root['daily_budget_usd'] === 'number' && Number.isFinite(root['daily_budget_usd']) && root['daily_budget_usd'] >= 0
      ? Math.min(root['daily_budget_usd'], 10_000)
      : 0,
    evaWorkspacePath: typeof root['eva_workspace_path'] === 'string' ? root['eva_workspace_path'].slice(0, 4_096) : null,
     watchedRepositories: Array.isArray(root['watched_repositories'])
       ? root['watched_repositories'].flatMap((raw) => {
           if (typeof raw === 'string') {
             return raw.trim().length > 0 ? [{ path: raw.slice(0, 4_096), status: 'idle' as const, lastScannedAtMs: null, currentTask: null }] : []
           }
           if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) return []
           const item = raw as Record<string, unknown>
           const path = typeof item['path'] === 'string' ? item['path'].trim().slice(0, 4_096) : ''
           if (!path) return []
           const status: HiveStateView['watchedRepositories'][number]['status'] = item['status'] === 'scanning' || item['status'] === 'working' || item['status'] === 'error' ? item['status'] : 'idle'
           return [{
             path,
             status,
             lastScannedAtMs: typeof item['last_scanned_at_ms'] === 'number' && Number.isSafeInteger(item['last_scanned_at_ms']) ? item['last_scanned_at_ms'] : null,
             currentTask: typeof item['current_task'] === 'string' && item['current_task'].trim().length > 0 ? item['current_task'].slice(0, 180) : null
           }]
         }).filter((item) => {
           const eva = typeof root['eva_workspace_path'] === 'string' ? root['eva_workspace_path'] : ''
           return !sameRepositoryPath(item.path, eva)
         })
       : [],
    hiveProviderProfile: typeof root['hive_provider_profile'] === 'string' && root['hive_provider_profile'].trim().length > 0
      ? root['hive_provider_profile'].slice(0, 128)
      : null,
    hiveModel: typeof root['hive_model'] === 'string' && root['hive_model'].trim().length > 0
      ? root['hive_model'].slice(0, 128)
      : null,
    hiveFallbackProviderProfile: typeof root['hive_fallback_provider_profile'] === 'string' && root['hive_fallback_provider_profile'].trim().length > 0
      ? root['hive_fallback_provider_profile'].slice(0, 128)
      : null,
    hiveFallbackModel: typeof root['hive_fallback_model'] === 'string' && root['hive_fallback_model'].trim().length > 0
      ? root['hive_fallback_model'].slice(0, 128)
      : null,
    hiveProviderLimits: {
      rpm: count(record(root['hive_provider_limits'])['rpm'], 10_000),
      rph: count(record(root['hive_provider_limits'])['rph'], 10_000),
      rpd: count(record(root['hive_provider_limits'])['rpd'], 10_000)
    },
    hiveFallbackProviderLimits: {
      rpm: count(record(root['hive_fallback_provider_limits'])['rpm'], 10_000),
      rph: count(record(root['hive_fallback_provider_limits'])['rph'], 10_000),
      rpd: count(record(root['hive_fallback_provider_limits'])['rpd'], 10_000)
    },
    hiveDailyTokenQuota: count(root['hive_daily_token_quota'], 1_000_000_000_000),
    hiveFallbackDailyTokenQuota: count(root['hive_fallback_daily_token_quota'], 1_000_000_000_000),
    hiveTokenQuotaConfigured: root['hive_token_quota_configured'] === true,
    hiveFallbackTokenQuotaConfigured: root['hive_fallback_token_quota_configured'] === true,
    hiveQuotaResetUtcMidnight: root['hive_quota_reset_utc_midnight'] === true,
    hiveFallbackQuotaResetUtcMidnight: root['hive_fallback_quota_reset_utc_midnight'] === true,
    proactiveEvolutionEnabled: root['proactive_evolution_enabled'] === true,
    observationStatus: text(root['observation_status'] ?? root['pulse_status'], 32),
    nextWakeAtMs: typeof root['next_wake_at_ms'] === 'number' && Number.isSafeInteger(root['next_wake_at_ms'])
      ? root['next_wake_at_ms']
      : null
  }
}

function parseHiveObservationState(event: CoreEvent | undefined): HiveObservationStateView | null {
  if (!event) return null
  try {
    const parsed = JSON.parse(event.payload) as Record<string, unknown>
    if (parsed['schema_version'] !== 1 || parsed['owner'] !== 'hive_observation') return null
    const status = parsed['status']
    if (typeof status !== 'string' || status.length > 32) return null
    const nextWakeAtMs = parsed['next_wake_at_ms']
    return {
      status: status.slice(0, 32),
      enabled: parsed['enabled'] === true,
      nextWakeAtMs: typeof nextWakeAtMs === 'number' && Number.isSafeInteger(nextWakeAtMs)
        ? nextWakeAtMs
        : null
    }
  } catch {
    return null
  }
}

function normalizeRepositoryPath(raw: string): string {
  return raw.trim().replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase()
}

function sameRepositoryPath(left: string, right: string): boolean {
  const a = normalizeRepositoryPath(left)
  const b = normalizeRepositoryPath(right)
  return a.length > 0 && a === b
}

function shortRepositoryPath(path: string): string {
  const parts = path.replace(/\\/g, '/').split('/').filter(Boolean)
  return parts.length > 2 ? `…/${parts.slice(-2).join('/')}` : path
}

/** Percentage of a hive counter against its ceiling, clamped to 0–100. */
function hivePercent(value: number, ceiling: number): number {
  if (ceiling <= 0) return 0
  return Math.max(0, Math.min(100, Math.round((value / ceiling) * 100)))
}

type HiveRequestLimitsDraft = {
  readonly rpm: number
  readonly rph: number | null
  readonly rpd: number | null
}

function hiveLimitsDraftFromStored(limits: ProviderLimits): HiveRequestLimitsDraft {
  return {
    rpm: limits.rpm,
    // Values matching the conventional RPM-derived windows are treated as
    // automatic, including settings written by older versions of the UI.
    // The empty state (`rpm == 0`, zero windows) is automatic as well, so an
    // untouched editor keeps deriving both windows from a freshly typed RPM
    // instead of pinning them to an explicit zero.
    rph: limits.rph === limits.rpm * 60 ? null : limits.rph,
    rpd: limits.rpd === limits.rpm * 1440 ? null : limits.rpd
  }
}

function HiveRequestLimitsEditor({
  title,
  prefix,
  limits,
  quota,
  resetAtUtcMidnight,
  disabled,
  onLimitsChange,
  onQuotaChange,
  onResetChange
}: {
  readonly title: string
  readonly prefix: 'hive-primary' | 'hive-fallback'
  readonly limits: HiveRequestLimitsDraft
  readonly quota: string
  readonly resetAtUtcMidnight: boolean
  readonly disabled: boolean
  readonly onLimitsChange: (limits: HiveRequestLimitsDraft) => void
  readonly onQuotaChange: (quota: string) => void
  readonly onResetChange: (reset: boolean) => void
}): React.JSX.Element {
  return (
    <fieldset className="provider-form__tier" aria-label={translate(title)}>
      <legend>{translate(title)}</legend>
      <div className="provider-form__grid">
        {(['rpm', 'rph', 'rpd'] as const).map((key) => (
          <label key={key} htmlFor={`${prefix}-${key}`}>
            {key.toUpperCase()}
            <input
              id={`${prefix}-${key}`}
              aria-label={`${translate(title)} ${key.toUpperCase()}`}
              type="number"
              min={0}
              max={4_294_967_295}
              step={1}
              value={limits[key] ?? ''}
              disabled={disabled}
              placeholder={key === 'rpm' ? undefined : 'авто'}
              onChange={(event) => {
                const raw = event.target.value
                onLimitsChange({ ...limits, [key]: raw === '' ? (key === 'rpm' ? 0 : null) : Number(raw) })
              }}
            />
          </label>
        ))}
      </div>
      <label htmlFor={`${prefix}-daily-token-quota`}>
        {translate('Дневная квота токенов (0 — без лимита)')}
        <input
          id={`${prefix}-daily-token-quota`}
          type="number"
          min={0}
          max={1_000_000_000_000}
          step={1}
          value={quota}
          disabled={disabled}
          onChange={(event) => onQuotaChange(event.target.value)}
        />
      </label>
      <label>
        <input type="checkbox" checked={resetAtUtcMidnight} disabled={disabled || !quota || quota === '0'} onChange={(event) => onResetChange(event.target.checked)} />
        {translate('Сброс в полночь UTC')}
      </label>
    </fieldset>
  )
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
  readonly subtitle: string
  readonly icon: UiIconName
  readonly aliases: readonly string[]
}

const HIVE_DEPARTMENT_CARDS: readonly HiveDepartmentCard[] = [
  { key: 'analytics', label: 'Аналитика', subtitle: 'Анализ данных', icon: 'activity', aliases: ['analyst', 'analytics', 'analysis', 'review'] },
  { key: 'development', label: 'Разработка', subtitle: 'Разработка кода', icon: 'terminal', aliases: ['developer', 'engineer', 'coder', 'implementer', 'dev'] },
  { key: 'testing', label: 'Тестирование', subtitle: 'Проверка качества', icon: 'check-circle', aliases: ['qa', 'tester', 'test', 'verifier', 'validation'] },
  { key: 'planning', label: 'Планирование', subtitle: 'Планирование задач', icon: 'compass', aliases: ['planner', 'architect', 'plan'] },
  { key: 'research', label: 'Исследования', subtitle: 'Поиск решений', icon: 'target', aliases: ['researcher', 'research', 'scout', 'investigator'] },
  { key: 'integrations', label: 'Интеграции', subtitle: 'Инфраструктура и API', icon: 'link', aliases: ['integrator', 'integration', 'connector', 'adapter'] },
  { key: 'documentation', label: 'Документация', subtitle: 'База знаний и доки', icon: 'folder', aliases: ['documenter', 'documentation', 'docs', 'writer'] },
  { key: 'support', label: 'Поддержка', subtitle: 'Помощь и сервис', icon: 'message', aliases: ['support', 'assistant', 'responder', 'helper'] }
]

interface DepartmentView {
  readonly key: string
  readonly label: string
  readonly subtitle: string
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
      subtitle: card.subtitle,
      icon: card.icon,
      activeCount: bucket?.active ?? 0,
      maxCapacity: bucket?.max ?? 0
    }
  })
  for (const department of extra) {
    cards.push({
      key: `role:${department.role}`,
      label: department.role || 'unknown',
      subtitle: 'Агент улья',
      icon: 'box',
      activeCount: department.activeCount,
      maxCapacity: department.maxCapacity
    })
  }
  return cards
}

const HIVE_INTERVALS = [1000, 2000, 5000, 10000] as const
const HIVE_PREFERENCE_KEY = 'evohime.hive.dashboard.v1'
const QUEEN_TYPING_WATCHDOG_MS = 60_000

interface HivePreference {
  readonly intervalMs: number
}

/** Единственный владелец этого состояния — оболочка: это режим отображения. */
function loadHivePreference(): HivePreference {
  try {
    const stored: unknown = JSON.parse(window.localStorage.getItem(HIVE_PREFERENCE_KEY) ?? 'null')
    if (stored === null || typeof stored !== 'object' || Array.isArray(stored)) {
      return { intervalMs: 2000 }
    }
    const record = stored as Record<string, unknown>
    const interval = typeof record['intervalMs'] === 'number' && (HIVE_INTERVALS as readonly number[]).includes(record['intervalMs'])
      ? record['intervalMs']
      : 2000
    return { intervalMs: interval }
  } catch {
    return { intervalMs: 2000 }
  }
}

function saveHivePreference(preference: HivePreference): void {
  try {
    window.localStorage.setItem(HIVE_PREFERENCE_KEY, JSON.stringify(preference))
  } catch {
    // Storage may be unavailable; the in-memory mode stays active.
  }
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
  'runtime.schedule_completed': 'Autonomous Cycle: расписание выполнено',
  'runtime.schedule_failed': 'Autonomous Cycle: ошибка расписания',
  'runtime.schedule_dead_letter': 'Autonomous Cycle: задание в dead-letter'
}

function hiveLogLevel(eventType: string): HiveLogLevel {
  if (eventType === 'approval.required') return 'warning'
  if (/(failed|denied|dead_letter|error|cancelled|timed_out|stopped)/.test(eventType)) return 'warning'
  if (/(completed|succeeded|accepted|resolved)/.test(eventType)) return 'success'
  return 'info'
}

function coreEventKey(event: CoreEvent): string {
  return `${event.coreInstanceId ?? 'legacy'}:${event.sessionEpoch ?? 0}:${event.sequenceId}:${event.taskId}:${event.eventType}`
}

const MAX_HIVE_LOGS_CLIPBOARD_CHARS = 60_000
const MAX_HIVE_LOG_PAYLOAD_CHARS = 1000
const HIVE_LOGS_CLIPBOARD_LIMIT = 100

/** Служебные события: в компактном списке логов скрыты, чтобы не заглушать действия Евы. */
const HIVE_LOG_NOISE_EVENTS: ReadonlySet<string> = new Set([
  'task.checkpoint.saved',
  'routing.terminal',
  'tool.telemetry'
])

const TOOL_ACTION_LABELS: Record<string, string> = {
  filesystem_read: 'читает файл',
  eva_workspace_read_file: 'читает файл',
  filesystem_patch: 'правит файл',
  eva_workspace_write_file: 'пишет файл',
  eva_workspace_run_command: 'запускает команду',
  eva_workspace_git: 'работает с git',
  filesystem_list: 'смотрит содержимое папки',
  analysis_run: 'запускает анализ',
  goal_create: 'создаёт цель',
  goal_verify: 'проверяет цель',
  workflow_start: 'запускает workflow',
  workflow_status: 'проверяет workflow',
  workflow_cancel: 'отменяет workflow',
  workflow_catalog: 'смотрит каталог workflow'
}

/** Разбирает payload события; Core оборачивает тело в единственный ключ-вариант (`ToolStarted` и т. п.). */
function eventBody(event: CoreEvent): Record<string, unknown> {
  if (typeof event.payload !== 'string' || event.payload.trim().length === 0) return {}
  try {
    const parsed: unknown = JSON.parse(event.payload)
    if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return {}
    const root = parsed as Record<string, unknown>
    const keys = Object.keys(root)
    const only = keys.length === 1 ? root[keys[0] as string] : undefined
    return only !== null && typeof only === 'object' && !Array.isArray(only)
      ? (only as Record<string, unknown>)
      : root
  } catch {
    return {}
  }
}

function shortText(value: string, max: number): string {
  const single = value.replace(/\s+/g, ' ').trim()
  return single.length > max ? `${single.slice(0, max)}…` : single
}

/** «читает файл AGENTS.md» по событию `tool.started`. */
function describeToolStart(body: Record<string, unknown>): string {
  const tool = typeof body['tool_name'] === 'string' ? body['tool_name'] : 'инструмент'
  const action = TOOL_ACTION_LABELS[tool] ?? `вызывает ${tool}`
  let target = ''
  if (typeof body['arguments'] === 'string') {
    try {
      const args = JSON.parse(body['arguments']) as Record<string, unknown>
      const raw = args['path'] ?? args['command'] ?? args['file'] ?? args['query']
      if (typeof raw === 'string') target = shortText(raw, 80)
    } catch {
      target = ''
    }
  }
  return target ? `${action}: ${target}` : action
}

/** Человекочитаемая строка лога; незнакомое событие остаётся подписью по типу. */
function describeHiveEvent(event: CoreEvent): string {
  const body = eventBody(event)
  if (event.eventType === 'task.failed' && !body['error_code'] && !body['error']) {
    return `Задача остановлена с ошибкой · ${event.taskId}`
  }
  if (event.eventType === 'task.completed' && !body['final_message'] && !body['summary']) {
    return `Задача завершена · ${event.taskId}`
  }
  switch (event.eventType) {
    case 'tool.started':
      return `Ева ${describeToolStart(body)}`
    case 'tool.output': {
      const output = typeof body['output'] === 'string' ? body['output'] : ''
      const failed = /^(resource not found|tool execution failed|permission denied)/i.test(output)
      const tool = typeof body['tool_name'] === 'string' ? body['tool_name'] : 'инструмент'
      return failed
        ? `Ошибка инструмента ${tool}: ${shortText(output, 90)}`
        : `Результат ${tool} получен${output ? ` (${output.length} симв.)` : ''}`
    }
    case 'model.context': {
      const model = typeof body['model'] === 'string' ? body['model'] : ''
      const tokens = typeof body['estimated_tokens'] === 'number' ? ` · ~${body['estimated_tokens']} токенов` : ''
      return `Ева думает${model ? ` (${model})` : ''}${tokens}`
    }
    default:
      return `${HIVE_LOG_LABELS[event.eventType] ?? event.eventType}${event.taskId ? ` · ${event.taskId}` : ''}`
  }
}

/** Что Королева делает прямо сейчас: по последнему событию её незавершённой задачи. */
function queenCurrentActivity(events: readonly CoreEvent[]): string {
  let latestEvent: CoreEvent | undefined
  for (const event of events) {
    if (!event.taskId || event.eventType === 'hive_state.snapshot') continue
    if (event.eventType !== 'tool.started' && event.eventType !== 'tool.output' && event.eventType !== 'model.context') continue
    if (!latestEvent || event.sequenceId > latestEvent.sequenceId) latestEvent = event
  }
  if (!latestEvent) return 'Обрабатывает запрос'
  const body = eventBody(latestEvent)
  if (latestEvent.eventType === 'tool.started') return `Ева ${describeToolStart(body)}`
  if (latestEvent.eventType === 'tool.output') return 'Анализирует результат инструмента'
  return 'Ева думает над следующим шагом'
}

function hiveLogEntries(
  events: readonly CoreEvent[],
  observedAt: ReadonlyMap<string, string>,
  limit: number,
  includeNoise = true,
  chatTaskIds: ReadonlySet<string> = new Set(),
  observedAtFor: (event: CoreEvent) => string = (event) => observedAt.get(coreEventKey(event)) ?? '—'
): readonly HiveLogEntry[] {
  return events
    .filter((event) => event.eventType !== 'hive_state.snapshot' && event.eventType !== 'hive.observation_status')
    .filter((event) => !event.taskId || chatTaskIds.has(event.taskId) || event.taskId.startsWith('eva-pulse-'))
    .filter((event) => includeNoise || !HIVE_LOG_NOISE_EVENTS.has(event.eventType))
    .sort((left, right) => right.sequenceId - left.sequenceId)
    .slice(0, limit)
    .map((event) => {
      const key = coreEventKey(event)
      return {
        id: key,
        at: observedAtFor(event),
        level: hiveLogLevel(event.eventType),
        text: describeHiveEvent(event)
      }
    })
}

function formatHiveLogsForClipboard(
  events: readonly CoreEvent[],
  observedAt: ReadonlyMap<string, string>,
  budget: HiveStateView['budget'],
  chatTaskIds: ReadonlySet<string> = new Set(),
  observedAtFor: (event: CoreEvent) => string = (event) => observedAt.get(coreEventKey(event)) ?? '—'
): string {
  const headerParts: string[] = [
    `Провайдер: ${budget.provider || 'не выбран'}`,
    `Модель: ${budget.model || 'не известна'}`
  ]
  if (budget.fallbackProvider) {
    headerParts.push(
      `Резерв: ${budget.fallbackProvider}${budget.fallbackTokensRemaining !== null ? ` (${budget.fallbackTokensRemaining} токенов)` : ''}${budget.fallbackModel ? ` [${budget.fallbackModel}]` : ''}`
    )
  }
  const lines: string[] = [
    '=== Логи Улья (EvoHime) ===',
    headerParts.join(' | '),
    ''
  ]
  const relevant = events
    .filter((event) => event.eventType !== 'hive_state.snapshot' && event.eventType !== 'hive.observation_status')
    .filter((event) => !event.taskId || chatTaskIds.has(event.taskId) || event.taskId.startsWith('eva-pulse-'))
    .sort((left, right) => right.sequenceId - left.sequenceId)
    .slice(0, HIVE_LOGS_CLIPBOARD_LIMIT)
  for (const event of relevant) {
    const at = observedAtFor(event)
    const level = hiveLogLevel(event.eventType).toUpperCase()
    const label = HIVE_LOG_LABELS[event.eventType]
    const meta = [
      `seq=${event.sequenceId}`,
      event.taskId ? `task=${event.taskId}` : ''
    ].filter(Boolean).join(', ')
    lines.push(`[${at}] [${level}] ${event.eventType} (${meta})${label ? ` — ${label}` : ''}`)
    const rawPayload = typeof event.payload === 'string' ? event.payload.trim() : ''
    if (rawPayload.length > 0 && rawPayload !== '{}') {
      const singleLine = rawPayload.replace(/\s+/g, ' ')
      const bounded = singleLine.length > MAX_HIVE_LOG_PAYLOAD_CHARS
        ? `${singleLine.slice(0, MAX_HIVE_LOG_PAYLOAD_CHARS)}…`
        : singleLine
      lines.push(`  payload: ${bounded}`)
    }
  }
  const joined = lines.join('\n')
  if (joined.length <= MAX_HIVE_LOGS_CLIPBOARD_CHARS) {
    return joined
  }
  const suffix = '\n… (обрезано по лимиту буфера обмена)'
  return `${joined.slice(0, MAX_HIVE_LOGS_CLIPBOARD_CHARS - suffix.length)}${suffix}`
}

/** Круговой индикатор эффективности: только SVG, без сторонних библиотек. */
function EfficiencyRing({ percent }: { readonly percent: number }): React.JSX.Element {
  const bounded = Math.max(0, Math.min(100, Math.round(percent)))
  const radius = 34
  const circumference = 2 * Math.PI * radius
  return (
    <svg className="hive-ring" viewBox="0 0 88 88" role="img" aria-label={`${translate('Эффективность')} ${bounded}%`}>
      <defs>
        <linearGradient id="hive-ring-grad" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%" stopColor="var(--hive-blue)" />
          <stop offset="55%" stopColor="var(--hive-violet)" />
          <stop offset="100%" stopColor="var(--hive-green)" />
        </linearGradient>
      </defs>
      <circle className="hive-ring__track" cx="44" cy="44" r={radius} />
      <circle
        className="hive-ring__value"
        cx="44"
        cy="44"
        r={radius}
        stroke="url(#hive-ring-grad)"
        strokeDasharray={circumference}
        strokeDashoffset={circumference * (1 - bounded / 100)}
        transform="rotate(-90 44 44)"
      />
      <text className="hive-ring__label" x="44" y="45" textAnchor="middle">{bounded}%</text>
      <text className="hive-ring__sublabel" x="44" y="58" textAnchor="middle">{translate('Эффективность')}</text>
    </svg>
  )
}

function HiveSkeleton({ className = '' }: { readonly className?: string }): React.JSX.Element {
  return <span className={`hive-skeleton ${className}`.trim()} aria-hidden="true" />
}

const HIVE_CONNECTION_LABELS: Record<ConnectionState, string> = {
  starting: 'Запуск',
  connecting: 'Подключение к Core',
  connected: 'Подключено',
  reconnecting: 'Переподключение',
  replaying: 'Восстановление событий',
  resyncing: 'Синхронизация состояния',
  'state-gap': 'Пробел в состоянии — нужна пересинхронизация',
  'version-mismatch': 'Несовместимая версия IPC',
  degraded: 'Ограниченный режим',
  fatal: 'Критическая ошибка'
}

/** Векторная эмблема цветка лотоса Улья для карточки Королевы и лаунчера диалога. */
function HiveLotusMark({ size = 24 }: { readonly size?: number }): React.JSX.Element {
  const gradientId = useId()
  const centerId = `${gradientId}-center`
  const wingId = `${gradientId}-wing`

  return (
    <svg className="hive-lotus-mark" viewBox="0 0 48 48" width={size} height={size} fill="none" aria-hidden="true">
      <defs>
        <linearGradient id={centerId} x1="24" y1="8" x2="24" y2="40" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#e9d5ff" />
          <stop offset="55%" stopColor="#8d82e8" />
          <stop offset="100%" stopColor="#5ea8ff" />
        </linearGradient>
        <linearGradient id={wingId} x1="8" y1="16" x2="40" y2="38" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#c084fc" stopOpacity="0.92" />
          <stop offset="100%" stopColor="#4ec9a4" stopOpacity="0.72" />
        </linearGradient>
      </defs>
      <path d="M24 8C28.5 14.5 30 22 24 34C18 22 19.5 14.5 24 8Z" fill={`url(#${centerId})`} />
      <path d="M12 16C18.5 18 22 24.5 23.5 34.5C15.5 31.5 11.5 24.5 12 16Z" fill={`url(#${wingId})`} />
      <path d="M36 16C29.5 18 26 24.5 24.5 34.5C32.5 31.5 36.5 24.5 36 16Z" fill={`url(#${wingId})`} />
      <path d="M7 25.5C13.5 25.5 18.5 29.5 23 35.5C15 36.5 9.5 32.5 7 25.5Z" fill="#8d82e8" fillOpacity="0.65" />
      <path d="M41 25.5C34.5 25.5 29.5 29.5 25 35.5C33 36.5 38.5 32.5 41 25.5Z" fill="#5ea8ff" fillOpacity="0.65" />
      <ellipse cx="24" cy="37.5" rx="11" ry="2.2" fill="#8d82e8" fillOpacity="0.35" />
    </svg>
  )
}

function parseApprovalRequest(event: CoreEvent): ApprovalRequestView | null {
  try {
    const parsed = eventBody(event)
    // Core serializes enum variants as externally tagged objects, e.g.
    // { "ApprovalRequired": { ... } }. Keep accepting the flat legacy shape.
    const tagged = parsed['ApprovalRequired']
    const root = tagged !== null && typeof tagged === 'object' && !Array.isArray(tagged)
      ? tagged as Record<string, unknown>
      : parsed
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
  limits: { rpm: 0, rph: 0, rpd: 0 },
  minuteExhausted: false,
  exhausted: false,
  dailyCostAvailable: false,
  dailyCostNanoUsd: null,
  dailyTokenQuota: null,
  dailyTokensRemaining: null,
  quotaResetUtcMidnight: false,
  fallbackProvider: null,
  fallbackModel: null,
  fallbackTokensRemaining: null,
  fallbackDailyTokenQuota: null,
  fallbackQuotaResetUtcMidnight: false
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

function transcriptEventPayload(event: ConversationEventProjection): string {
  const body = typeof event.payload === 'object' && event.payload !== null
    ? event.payload as Record<string, unknown>
    : {}
  if (event.kind === 'assistant_message_started') {
    return JSON.stringify({ prompt: typeof body['prompt'] === 'string' ? body['prompt'] : '' })
  }
  if (event.kind === 'assistant_message_delta') {
    return JSON.stringify({ content: typeof body['content'] === 'string' ? body['content'] : '' })
  }
  if (event.kind === 'assistant_message_finalized') {
    return JSON.stringify({ final_message: typeof body['content'] === 'string' ? body['content'] : '' })
  }
  return JSON.stringify(event.payload)
}

function projectHiveChatEvents(events: readonly ConversationEventProjection[]): readonly CoreEvent[] {
  return events.map((event) => ({
    sequenceId: event.sequence,
    taskId: event.taskId,
    eventType: event.kind === 'assistant_message_started' ? 'task.started'
      : event.kind === 'task_started' ? 'task.started'
      : event.kind === 'assistant_message_delta' ? 'agent.message.delta'
        : event.kind === 'assistant_message_finalized' ? 'agent.message.finalized'
          : event.kind === 'assistant_message_failed' ? 'agent.message.failed'
            : event.kind === 'task_completed' ? 'task.completed'
              : event.kind === 'task_failed' ? 'task.failed'
                : event.kind === 'task_stopped' ? 'task.stopped'
                  : event.kind.replaceAll('_', '.'),
    payload: transcriptEventPayload(event),
    executionEvent: null,
    conversationEventLog: null
  }))
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

    if (event.eventType === 'task.completed' || event.eventType === 'agent.message.finalized') {
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
      const message = readEventString(payload, 'final_message') || readEventString(payload, 'message')
      const errorCode = readEventString(payload, 'error_code') || readEventString(payload, 'error')
      const existing = queenByTask.get(taskKey)
      const fallbackText = formatQueenFailureText(errorCode)
      const text = message.length > 0
        ? message
        : existing && existing.text.trim().length > 0 && existing.text.includes('Королева Ева:')
          ? existing.text
          : fallbackText
      queenByTask.set(taskKey, {
        text,
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

function formatQueenFailureText(errorCodeOrMessage: string): string {
  const normalized = errorCodeOrMessage.trim()
  const lower = normalized.toLowerCase()
  if (lower === 'permission_denied' || lower.includes('permission denied') || lower.includes('access denied')) {
    return 'Королева Ева: У меня нет прав на выполнение этого действия. Проверьте, что указан правильный путь к песочнице и у меня есть доступ к файлам.'
  }
  const generic = normalized.length === 0 || normalized === 'task_failed'
  if (generic) {
    return 'Королева Ева: Я не смогла выполнить задачу, Core не сообщил точную причину. Последние действия и ошибки инструментов смотри в «Логи улья» (кнопка «Все логи»).'
  }
  return `Королева Ева: Я не смогла выполнить задачу. Причина: ${normalized}. Подробности — в «Логи улья».`
}

/** Поясняет отказ Core при приёме сообщения в чат Улья. */
function hiveAcceptErrorMessage(errorCode: string): string {
  switch (errorCode) {
    case 'forbidden':
      return 'Core отклонил сообщение: чат Улья доступен только оболочке EvoHime.'
    case 'idempotency_conflict':
      return 'Это сообщение уже отправлено Королеве Еве.'
    case 'dispatch_unknown':
      return 'Не удалось подтвердить запуск задачи. Проверьте состояние Улья и повторите отправку.'
    case 'invalid_argument':
      return 'Сообщение для Королевы Евы отклонено как некорректное.'
    default:
      return 'Core отклонил сообщение для Королевы Евы. Подробности — в логах Core.'
  }
}

function isQueenThinking(
  events: readonly CoreEvent[],
  pendingTaskIds: ReadonlySet<string>,
  sendingMessage: boolean,
  activeApprovalTaskIds: ReadonlySet<string>,
  watchdogExpiredTaskIds: ReadonlySet<string>
): boolean {
  if (sendingMessage) return true
  const terminalTypes = new Set(['task.completed', 'task.failed', 'task.stopped'])
  const failedOrStoppedTaskIds = new Set<string>()
  const activeTaskSeq = new Map<string, number>()
  const terminalTaskSeq = new Map<string, number>()
  const approvalTaskSeq = new Map<string, number>()

  for (const event of events) {
    if (!event.taskId) continue
    if (terminalTypes.has(event.eventType)) {
      if (event.eventType === 'task.failed' || event.eventType === 'task.stopped') {
        failedOrStoppedTaskIds.add(event.taskId)
      }
      const current = terminalTaskSeq.get(event.taskId) ?? -1
      if (event.sequenceId > current) terminalTaskSeq.set(event.taskId, event.sequenceId)
    } else if (event.eventType === 'approval.required') {
      const current = approvalTaskSeq.get(event.taskId) ?? -1
      if (event.sequenceId > current) approvalTaskSeq.set(event.taskId, event.sequenceId)
    } else if (event.eventType === 'task.started' || event.eventType === 'agent.message.delta') {
      const current = activeTaskSeq.get(event.taskId) ?? -1
      if (event.sequenceId > current) activeTaskSeq.set(event.taskId, event.sequenceId)
    }
  }

  for (const taskId of pendingTaskIds) {
    if (activeApprovalTaskIds.has(taskId) || failedOrStoppedTaskIds.has(taskId) || watchdogExpiredTaskIds.has(taskId)) continue
    const approvalSeq = approvalTaskSeq.get(taskId) ?? -1
    const activeSeq = activeTaskSeq.get(taskId) ?? -1
    if (approvalSeq >= 0 && approvalSeq >= activeSeq) continue
    const finished = events.some((event) => event.taskId === taskId && terminalTypes.has(event.eventType))
    if (!finished) return true
  }

  for (const [taskId, startedSeq] of activeTaskSeq.entries()) {
    if (activeApprovalTaskIds.has(taskId) || failedOrStoppedTaskIds.has(taskId) || watchdogExpiredTaskIds.has(taskId)) continue
    const approvalSeq = approvalTaskSeq.get(taskId) ?? -1
    if (approvalSeq >= startedSeq) continue
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
  onChatOpened,
  onChatTouched,
  resolvedApprovalIds = [],
  onApprovalResolved
}: HiveDashboardProps): React.JSX.Element {
  const api = useShellApi()
  const {
    summary: providerSummary,
    loading: providerLoading,
    error: providerError,
    refresh: refreshProviderSummary,
    apply: applyProviderSummary
  } = useProviderState()
  const hiveApiKeyConfigured = providerSummary?.hiveApiKeyConfigured === true
  const [resolvingApprovalIds, setResolvingApprovalIds] = useState<ReadonlySet<string>>(() => new Set())
  const [resolvedLocally, setResolvedLocally] = useState<readonly string[]>([])
  const [commandError, setCommandError] = useState<string | null>(null)
  const [preference, setPreference] = useState<HivePreference>(() => loadHivePreference())
  const [allLogs, setAllLogs] = useState(false)
  const [evaWorkspacePath, setEvaWorkspacePath] = useState('')
  const [evaWorkspaceDraft, setEvaWorkspaceDraft] = useState('')
  const [evaWorkspaceError, setEvaWorkspaceError] = useState<string | null>(null)
  const [evaWorkspaceBusy, setEvaWorkspaceBusy] = useState(false)
  const [watchedRepositories, setWatchedRepositories] = useState<readonly string[]>([])
  const [newRepoDraft, setNewRepoDraft] = useState('')
  const [watchedReposBusy, setWatchedReposBusy] = useState(false)
  const [watchedReposError, setWatchedReposError] = useState<string | null>(null)
  const [hiveProviderProfile, setHiveProviderProfile] = useState('')
  const [hiveTier, setHiveTier] = useState<ModelTier>('free')
  const [hiveModelDraft, setHiveModelDraft] = useState('')
  const [hiveCatalogModels, setHiveCatalogModels] = useState<readonly string[]>([])
  const [hiveCatalogError, setHiveCatalogError] = useState<string | null>(null)
  const [hiveCatalogLoading, setHiveCatalogLoading] = useState(false)
  const [hiveProviderBusy, setHiveProviderBusy] = useState(false)
  const [hiveProviderError, setHiveProviderError] = useState<string | null>(null)
  const [hiveApiKeyDraft, setHiveApiKeyDraft] = useState('')
  const [isEditingMainKey, setIsEditingMainKey] = useState(false)
  const [hiveApiKeyBusy, setHiveApiKeyBusy] = useState(false)
  const [hiveApiKeyError, setHiveApiKeyError] = useState<string | null>(null)
  const [hiveFallbackProviderProfile, setHiveFallbackProviderProfile] = useState('')
  const [hiveFallbackTier, setHiveFallbackTier] = useState<ModelTier>('free')
  const [hiveFallbackModelDraft, setHiveFallbackModelDraft] = useState('')
  const [hiveFallbackCatalogModels, setHiveFallbackCatalogModels] = useState<readonly string[]>([])
  const [hiveFallbackCatalogError, setHiveFallbackCatalogError] = useState<string | null>(null)
  const [hiveFallbackCatalogLoading, setHiveFallbackCatalogLoading] = useState(false)
  const [hiveFallbackProviderBusy, setHiveFallbackProviderBusy] = useState(false)
  const [hiveFallbackProviderError, setHiveFallbackProviderError] = useState<string | null>(null)
  const [hiveFallbackProviderSaved, setHiveFallbackProviderSaved] = useState(false)
  const [hiveFallbackApiKeyDraft, setHiveFallbackApiKeyDraft] = useState('')
  const [isEditingFallbackKey, setIsEditingFallbackKey] = useState(false)
  const [hiveFallbackApiKeyBusy, setHiveFallbackApiKeyBusy] = useState(false)
  const [hiveFallbackApiKeyError, setHiveFallbackApiKeyError] = useState<string | null>(null)
  const [hiveFallbackApiKeySaved, setHiveFallbackApiKeySaved] = useState(false)
  const [dailyBudgetDraft, setDailyBudgetDraft] = useState('')
  const [dailyBudgetBusy, setDailyBudgetBusy] = useState(false)
  const [dailyBudgetError, setDailyBudgetError] = useState<string | null>(null)
  const [hiveProviderSaved, setHiveProviderSaved] = useState(false)
  const [hiveApiKeySaved, setHiveApiKeySaved] = useState(false)
  const [dailyBudgetSaved, setDailyBudgetSaved] = useState(false)
  const [hivePrimaryLimitsDraft, setHivePrimaryLimitsDraft] = useState<HiveRequestLimitsDraft>({ rpm: 0, rph: null, rpd: null })
  const [hiveFallbackLimitsDraft, setHiveFallbackLimitsDraft] = useState<HiveRequestLimitsDraft>({ rpm: 0, rph: null, rpd: null })
  const [hivePrimaryQuotaDraft, setHivePrimaryQuotaDraft] = useState('0')
  const [hiveFallbackQuotaDraft, setHiveFallbackQuotaDraft] = useState('0')
  const [hivePrimaryQuotaResetDraft, setHivePrimaryQuotaResetDraft] = useState(false)
  const [hiveFallbackQuotaResetDraft, setHiveFallbackQuotaResetDraft] = useState(false)
  const [hiveLimitsBusy, setHiveLimitsBusy] = useState(false)
  const [hiveLimitsError, setHiveLimitsError] = useState<string | null>(null)
  const [hiveLimitsSaved, setHiveLimitsSaved] = useState(false)
  const [proactiveEvolutionEnabled, setProactiveEvolutionEnabled] = useState(false)
  const [proactiveEvolutionBusy, setProactiveEvolutionBusy] = useState(false)
  const [proactiveEvolutionError, setProactiveEvolutionError] = useState<string | null>(null)
  const [hiveSettingsOpen, setHiveSettingsOpen] = useState(false)
  const [hiveChatOpen, setHiveChatOpen] = useState(false)
  const [selectedDepartment, setSelectedDepartment] = useState<string | null>(null)
  const [evaWorkspaceValidation, setEvaWorkspaceValidation] = useState<'idle' | 'checking' | 'valid' | 'invalid'>('idle')
  const [evaWorkspaceValidationMessage, setEvaWorkspaceValidationMessage] = useState<string | null>(null)
  const [observedAt, setObservedAt] = useState<ReadonlyMap<string, string>>(() => new Map())
  const logListRef = useRef<HTMLOListElement | null>(null)
  const [logsCopyState, setLogsCopyState] = useState<'idle' | 'copied' | 'failed'>('idle')
  const logsCopyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const [chatDraft, setChatDraft] = useState('')
  const [sendingMessage, setSendingMessage] = useState(false)
  const [pendingTaskIds, setPendingTaskIds] = useState<ReadonlySet<string>>(() => new Set())
  const [typingWatchdogExpiredTaskIds, setTypingWatchdogExpiredTaskIds] = useState<ReadonlySet<string>>(() => new Set())
  const [localPrompts, setLocalPrompts] = useState<readonly HiveUserPrompt[]>([])
  const [historyPrompts, setHistoryPrompts] = useState<readonly HiveUserPrompt[]>([])
  const [hiveConversationLogEvents, setHiveConversationLogEvents] = useState<readonly ConversationEventProjection[]>([])
  const hiveConversationRef = useRef(createConversationProjection(HIVE_SYSTEM_CHAT_ID))
  const hiveSubscriptionRef = useRef<{ key: string; requestId: string } | null>(null)
  const hiveSeenPagesRef = useRef(new Set<string>())
  const connected = CONNECTED_STATES.includes(connection)
  const hive = useMemo(() => parseHiveState(latest(events, 'hive_state.snapshot')), [events])
  const hasHiveSnapshot = latest(events, 'hive_state.snapshot') !== undefined
  const observationState = useMemo(() => parseHiveObservationState(latest(events, 'hive.observation_status')), [events])
  const hiveChatTaskIds = useMemo(
    () => new Set([...hiveConversationLogEvents, ...localPrompts.map((prompt) => ({ taskId: prompt.taskId }))]
      .map((item) => item.taskId)
      .filter(Boolean)),
    [hiveConversationLogEvents, localPrompts]
  )
  const hiveConversationObservedAt = useMemo(() => {
    const received = new Map<string, string>()
    for (const event of events) {
      const page = event.conversationEventLog
      if (page?.conversationId !== HIVE_SYSTEM_CHAT_ID) continue
      const at = observedAt.get(coreEventKey(event))
      if (!at) continue
      for (const item of page.events) received.set(item.eventId, at)
    }
    return received
  }, [events, observedAt])
  const observedAtForHiveEvent = useCallback((event: CoreEvent): string => {
    const direct = observedAt.get(coreEventKey(event))
    if (direct) return direct
    const kind = event.eventType === 'agent.message.finalized' ? 'assistant_message_finalized'
      : event.eventType === 'agent.message.failed' ? 'assistant_message_failed'
        : event.eventType === 'task.started' ? 'task_started'
          : event.eventType === 'agent.message.delta' ? 'assistant_message_delta'
            : event.eventType === 'task.completed' ? 'task_completed'
              : event.eventType === 'task.failed' ? 'task_failed'
                : event.eventType === 'task.stopped' ? 'task_stopped'
                  : event.eventType === 'approval.required' ? 'approval_required'
                    : event.eventType === 'tool.started' ? 'tool_started'
                      : event.eventType === 'tool.output' ? 'tool_output'
                        : event.eventType.replaceAll('.', '_')
    const conversationEventId = hiveConversationLogEvents.find((item) => item.sequence === event.sequenceId
      && item.taskId === event.taskId && item.kind === kind)?.eventId
    return conversationEventId ? hiveConversationObservedAt.get(conversationEventId) ?? '—' : '—'
  }, [hiveConversationLogEvents, hiveConversationObservedAt, observedAt])
  const watchedReposKey = hive ? hive.watchedRepositories.map((repository) => repository.path).join('\n') : ''
  useEffect(() => {
    if (!hive) return
    const configured = hive.evaWorkspacePath ?? ''
    setEvaWorkspacePath(configured)
    setEvaWorkspaceDraft(configured)
  }, [hive?.evaWorkspacePath])
  useEffect(() => {
    if (!hive) return
    setWatchedRepositories(hive.watchedRepositories.map((repository) => repository.path))
  }, [watchedReposKey])
  useEffect(() => {
    if (!hive) return
    setHiveProviderProfile(hive.hiveProviderProfile ?? '')
  }, [hive?.hiveProviderProfile])
  useEffect(() => {
    if (!hive) return
    const configuredModel = (hive.hiveModel ?? '').trim()
    setHiveModelDraft(configuredModel)
    if (configuredModel.length > 0) {
      setHiveTier(configuredModel.endsWith(':free') ? 'free' : 'paid')
    }
  }, [hive?.hiveModel])
  useEffect(() => {
    if (!hive) return
    setHiveFallbackProviderProfile(hive.hiveFallbackProviderProfile ?? '')
  }, [hive?.hiveFallbackProviderProfile])
  useEffect(() => {
    if (!hive) return
    const configuredModel = (hive.hiveFallbackModel ?? '').trim()
    setHiveFallbackModelDraft(configuredModel)
    if (configuredModel.length > 0) {
      setHiveFallbackTier(configuredModel.endsWith(':free') ? 'free' : 'paid')
    }
  }, [hive?.hiveFallbackModel])
  useEffect(() => {
    if (!api || !connected || !hiveSettingsOpen) return
    setHiveCatalogModels([])
    setHiveCatalogError(null)
    setHiveCatalogLoading(true)
    const trimmedProfile = hiveProviderProfile.trim()
    const profileArg = trimmedProfile === 'literouter'
      ? `literouter:${hiveTier}`
      : trimmedProfile
    void api.invoke('core.listModelCatalog', {
      mode: 'hive_catalog',
      ...(profileArg ? { profile: profileArg } : {})
    }).then((outcome) => {
      if (!outcome.ok) setHiveCatalogError('Не удалось получить каталог моделей.')
    }).catch(() => {
      setHiveCatalogError('Не удалось получить каталог моделей.')
    }).finally(() => setHiveCatalogLoading(false))
  }, [api, connected, hiveProviderProfile, hiveTier, hiveSettingsOpen])
  const loadHiveFallbackCatalog = useCallback(async (): Promise<void> => {
    if (!api || !connected || !hiveSettingsOpen) return
    const trimmedProfile = hiveFallbackProviderProfile.trim()
    if (!trimmedProfile) {
      setHiveFallbackCatalogModels([])
      setHiveFallbackCatalogError(null)
      setHiveFallbackCatalogLoading(false)
      return
    }
    setHiveFallbackCatalogModels([])
    setHiveFallbackCatalogError(null)
    setHiveFallbackCatalogLoading(true)
    const profileArg = trimmedProfile === 'literouter'
      ? `literouter:${hiveFallbackTier}`
      : trimmedProfile
    try {
      const outcome = await api.invoke('core.listModelCatalog', {
        mode: 'hive_fallback_catalog',
        profile: profileArg
      })
      if (!outcome.ok) setHiveFallbackCatalogError('Не удалось получить каталог моделей.')
    } catch {
      setHiveFallbackCatalogError('Не удалось получить каталог моделей.')
    } finally {
      setHiveFallbackCatalogLoading(false)
    }
  }, [api, connected, hiveFallbackProviderProfile, hiveFallbackTier, hiveSettingsOpen])
  useEffect(() => {
    void loadHiveFallbackCatalog()
  }, [loadHiveFallbackCatalog])
  const hiveModelCatalogEvent = latest(events, 'hive.model.catalog')
  useEffect(() => {
    if (!hiveModelCatalogEvent) return
    try {
      const parsed = JSON.parse(hiveModelCatalogEvent.payload) as { profile?: unknown; tier?: unknown; models?: unknown; error?: unknown }
      const trimmedProfile = hiveProviderProfile.trim()
      if (trimmedProfile.length > 0 && typeof parsed.profile === 'string' && parsed.profile.trim().length > 0 && parsed.profile.trim() !== trimmedProfile) {
        return
      }
      if (trimmedProfile === 'literouter' && typeof parsed.tier === 'string' && parsed.tier !== hiveTier) {
        return
      }
      setHiveCatalogLoading(false)
      if (Array.isArray(parsed.models)) {
        let models = parsed.models.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
        if (trimmedProfile === 'literouter') {
          models = models.filter((modelId) => (hiveTier === 'free' ? modelId.endsWith(':free') : !modelId.endsWith(':free')))
        }
        setHiveCatalogModels([...new Set(models)])
      }
      const errorCode = safeCatalogErrorCode(parsed.error)
      setHiveCatalogError(errorCode ? catalogErrorMessage(errorCode) : null)
    } catch {
      // Ignore malformed catalog payload
    }
  }, [hiveModelCatalogEvent, hiveProviderProfile, hiveTier])
  const hiveFallbackModelCatalogEvent = latest(events, 'hive.fallback_model.catalog')
  useEffect(() => {
    const trimmedProfile = hiveFallbackProviderProfile.trim()
    if (!trimmedProfile || !hiveFallbackModelCatalogEvent) return
    try {
      const parsed = JSON.parse(hiveFallbackModelCatalogEvent.payload) as { profile?: unknown; tier?: unknown; models?: unknown; error?: unknown }
      if (typeof parsed.profile === 'string' && parsed.profile.trim().length > 0 && parsed.profile.trim() !== trimmedProfile) {
        return
      }
      if (trimmedProfile === 'literouter' && typeof parsed.tier === 'string' && parsed.tier !== hiveFallbackTier) {
        return
      }
      setHiveFallbackCatalogLoading(false)
      if (Array.isArray(parsed.models)) {
        let models = parsed.models.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
        if (trimmedProfile === 'literouter') {
          models = models.filter((modelId) => (hiveFallbackTier === 'free' ? modelId.endsWith(':free') : !modelId.endsWith(':free')))
        }
        setHiveFallbackCatalogModels([...new Set(models)])
      }
      const errorCode = safeCatalogErrorCode(parsed.error)
      setHiveFallbackCatalogError(errorCode ? catalogErrorMessage(errorCode) : null)
    } catch {
      // Ignore malformed catalog payload
    }
  }, [hiveFallbackModelCatalogEvent, hiveFallbackProviderProfile, hiveFallbackTier])
  useEffect(() => {
    if (!hive) return
    setDailyBudgetDraft(String(hive.dailyBudgetUsd))
  }, [hive?.dailyBudgetUsd])
  useEffect(() => {
    if (!hive) return
    setHivePrimaryLimitsDraft(hiveLimitsDraftFromStored(hive.hiveProviderLimits))
    setHiveFallbackLimitsDraft(hiveLimitsDraftFromStored(hive.hiveFallbackProviderLimits))
    setHivePrimaryQuotaDraft(String(hive.hiveTokenQuotaConfigured ? hive.hiveDailyTokenQuota : 0))
    setHiveFallbackQuotaDraft(String(hive.hiveFallbackTokenQuotaConfigured ? hive.hiveFallbackDailyTokenQuota : 0))
    setHivePrimaryQuotaResetDraft(hive.hiveQuotaResetUtcMidnight)
    setHiveFallbackQuotaResetDraft(hive.hiveFallbackQuotaResetUtcMidnight)
  }, [hive?.hiveProviderLimits.rpm, hive?.hiveProviderLimits.rph, hive?.hiveProviderLimits.rpd, hive?.hiveFallbackProviderLimits.rpm, hive?.hiveFallbackProviderLimits.rph, hive?.hiveFallbackProviderLimits.rpd, hive?.hiveDailyTokenQuota, hive?.hiveFallbackDailyTokenQuota, hive?.hiveTokenQuotaConfigured, hive?.hiveFallbackTokenQuotaConfigured, hive?.hiveQuotaResetUtcMidnight, hive?.hiveFallbackQuotaResetUtcMidnight])
  useEffect(() => {
    if (!hive) return
    setProactiveEvolutionEnabled(hive.proactiveEvolutionEnabled)
  }, [hive?.proactiveEvolutionEnabled])
  const hiveSettingsResult = latest(events, 'hive_settings.saved')
  useEffect(() => {
    if (!hiveSettingsResult) return
    try {
      const result = JSON.parse(hiveSettingsResult.payload) as {
        accepted?: boolean
        setting_key?: string
        eva_workspace_path?: string | null
        watched_repositories?: readonly string[]
        hive_provider_profile?: string | null
        hive_model?: string | null
        hive_fallback_provider_profile?: string | null
        hive_fallback_model?: string | null
        daily_budget_usd?: number | null
        proactive_evolution_enabled?: boolean
        error_code?: string
      }
      if (result.setting_key === 'hive_request_limits') {
        setHiveLimitsBusy(false)
        setHiveLimitsSaved(result.accepted === true)
        setHiveLimitsError(result.accepted ? null : (result.error_code || 'Core отклонил лимиты Улья.'))
        return
      }
      if (result.setting_key === 'watched_repositories') {
        if (Array.isArray(result.watched_repositories)) {
          setWatchedRepositories(result.watched_repositories.filter((item) => !sameRepositoryPath(item, evaWorkspacePath)))
        }
        setWatchedReposError(result.accepted ? null : (result.error_code || 'Не удалось обновить список проектов.'))
        return
      }
      if (result.setting_key === 'hive_provider_profile') {
        if (result.accepted) {
          setHiveProviderProfile(result.hive_provider_profile ?? '')
          setHiveProviderError(null)
        } else {
          setHiveProviderError(result.error_code || 'Не удалось сохранить провайдера Улья.')
        }
        return
      }
      if (result.setting_key === 'hive_model') {
        if (result.accepted) {
          setHiveModelDraft(result.hive_model ?? '')
          setHiveProviderError(null)
        } else {
          setHiveProviderError(result.error_code || 'Не удалось сохранить модель Улья.')
        }
        return
      }
      if (result.setting_key === 'hive_fallback_provider_profile') {
        if (result.accepted) {
          setHiveFallbackProviderProfile(result.hive_fallback_provider_profile ?? '')
          setHiveFallbackProviderError(null)
        } else {
          setHiveFallbackProviderError(result.error_code || 'Не удалось сохранить резервного провайдера Улья.')
        }
        return
      }
      if (result.setting_key === 'hive_fallback_model') {
        if (result.accepted) {
          setHiveFallbackModelDraft(result.hive_fallback_model ?? '')
          setHiveFallbackProviderError(null)
        } else {
          setHiveFallbackProviderError(result.error_code || 'Не удалось сохранить резервную модель Улья.')
        }
        return
      }
      if (result.setting_key === 'proactive_evolution_enabled') {
        if (typeof result.proactive_evolution_enabled === 'boolean') {
          setProactiveEvolutionEnabled(result.proactive_evolution_enabled)
        }
        setProactiveEvolutionError(result.accepted ? null : (result.error_code || 'Не удалось сохранить настройку.'))
        return
      }
      if (typeof result.daily_budget_usd === 'number' && Number.isFinite(result.daily_budget_usd)) {
        if (result.accepted) {
          setDailyBudgetDraft(String(result.daily_budget_usd))
          setDailyBudgetError(null)
        } else {
          setDailyBudgetError(result.error_code || 'Дневной лимит отклонён Core.')
        }
      }
      if (!result.accepted && !result.error_code && !result.eva_workspace_path) {
        setEvaWorkspacePath('')
        setEvaWorkspaceDraft('')
        setEvaWorkspaceError('Путь не задан; инструменты саморазвития выключены.')
        return
      }
      if (!result.accepted || result.error_code) {
        if (typeof result.daily_budget_usd !== 'number') {
          setEvaWorkspacePath('')
          setEvaWorkspaceDraft('')
          setEvaWorkspaceError(result.error_code || 'Путь к репозиторию отклонён Core.')
        }
        return
      }
      const nextEvaPath = result.eva_workspace_path ?? ''
      setEvaWorkspacePath(nextEvaPath)
      setEvaWorkspaceDraft(nextEvaPath)
      if (nextEvaPath.length > 0) {
        setWatchedRepositories((current) => current.filter((item) => !sameRepositoryPath(item, nextEvaPath)))
      }
      setEvaWorkspaceError(null)
    } catch {
      setEvaWorkspaceError('Core вернул некорректный ответ о пути репозитория.')
    }
  }, [hiveSettingsResult])
  const hiveSettingsValidationResult = latest(events, 'hive_settings.validated')
  useEffect(() => {
    if (!hiveSettingsValidationResult) return
    try {
      const result = JSON.parse(hiveSettingsValidationResult.payload) as {
        valid?: boolean
        eva_workspace_path?: string | null
        error_code?: string
      }
      if (result.valid) {
        setEvaWorkspaceValidation('valid')
        setEvaWorkspaceValidationMessage(null)
      } else {
        setEvaWorkspaceValidation('invalid')
        setEvaWorkspaceValidationMessage(result.error_code || 'Путь не является валидным Git-репозиторием.')
      }
    } catch {
      setEvaWorkspaceValidation('invalid')
      setEvaWorkspaceValidationMessage('Core вернул некорректный ответ о проверке пути.')
    }
  }, [hiveSettingsValidationResult])

  // Метка времени ставится в момент, когда оболочка получила событие: Core не
  // публикует wall-clock для каждого события, а выдумывать время нельзя.
  useEffect(() => {
    setObservedAt((current) => {
      let next = new Map(current)
      if (next.size > 2000) {
        next = new Map()
        for (const event of events) {
          const key = coreEventKey(event)
          const seen = current.get(key)
          if (seen) next.set(key, seen)
        }
      }
      let changed = next.size !== current.size
      for (const event of events) {
        const key = coreEventKey(event)
        if (next.has(key)) continue
        next.set(key, new Date().toLocaleTimeString('ru-RU', { hour12: false }))
        changed = true
      }
      return changed ? next : current
    })
  }, [events])

  const requestSnapshot = useCallback((): void => {
    if (!api) return
    void api.invoke('core.getHiveState', {}).catch(() => undefined)
    void api.invoke('core.getHiveObservationStatus', {}).catch(() => undefined)
  }, [api])

  // Snapshot polling is read-only and bounded by the chosen interval. The autonomous
  // observation cycle runs independently inside Core/daemon.
  useEffect(() => {
    if (!api || !connected) return
    requestSnapshot()
    const timer = window.setInterval(requestSnapshot, preference.intervalMs)
    return () => window.clearInterval(timer)
  }, [api, connected, preference.intervalMs, requestSnapshot])

  useEffect(() => {
    if (!api) {
      setHistoryPrompts([])
      return
    }
    let cancelled = false
    void api.invoke('chat.open', { chatId: HIVE_SYSTEM_CHAT_ID }).then((outcome) => {
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
  }, [api])

  useEffect(() => {
    hiveSeenPagesRef.current.clear()
    if (!api || !connected) return

    const requestHistory = (): void => {
      void api.invoke('core.getConversationEvents', {
        conversationId: HIVE_SYSTEM_CHAT_ID,
        limit: 200
      })
    }
    requestHistory()
    return () => {
      hiveSubscriptionRef.current = null
    }
  }, [api, connected])

  useEffect(() => {
    if (!api || !connected) return
    const subscriptionKey = `${HIVE_SYSTEM_CHAT_ID}:${connection}`
    if (hiveSubscriptionRef.current?.key === subscriptionKey) return
    const current = hiveConversationRef.current
    const afterSequence = current.sync.state === 'gap'
      ? current.lastSequence
      : current.sync.state === 'cursor-expired'
        ? Math.max(0, current.sync.earliestAvailableSequence - 1)
        : current.lastSequence
    const requestId = `${subscriptionKey}:${Date.now()}`
    hiveSubscriptionRef.current = { key: subscriptionKey, requestId }
    void api.invoke('core.subscribeConversationEvents', {
      conversationId: HIVE_SYSTEM_CHAT_ID,
      afterSequence,
      limit: 200
    }).then((outcome) => {
      if (!outcome.ok && hiveSubscriptionRef.current?.requestId === requestId) {
        hiveSubscriptionRef.current = null
      }
    }).catch(() => {
      if (hiveSubscriptionRef.current?.requestId === requestId) {
        hiveSubscriptionRef.current = null
      }
    })
  }, [api, connected, connection])

  useEffect(() => {
    const pages: NonNullable<CoreEvent['conversationEventLog']>[] = []
    for (const event of events) {
      const page = event.conversationEventLog
      if (page?.conversationId !== HIVE_SYSTEM_CHAT_ID) continue
      const key = event.sequenceId === 0
        ? `${event.coreInstanceId ?? 'legacy'}:${event.sessionEpoch ?? 0}:${page.operation}:${page.conversationId}:${page.errorCode}:${page.events.map((item) => `${item.sequence}:${item.eventId}`).join(',')}`
        : `${event.coreInstanceId ?? 'legacy'}:${event.sessionEpoch ?? 0}:${event.sequenceId}:${page.operation}:${page.oldestSequence}:${page.newestSequence}`
      if (hiveSeenPagesRef.current.has(key)) continue
      hiveSeenPagesRef.current.add(key)
      if (hiveSeenPagesRef.current.size > 1000) {
        const oldest = hiveSeenPagesRef.current.values().next().value
        if (oldest) hiveSeenPagesRef.current.delete(oldest)
      }
      pages.push(page)
    }
    if (pages.length === 0) return

    let next = hiveConversationRef.current
    let acceptError = ''
    for (const page of [...pages].reverse()) {
      if (page.errorCode === 'cursor_expired') {
        next = { ...next, sync: { state: 'cursor-expired', earliestAvailableSequence: page.earliestAvailableSequence } }
        continue
      }
      if (page.operation === 'accept' && page.errorCode !== '') {
        acceptError = page.errorCode
        continue
      }
      if (page.errorCode !== '') continue
      if (next.sync.state === 'cursor-expired' && page.events[0]?.sequence === page.earliestAvailableSequence) {
        next = resumeAtRetainedBoundary(next, page.earliestAvailableSequence)
      }
      const known = conversationEvents(next)
      const olderPage = known.length > 0 && page.events.length > 0
        && page.events.every((item) => item.sequence < (known[0]?.sequence ?? Number.MAX_SAFE_INTEGER))
      if (olderPage) {
        next = prependConversationEvents(next, page.events)
      } else if (page.operation === 'live' || page.operation === 'subscribed') {
        next = applyConversationEvents(next, page.events)
      } else if (!next.historyReady) {
        next = applyInitialConversationHistory(next, page.events)
      } else {
        next = applyConversationEvents(next, page.events)
      }
    }
    if (next.sync.state === 'gap') {
      void api?.invoke('core.getConversationEvents', {
        conversationId: HIVE_SYSTEM_CHAT_ID,
        afterSequence: next.lastSequence,
        limit: 200
      })
    }
    if (acceptError !== '') {
      // A rejected `startTask` never reaches the task stream, so it cannot
      // produce a terminal event; without this the indicator would stay on.
      const started = new Set(conversationEvents(next).map((item) => item.taskId))
      setPendingTaskIds((current) => {
        const remaining = [...current].filter((taskId) => started.has(taskId))
        return remaining.length === current.size ? current : new Set(remaining)
      })
      setCommandError(hiveAcceptErrorMessage(acceptError))
    }
    if (next === hiveConversationRef.current) return
    hiveConversationRef.current = next
    setHiveConversationLogEvents(conversationEvents(next))
  }, [api, events])

  useEffect(() => {
    if (pendingTaskIds.size === 0) return
    const terminalTypes = new Set(['task.completed', 'task.failed', 'task.stopped'])
    const chatEvents = projectHiveChatEvents(hiveConversationLogEvents)
    setPendingTaskIds((current) => {
      let changed = false
      const next = new Set(current)
      for (const taskId of current) {
        if (chatEvents.some((event) => event.taskId === taskId && terminalTypes.has(event.eventType))) {
          next.delete(taskId)
          changed = true
        }
      }
      return changed ? next : current
    })
  }, [hiveConversationLogEvents, pendingTaskIds.size])

  const hiveEvents = useMemo(
    () => [
      ...projectHiveChatEvents(hiveConversationLogEvents),
        ...events.filter((event) => event.eventType === 'hive_state.snapshot'
          || event.eventType.startsWith('hive.')
          || event.eventType.startsWith('eva_pulse.'))
    ],
    [events, hiveConversationLogEvents]
  )
  // A task accepted from an EvaPulse signal has no chat binding, so its
  // approval and terminal state reach the Hive view only by the Hive-owned task
  // prefix. The projection stays separate from `hiveEvents` so pane counters
  // and logs keep counting Hive-owned work only.
  const hiveApprovalEvents = useMemo(
    () => [
      ...hiveEvents,
      ...events.filter((event) => event.taskId.startsWith('eva-pulse-'))
    ],
    [events, hiveEvents]
  )
  const hiveChatEvents = useMemo(
    () => projectHiveChatEvents(hiveConversationLogEvents),
    [hiveConversationLogEvents]
  )

  const activeApprovals = useMemo(() => {
    const terminalTypes = new Set(['task.completed', 'task.failed', 'task.stopped'])
    const resolved = new Set([...resolvedApprovalIds, ...resolvedLocally])
    const byId = new Map<string, { request: ApprovalRequestView; sequence: number }>()
    const newestFirst = [...hiveApprovalEvents].sort((left, right) => right.sequenceId - left.sequenceId)
    const terminated = (event: CoreEvent): boolean =>
      hiveApprovalEvents.some((candidate) => candidate.taskId === event.taskId
        && candidate.sequenceId > event.sequenceId && terminalTypes.has(candidate.eventType))
    for (const event of newestFirst) {
      if (event.eventType !== 'approval.required') continue
      if (terminated(event)) continue
      const request = parseApprovalRequest(event)
      if (!request || resolved.has(request.approvalId)) continue
      if (!byId.has(request.approvalId)) byId.set(request.approvalId, { request, sequence: event.sequenceId })
    }
    return [...byId.entries()]
      .filter(([approvalId]) => !resolved.has(approvalId))
      .map(([, item]) => item.request)
  }, [hiveApprovalEvents, resolvedApprovalIds, resolvedLocally])

  const approvalsFromConversationLog = useMemo(() => {
    const terminalTypes = new Set(['task.completed', 'task.failed', 'task.stopped'])
    const resolved = new Set([...resolvedApprovalIds, ...resolvedLocally])
    const requests = new Map<string, { request: ApprovalRequestView; sequence: number }>()
    const orderedEvents = [...hiveChatEvents].sort((left, right) => right.sequenceId - left.sequenceId)
    for (const event of orderedEvents) {
      if (event.eventType !== 'approval.required') continue
      if (orderedEvents.some((candidate) => candidate.taskId === event.taskId
        && candidate.sequenceId > event.sequenceId && terminalTypes.has(candidate.eventType))) continue
      const request = parseApprovalRequest(event)
      if (!request || resolved.has(request.approvalId) || requests.has(request.approvalId)) continue
      requests.set(request.approvalId, { request, sequence: event.sequenceId })
    }
    return [...requests.values()].map((entry) => entry.request)
  }, [hiveChatEvents, resolvedApprovalIds, resolvedLocally])
  const effectiveActiveApprovals = useMemo(() => {
    const merged = new Map<string, ApprovalRequestView>()
    for (const request of activeApprovals) merged.set(request.approvalId, request)
    for (const request of approvalsFromConversationLog) merged.set(request.approvalId, request)
    return [...merged.values()]
  }, [activeApprovals, approvalsFromConversationLog])

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

  const selectedProviderKey = currentProviderKey(providerSummary)

  const updatePreference = useCallback((next: HivePreference): void => {
    setPreference(next)
    saveHivePreference(next)
  }, [])

  const validateEvaWorkspacePath = useCallback(async (): Promise<void> => {
    if (!api) return
    setEvaWorkspaceValidation('checking')
    setEvaWorkspaceValidationMessage(null)
    try {
      const submittedPath = evaWorkspaceDraft.trim()
      const outcome = await api.invoke('core.validateEvaWorkspacePath', { path: submittedPath })
      if (!outcome.ok) {
        setEvaWorkspaceValidation('invalid')
        setEvaWorkspaceValidationMessage(outcome.message)
      }
    } catch {
      setEvaWorkspaceValidation('invalid')
      setEvaWorkspaceValidationMessage('Не удалось проверить путь: Core не ответил.')
    }
  }, [api, evaWorkspaceDraft])

  const saveEvaWorkspacePath = useCallback(async (): Promise<void> => {
    if (!api) return
    setEvaWorkspaceBusy(true)
    setEvaWorkspaceError(null)
    try {
      const submittedPath = evaWorkspaceDraft.trim()
      const outcome = await api.invoke('core.setHiveSetting', { key: 'eva_workspace_path', value: submittedPath })
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

  const saveDailyBudget = useCallback(async (): Promise<void> => {
    if (!api) return
    const raw = dailyBudgetDraft.trim()
    const parsed = Number(raw)
    if (raw.length === 0 || !Number.isFinite(parsed) || parsed < 0 || parsed > 10_000) {
      setDailyBudgetError('Укажите число от 0 до 10000.')
      return
    }
    setDailyBudgetBusy(true)
    setDailyBudgetError(null)
    setDailyBudgetSaved(false)
    try {
      const outcome = await api.invoke('core.setHiveSetting', {
        key: 'daily_budget_usd',
        value: String(parsed)
      })
      if (!outcome.ok) {
        setDailyBudgetError(outcome.message)
        return
      }
      setDailyBudgetSaved(true)
      requestSnapshot()
    } catch {
      setDailyBudgetError('Не удалось сохранить дневной лимит: Core не ответил.')
    } finally {
      setDailyBudgetBusy(false)
    }
  }, [api, dailyBudgetDraft, requestSnapshot])

  const saveHiveRequestLimits = useCallback(async (): Promise<void> => {
    if (!api) return
    const resolveLimits = (draft: HiveRequestLimitsDraft): ProviderLimits => ({
      rpm: draft.rpm,
      rph: draft.rph ?? (draft.rpm > 0 ? draft.rpm * 60 : 0),
      rpd: draft.rpd ?? (draft.rpm > 0 ? draft.rpm * 1440 : 0)
    })
    const primaryLimits = resolveLimits(hivePrimaryLimitsDraft)
    const fallbackLimits = resolveLimits(hiveFallbackLimitsDraft)
    const values = [primaryLimits.rpm, primaryLimits.rph, primaryLimits.rpd,
      fallbackLimits.rpm, fallbackLimits.rph, fallbackLimits.rpd]
    const primaryQuota = Number(hivePrimaryQuotaDraft)
    const fallbackQuota = Number(hiveFallbackQuotaDraft)
    if (values.some((value) => !Number.isInteger(value) || value < 0 || value > 4_294_967_295)
      || !Number.isSafeInteger(primaryQuota) || primaryQuota < 0 || primaryQuota > 1_000_000_000_000
      || !Number.isSafeInteger(fallbackQuota) || fallbackQuota < 0 || fallbackQuota > 1_000_000_000_000) {
      setHiveLimitsError('Лимиты запросов должны быть целыми числами от 0 до 4294967295, квота токенов — целым числом от 0 до 1000000000000.')
      return
    }
    setHiveLimitsBusy(true)
    setHiveLimitsError(null)
    setHiveLimitsSaved(false)
    try {
      const value = JSON.stringify({
        primary_limits: primaryLimits,
        fallback_limits: fallbackLimits,
        primary_token_quota: primaryQuota,
        fallback_token_quota: fallbackQuota,
        primary_quota_reset_utc_midnight: hivePrimaryQuotaResetDraft,
        fallback_quota_reset_utc_midnight: hiveFallbackQuotaResetDraft
      })
      const outcome = await api.invoke('core.setHiveSetting', { key: 'hive_request_limits', value })
      if (!outcome.ok) {
        setHiveLimitsError(outcome.message)
        setHiveLimitsBusy(false)
        return
      }
      requestSnapshot()
    } catch {
      setHiveLimitsError('Не удалось сохранить лимиты Улья: Core не ответил.')
      setHiveLimitsBusy(false)
    }
  }, [api, hiveFallbackLimitsDraft, hiveFallbackQuotaDraft, hiveFallbackQuotaResetDraft, hivePrimaryLimitsDraft, hivePrimaryQuotaDraft, hivePrimaryQuotaResetDraft, requestSnapshot])

  const toggleProactiveEvolution = useCallback(async (nextEnabled: boolean): Promise<void> => {
    if (!api) return
    setProactiveEvolutionBusy(true)
    setProactiveEvolutionError(null)
    setProactiveEvolutionEnabled(nextEnabled)
    try {
      const outcome = await api.invoke('core.setHiveSetting', {
        key: 'proactive_evolution_enabled',
        value: nextEnabled ? 'true' : 'false'
      })
      if (!outcome.ok) {
        setProactiveEvolutionEnabled(!nextEnabled)
        setProactiveEvolutionError(outcome.message)
        return
      }
      requestSnapshot()
    } catch {
      setProactiveEvolutionEnabled(!nextEnabled)
      setProactiveEvolutionError('Не удалось сохранить настройку: Core не ответил.')
    } finally {
      setProactiveEvolutionBusy(false)
    }
  }, [api, requestSnapshot])

  const addWatchedRepository = useCallback(async (): Promise<void> => {
    if (!api) return
    const trimmed = newRepoDraft.trim()
    if (trimmed.length === 0) {
      setWatchedReposError('Укажи путь к Git-репозиторию.')
      return
    }
    if (sameRepositoryPath(trimmed, evaWorkspacePath) || sameRepositoryPath(trimmed, evaWorkspaceDraft)) {
      setWatchedReposError('Папка исходников Евы задаётся отдельно выше и не добавляется в сторонние проекты.')
      return
    }
    if (watchedRepositories.some((repo) => sameRepositoryPath(repo, trimmed))) {
      setWatchedReposError('Этот проект уже добавлен в список.')
      return
    }
    setWatchedReposBusy(true)
    setWatchedReposError(null)
    try {
      const validation = await api.invoke('core.validateEvaWorkspacePath', { path: trimmed })
      if (!validation.ok) {
        setWatchedReposError(validation.message)
        return
      }
      const nextRepos = [...watchedRepositories, trimmed]
      const outcome = await api.invoke('core.setHiveSetting', {
        key: 'watched_repositories',
        value: JSON.stringify(nextRepos)
      })
      if (!outcome.ok) {
        setWatchedReposError(outcome.message)
        return
      }
      setWatchedRepositories(nextRepos)
      setNewRepoDraft('')
      requestSnapshot()
    } catch {
      setWatchedReposError('Не удалось добавить проект: Core не ответил.')
    } finally {
      setWatchedReposBusy(false)
    }
  }, [api, evaWorkspaceDraft, evaWorkspacePath, newRepoDraft, requestSnapshot, watchedRepositories])

  const removeWatchedRepository = useCallback(async (targetPath: string): Promise<void> => {
    if (!api) return
    setWatchedReposBusy(true)
    setWatchedReposError(null)
    try {
      const nextRepos = watchedRepositories.filter((item) => item !== targetPath)
      const outcome = await api.invoke('core.setHiveSetting', {
        key: 'watched_repositories',
        value: JSON.stringify(nextRepos)
      })
      if (!outcome.ok) {
        setWatchedReposError(outcome.message)
        return
      }
      setWatchedRepositories(nextRepos)
      requestSnapshot()
    } catch {
      setWatchedReposError('Не удалось удалить проект: Core не ответил.')
    } finally {
      setWatchedReposBusy(false)
    }
  }, [api, requestSnapshot, watchedRepositories])

  const saveHiveProviderSettings = useCallback(async (): Promise<void> => {
    if (!api) return
    setHiveProviderBusy(true)
    setHiveProviderError(null)
    setHiveProviderSaved(false)
    try {
      const profileOutcome = await api.invoke('core.setHiveSetting', {
        key: 'hive_provider_profile',
        value: hiveProviderProfile.trim()
      })
      if (!profileOutcome.ok) {
        setHiveProviderError(profileOutcome.message)
        return
      }
      const modelOutcome = await api.invoke('core.setHiveSetting', {
        key: 'hive_model',
        value: hiveModelDraft.trim()
      })
      if (!modelOutcome.ok) {
        setHiveProviderError(modelOutcome.message)
        return
      }
      setHiveProviderSaved(true)
      requestSnapshot()
    } catch {
      setHiveProviderError('Не удалось сохранить провайдера Улья: Core не ответил.')
    } finally {
      setHiveProviderBusy(false)
    }
  }, [api, hiveModelDraft, hiveProviderProfile, requestSnapshot])

  const saveHiveApiKey = useCallback(async (): Promise<void> => {
    if (!api) return
    const trimmedKey = hiveApiKeyDraft.trim()
    setHiveApiKeyBusy(true)
    setHiveApiKeyError(null)
    setHiveApiKeySaved(false)
    try {
      const outcome = await api.invoke('core.setHiveSetting', {
        key: 'hive_api_key',
        value: trimmedKey
      })
      if (!outcome.ok) {
        setHiveApiKeyError(outcome.message)
        return
      }
      setHiveApiKeyDraft('')
      setIsEditingMainKey(false)
      setHiveApiKeySaved(true)
      if (providerSummary) {
        applyProviderSummary({ ...providerSummary, hiveApiKeyConfigured: trimmedKey.length > 0 })
      } else {
        void refreshProviderSummary()
      }
      requestSnapshot()
    } catch {
      setHiveApiKeyError('Не удалось сохранить API-ключ Улья: Core не ответил.')
    } finally {
      setHiveApiKeyBusy(false)
    }
  }, [api, applyProviderSummary, hiveApiKeyDraft, providerSummary, refreshProviderSummary, requestSnapshot])

  const saveHiveFallbackProviderSettings = useCallback(async (): Promise<void> => {
    if (!api) return
    setHiveFallbackProviderBusy(true)
    setHiveFallbackProviderError(null)
    setHiveFallbackProviderSaved(false)
    try {
      const profileOutcome = await api.invoke('core.setHiveSetting', {
        key: 'hive_fallback_provider_profile',
        value: hiveFallbackProviderProfile.trim()
      })
      if (!profileOutcome.ok) {
        setHiveFallbackProviderError(profileOutcome.message)
        return
      }
      const modelOutcome = await api.invoke('core.setHiveSetting', {
        key: 'hive_fallback_model',
        value: hiveFallbackModelDraft.trim()
      })
      if (!modelOutcome.ok) {
        setHiveFallbackProviderError(modelOutcome.message)
        return
      }
      setHiveFallbackProviderSaved(true)
      requestSnapshot()
    } catch {
      setHiveFallbackProviderError('Не удалось сохранить резервного провайдера Улья: Core не ответил.')
    } finally {
      setHiveFallbackProviderBusy(false)
    }
  }, [api, hiveFallbackModelDraft, hiveFallbackProviderProfile, requestSnapshot])

  const saveHiveFallbackApiKey = useCallback(async (): Promise<void> => {
    if (!api) return
    const trimmedKey = hiveFallbackApiKeyDraft.trim()
    setHiveFallbackApiKeyBusy(true)
    setHiveFallbackApiKeyError(null)
    setHiveFallbackApiKeySaved(false)
    setHiveFallbackCatalogError(null)
    try {
      const outcome = await api.invoke('core.setHiveSetting', {
        key: 'hive_fallback_api_key',
        value: trimmedKey
      })
      if (!outcome.ok) {
        setHiveFallbackApiKeyError(outcome.message)
        return
      }
      setHiveFallbackApiKeyDraft('')
      setIsEditingFallbackKey(false)
      setHiveFallbackApiKeySaved(true)
      setHiveFallbackCatalogError(null)
      if (providerSummary) {
        applyProviderSummary({ ...providerSummary, hiveFallbackApiKeyConfigured: trimmedKey.length > 0 })
      } else {
        void refreshProviderSummary()
      }
      requestSnapshot()
      if (trimmedKey.length > 0 && hiveFallbackProviderProfile.trim().length > 0) {
        void loadHiveFallbackCatalog()
      }
    } catch {
      setHiveFallbackApiKeyError('Не удалось сохранить резервный API-ключ Улья: Core не ответил.')
    } finally {
      setHiveFallbackApiKeyBusy(false)
    }
  }, [api, applyProviderSummary, hiveFallbackApiKeyDraft, hiveFallbackProviderProfile, loadHiveFallbackCatalog, providerSummary, refreshProviderSummary, requestSnapshot])

  const openHiveChat = useCallback(async (): Promise<void> => {
    if (!api) return
    setCommandError(null)
    try {
      const outcome = await api.invoke('chat.open', { chatId: HIVE_SYSTEM_CHAT_ID })
      if (!outcome.ok) {
        setCommandError(outcome.message)
        return
      }
      if (outcome.value) {
        setHistoryPrompts(
          outcome.value.messages.map((message, index) => ({
            taskId: message.taskId,
            text: message.prompt,
            sequenceHint: index + 1
          }))
        )
      }
      onChatOpened?.(HIVE_SYSTEM_CHAT_ID)
    } catch {
      setCommandError('Не удалось открыть чат Улья.')
    }
  }, [api, onChatOpened])

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
      setLocalPrompts((current) => [
        ...current,
        { taskId: nextTaskId, text, sequenceHint: current.length + 1 }
      ])
      setPendingTaskIds((current) => new Set(current).add(nextTaskId))
      setChatDraft('')

      const outcome = await api.invoke('core.startTask', {
        taskId: nextTaskId,
        prompt: text,
        workspacePath: '',
        conversationId: HIVE_SYSTEM_CHAT_ID,
        clientMessageId,
        preferredRouteHint: 'hive',
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
        chatId: HIVE_SYSTEM_CHAT_ID,
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
      setLocalPrompts((current) => current.filter((prompt) => prompt.taskId !== nextTaskId))
      setCommandError('Не удалось отправить сообщение Королеве Еве.')
    } finally {
      setSendingMessage(false)
    }
  }, [api, chatDraft, connected, onChatTouched, sendingMessage])

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
  const showingConnectedIdleState = connected && hive !== null && hive.activeAgents.length === 0
  // An invalid legacy snapshot is intentionally rendered as an empty projection
  // for compatibility. A missing projection, however, is a loading state: never
  // present transport defaults as real metrics while Core is unavailable.
  const hiveLoading = hive === null && (!connected || !hasHiveSnapshot)
  const workerSummaryLabel = showingConnectedIdleState ? '— / —' : `${agentsOnline} / ${capacityLabel}`
  const paused = hive?.queen.status === 'budget_exhausted' && budget.historyAvailable
  const queenActive = hive?.queen.status === 'active'
  const waitingApproval = effectiveActiveApprovals.length > 0
  const queenStatusLabel = paused
    ? 'Ожидает (лимиты)'
    : waitingApproval
      ? 'Ждёт подтверждения'
      : queenActive
        ? 'Активна'
        : 'Наблюдает'
  const wakeAtMs = hive?.queen.wakeAtMs ?? null
  const observationNextWakeAtMs = observationState?.nextWakeAtMs ?? hive?.nextWakeAtMs ?? null
  const observationStatus = observationState?.status ?? hive?.observationStatus ?? ''
  const nextObservationAtMs = observationNextWakeAtMs ?? (paused ? wakeAtMs : null)
  const observationStatusLabel = observationStatus === 'observing' || observationStatus === 'active'
    ? 'Улей наблюдает'
    : observationStatus === 'waiting_for_limits'
      ? 'Улей ожидает (лимиты)'
      : observationStatus === 'waiting_for_tasks' || observationStatus === 'disabled'
        ? 'Улей ожидает (задачи)'
        : observationStatus === 'sleeping'
          ? 'Ошибка наблюдения'
          : 'Ожидание...'
  const averageTaskTimeSec = hive?.metrics.averageTaskTimeSec ?? 0
  const averageTimeLabel = `${averageTaskTimeSec}с`
  const dailyBudgetUsd = hive?.dailyBudgetUsd ?? 0
  const spentUsd = budget.dailyCostAvailable && budget.dailyCostNanoUsd !== null ? budget.dailyCostNanoUsd / 1_000_000_000 : null
  const spendPercent = spentUsd === null || dailyBudgetUsd <= 0 ? 0 : Math.min(100, Math.round((spentUsd / dailyBudgetUsd) * 100))
  const remainingUsd = Math.max(0, dailyBudgetUsd - (spentUsd ?? 0))
  const spentLabel = budget.dailyCostAvailable && budget.dailyCostNanoUsd !== null ? formatMoney(budget.dailyCostNanoUsd) : '—'
  const requestSummary = [
    `${budget.historyAvailable ? budget.requestsLastMinute : '—'}${budget.limits.rpm > 0 ? ` / ${budget.limits.rpm}` : ''} ${translate('за минуту')}`,
    `${budget.historyAvailable ? budget.requestsLastHour : '—'}${budget.limits.rph > 0 ? ` / ${budget.limits.rph}` : ''} ${translate('за час')}`,
    `${budget.historyAvailable ? budget.requestsLastDay : '—'}${budget.limits.rpd > 0 ? ` / ${budget.limits.rpd}` : ''} ${translate('за сутки')}`
  ].join(' · ')
  const configuredCaps = budget.limits.rpm > 0 || budget.limits.rph > 0 || budget.limits.rpd > 0
  const snapshotBudgetLabel = budget.provider === 'literouter' || budget.provider === 'openrouter'
    ? `LiteRouter${budget.model.toLowerCase().includes(':free') ? ' (Free)' : ''}`
    : budget.provider === 'rout'
      ? 'Rout (Free)'
      : budget.provider.toLowerCase().includes('deepseek')
        ? 'DeepSeek API'
        : budget.provider || translate('провайдер не выбран')
  const normalizedBudgetProvider = budget.provider.trim().toLowerCase()
  const isRoutProvider = normalizedBudgetProvider === 'rout'
    || (normalizedBudgetProvider.length === 0 && selectedProviderKey === 'openai_compatible:rout')
  const routmyConfigured = budget.fallbackProvider === 'rout'
    || providerSummary?.compatibleProfiles?.rout?.configured === true
  const fallbackLabel = budget.fallbackProvider === 'rout'
    ? 'Rout'
    : budget.fallbackProvider === 'literouter'
      ? 'LiteRouter'
      : budget.fallbackProvider
  const effectiveFallbackModel = budget.fallbackModel || hive?.hiveFallbackModel || null
  const routmyStatusText = budget.provider === 'rout'
    ? `${translate('Осталось токенов:')} ${budget.dailyTokensRemaining ?? budget.dailyTokenQuota ?? 0}${budget.quotaResetUtcMidnight ? ` ${translate('до 00:00 UTC')}` : ''}`
    : (budget.provider === 'literouter' || budget.provider === 'openrouter') && routmyConfigured
      ? `${translate('Резерв: Rout')} (${budget.fallbackTokensRemaining ?? 0} ${translate('токенов')})${effectiveFallbackModel ? ` · ${translate('Модель резерва:')} ${effectiveFallbackModel}` : ''}`
      : fallbackLabel
        ? `${translate('Резерв:')} ${fallbackLabel}${budget.fallbackTokensRemaining !== null ? ` (${budget.fallbackTokensRemaining} ${translate('токенов')})` : ''}${effectiveFallbackModel ? ` · ${translate('Модель резерва:')} ${effectiveFallbackModel}` : ''}`
        : null
  const realLogs = hiveLogEntries(hiveEvents, observedAt, allLogs ? 40 : 6, allLogs, hiveChatTaskIds, observedAtForHiveEvent)
  const logs = realLogs
  const latestLogId = logs[0]?.id ?? null
  useEffect(() => {
    if (logListRef.current) {
      logListRef.current.scrollTop = 0
    }
  }, [latestLogId, allLogs])
  useEffect(() => {
    return () => {
      if (logsCopyTimerRef.current !== null) {
        clearTimeout(logsCopyTimerRef.current)
      }
    }
  }, [])
  const copyHiveLogs = useCallback(async (): Promise<void> => {
    const formatted = formatHiveLogsForClipboard(hiveEvents, observedAt, budget, hiveChatTaskIds, observedAtForHiveEvent)
    if (formatted.length === 0) return
    let copied = false
    try {
      if (api) {
        copied = await api.writeClipboardText(formatted)
      } else if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(formatted)
        copied = true
      }
    } catch {
      copied = false
    }
    setLogsCopyState(copied ? 'copied' : 'failed')
    if (logsCopyTimerRef.current !== null) {
      clearTimeout(logsCopyTimerRef.current)
    }
    logsCopyTimerRef.current = setTimeout(() => {
      setLogsCopyState('idle')
      logsCopyTimerRef.current = null
    }, 2000)
  }, [api, budget, hiveChatTaskIds, hiveEvents, observedAt, observedAtForHiveEvent])
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
    () => extractHiveChatMessages(hiveChatEvents, combinedPrompts, 5),
    [combinedPrompts, hiveChatEvents]
  )
  const activeApprovalTaskIds = useMemo(
    () => new Set(effectiveActiveApprovals.map((approval) => approval.taskId).filter(Boolean)),
    [effectiveActiveApprovals]
  )
  const queenWatchdogEvents = useMemo(
    () => hiveChatEvents.filter((event) =>
      event.eventType === 'task.started'
      || event.eventType === 'agent.message.delta'
      || event.eventType === 'task.completed'
      || event.eventType === 'task.failed'
    ),
    [hiveChatEvents]
  )
  useEffect(() => {
    const heartbeatTypes = new Set(['task.started', 'agent.message.delta'])
    const terminalTypes = new Set(['task.completed', 'task.failed'])
    const activeTasks = new Set<string>()
    const timers: ReturnType<typeof setTimeout>[] = []

    for (const event of queenWatchdogEvents) {
      if (!event.taskId) continue
      if (heartbeatTypes.has(event.eventType)) activeTasks.add(event.taskId)
      if (terminalTypes.has(event.eventType)) activeTasks.delete(event.taskId)
    }

    setTypingWatchdogExpiredTaskIds((current) => {
      const next = new Set([...current].filter((taskId) => activeTasks.has(taskId)))
      return next.size === current.size ? current : next
    })

    for (const taskId of activeTasks) {
      timers.push(setTimeout(() => {
        setTypingWatchdogExpiredTaskIds((current) => {
          if (current.has(taskId)) return current
          return new Set(current).add(taskId)
        })
      }, QUEEN_TYPING_WATCHDOG_MS))
    }

    return () => {
      for (const timer of timers) clearTimeout(timer)
    }
  }, [queenWatchdogEvents])
  const queenTyping = useMemo(
    () => isQueenThinking(hiveChatEvents, pendingTaskIds, sendingMessage, activeApprovalTaskIds, typingWatchdogExpiredTaskIds),
    [activeApprovalTaskIds, hiveChatEvents, pendingTaskIds, sendingMessage, typingWatchdogExpiredTaskIds]
  )
  const queenWorking = queenTyping && !paused && !waitingApproval
  const queenActivity = useMemo(
    () => (queenTyping ? queenCurrentActivity(hiveChatEvents) : ''),
    [hiveChatEvents, queenTyping]
  )
  useEffect(() => {
    if (!hiveChatOpen || hiveSettingsOpen || typeof window === 'undefined') return undefined
    const onKeyDown = (keyboardEvent: KeyboardEvent): void => {
      if (keyboardEvent.key === 'Escape') {
        setHiveChatOpen(false)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [hiveChatOpen, hiveSettingsOpen])
  const departmentPins = useMemo(
    () =>
      departments.map((department) => ({
        key: department.key,
        label: translate(department.label),
        subtitle: translate(department.subtitle),
        icon: department.icon,
        activeCount: department.activeCount,
        maxCapacity: department.maxCapacity
      })),
    [departments]
  )
  const visibleAgents = useMemo(() => {
    const all = hive?.activeAgents ?? []
    if (!selectedDepartment) return all
    return all.filter((agent) => {
      const matched = matchDepartmentCard(agent.role)
      return matched?.key === selectedDepartment || `role:${agent.role}` === selectedDepartment
    })
  }, [hive?.activeAgents, selectedDepartment])
  const selectedDepartmentLabel = useMemo(() => {
    if (!selectedDepartment) return null
    return departments.find((department) => department.key === selectedDepartment)?.label ?? null
  }, [departments, selectedDepartment])
  const toggleDepartmentFilter = useCallback((departmentKey: string): void => {
    setSelectedDepartment((current) => (current === departmentKey ? null : departmentKey))
  }, [])

  const queenCenterCard = (
    <div className="hive-queen-card">
      <div className="hive-queen-card__head">
        <div className="hive-queen-card__identity">
          <span className="hive-queen-card__emblem" aria-hidden="true"><HiveLotusMark size={20} /></span>
          <div>
            <p className="hive-queen__eyebrow">Eva core · {hive?.queen.name || 'EvoHime'}</p>
            <h2>{translate('Королева Ева')}</h2>
            <p className="hive-queen__role">{translate('Центр управления')}</p>
          </div>
        </div>
        <p className={`hive-queen__status hive-queen__status--${queenActive ? 'active' : 'observing'}`}>
          <span className="hive-dot" aria-hidden="true" />
          {translate(queenWorking ? 'Работает' : queenStatusLabel)}
        </p>
      </div>
      <label className="hive-settings-toggle" htmlFor="queen-autonomy-toggle">
        <span>{translate('Автономность Королевы')}</span>
        <input
          id="queen-autonomy-toggle"
          type="checkbox"
          role="switch"
          aria-label={translate('Автономность Королевы')}
          checked={proactiveEvolutionEnabled}
          disabled={!connected || proactiveEvolutionBusy}
          onChange={(changeEvent) => void toggleProactiveEvolution(changeEvent.target.checked)}
        />
      </label>
      {proactiveEvolutionError ? (
        <p role="alert" className="hive-console__error">{translate(proactiveEvolutionError)}</p>
      ) : null}
      <div className="hive-queen-card__foot">
        <p className="hive-queen__note">
          {paused
            ? `${translate(QUEEN_REASON_LABELS[hive?.queen.reason ?? ''] ?? hive?.queen.reason ?? 'лимит провайдера')} · ${formatWakeAt(wakeAtMs, Date.now())}`
            : queenWorking
              ? translate(queenActivity)
              : translate('Координирует агентов и выполняет задачи')}
        </p>
        <button
          type="button"
          className="hive-queen-card__chat-btn"
          onClick={() => setHiveChatOpen(true)}
        >
          <UiIcon name="message" size={14} />
          <span>{translate('Написать Еве')}</span>
        </button>
      </div>
    </div>
  )

  return (
    <section className="hive-console" aria-label={translate('Улей (Hive)')}>
      <div className="hive-stage">
        <section className="hive-stage__departments" aria-label={translate('Департаменты улья')}>
          <HiveCityScene
            status={observationStatus}
            departments={departmentPins}
            selectedDepartment={selectedDepartment}
            onSelectDepartment={toggleDepartmentFilter}
            queenWorking={queenWorking}
            queenOverlay={queenCenterCard}
            idleDepartmentsLabel={translate('Ожидание')}
            activeDepartmentsLabel={translate('Работает')}
          />
        </section>

        <header className="hive-hud-top">
          <div className="hive-hud-top__brand">
            <p className="hive-console__eyebrow">Hive / Eva</p>
            <h1>{translate('Улей (Hive)')}</h1>
            <p className="hive-hud-top__subtitle">{translate('Единый центр управления Ульем и Королевой Евой')}</p>
            <div className="hive-hud-top__dept-summary">
              <h2>{translate('Департаменты улья')}</h2>
              <span>
                {hiveLoading ? (
                  <HiveSkeleton className="hive-skeleton--text" />
                ) : showingConnectedIdleState ? (
                  translate('Нет активных агентов')
                ) : (
                  `${translate('Активно:')} ${agentsOnline} / ${capacityLabel}`
                )}
              </span>
            </div>
          </div>

          <div className="hive-hud-top__right">
            <div className="hive-console__controls">
              {!connected ? (
                <span className={`status-pill status-pill--${connection}`}>
                  {translate(HIVE_CONNECTION_LABELS[connection])}
                </span>
              ) : null}
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
              <button type="button" className="hive-console__refresh" onClick={requestSnapshot} disabled={!connected}>
                {translate('Обновить сейчас')}
              </button>
              <button
                type="button"
                className="hive-settings-trigger"
                aria-label={translate('Настройки улья')}
                title={translate('Настройки улья')}
                aria-expanded={hiveSettingsOpen}
                onClick={() => setHiveSettingsOpen(true)}
              >
                <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <circle cx="12" cy="12" r="3" />
                  <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z" />
                </svg>
              </button>
            </div>

            <aside className="hive-hud-status" aria-label={translate('Метрики улья')}>
              <div className="hive-panel__head">
                <h2>{translate('Метрики улья')}</h2>
                <span className="hive-hud-status__badge">{translate(observationStatusLabel)}</span>
              </div>
              <div className="hive-metrics-summary">
                <div className="hive-metrics">
                  <EfficiencyRing percent={efficiency} />
                </div>
                <ul className="hive-stat-list">
                  <li><span>{translate('Агентов онлайн')}</span><strong>{hiveLoading ? <HiveSkeleton className="hive-skeleton--number" /> : agentsOnline}</strong></li>
                  <li><span>{translate('Задач в работе')}</span><strong>{hiveLoading ? <HiveSkeleton className="hive-skeleton--number" /> : activeTasks}</strong></li>
                  <li>
                    <span>{translate('Задач сегодня')}</span>
                    <strong>{hiveLoading ? <HiveSkeleton className="hive-skeleton--number" /> : hive?.metrics.tasksToday ?? 0}</strong>
                  </li>
                  <li>
                    <span>{translate('Среднее время')}</span>
                    <strong>{hiveLoading ? <HiveSkeleton className="hive-skeleton--number" /> : averageTimeLabel}</strong>
                  </li>
                </ul>
              </div>
              <dl className="hive-queen__footer">
                <div>
                  <dt>{translate('Воркеры')}</dt>
                  <dd>{workerSummaryLabel}</dd>
                </div>
                {nextObservationAtMs !== null ? (
                  <div>
                    <dt>{translate('Следующая проверка окружения')}</dt>
                    <dd>{formatClock(nextObservationAtMs)}</dd>
                  </div>
                ) : observationStatus === 'observing' || observationStatus === 'active' ? (
                  <div>
                    <dt>{translate('Следующая проверка окружения')}</dt>
                    <dd>{translate('Наблюдение активно')}</dd>
                  </div>
                ) : null}
              </dl>
            </aside>
          </div>
        </header>

        {effectiveActiveApprovals.length > 0 || commandError ? (
          <div className="hive-hud-alerts">
            {effectiveActiveApprovals.map((approval) => (
              <ApprovalRequest
                key={approval.approvalId}
                request={approval}
                busy={resolvingApprovalIds.has(approval.approvalId)}
                onResolve={(granted) => void approvalResolved(approval, granted)}
              />
            ))}
            {commandError ? <p role="alert" className="hive-console__error">{translate(commandError)}</p> : null}
          </div>
        ) : null}

        <div className="hive-dock">
          <section className="hive-dock__card hive-dock__card--agents" aria-label={translate('Активные агенты')}>
            <div className="hive-panel__head">
              <h3>{translate('Активные агенты')} ({agentsOnline})</h3>
              {selectedDepartmentLabel ? (
                <button
                  type="button"
                  className="hive-filter-chip"
                  onClick={() => setSelectedDepartment(null)}
                >
                  <span>{translate(selectedDepartmentLabel)}</span>
                  <span aria-hidden="true">×</span>
                </button>
              ) : (
                <span>{translate('глубина и подагенты')}</span>
              )}
            </div>
            {hiveLoading ? (
              <ul className="hive-agent-list hive-agent-list--loading" aria-label={translate('Загрузка агентов')}>
                {[1, 2, 3].map((item) => <li key={item}><HiveSkeleton className="hive-skeleton--avatar" /><HiveSkeleton className="hive-skeleton--agent" /></li>)}
              </ul>
            ) : hive && visibleAgents.length > 0 ? (
              <ul className="hive-agent-list">
                {visibleAgents.slice(0, 8).map((agent) => (
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
                {selectedDepartment
                  ? translate('В выбранном департаменте активных агентов нет.')
                  : connected
                    ? translate('Активных агентов нет.')
                    : translate('Core недоступен — ожидается актуальная проекция.')}
              </p>
            )}

            <section className="hive-watched" aria-label={translate('Проекты под наблюдением')}>
              <div className="hive-panel__head">
                <h4>{translate('Проекты под наблюдением')}</h4>
                <span>{hive?.watchedRepositories.length ?? 0}</span>
              </div>
              {hive && hive.watchedRepositories.length > 0 ? (
                <ul className="hive-watched__list">
                  {hive.watchedRepositories.map((repository) => (
                    <li key={repository.path} className="hive-watched__item">
                      <div className="hive-watched__body">
                        <strong title={repository.path}>{shortRepositoryPath(repository.path)}</strong>
                        {repository.currentTask ? <small>{translate(repository.currentTask)}</small> : null}
                      </div>
                      <span className={`hive-watched__status hive-watched__status--${repository.status}`}>{translate(repository.status)}</span>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="hive-empty hive-empty--compact">{translate('Нет отслеживаемых проектов')}</p>
              )}
            </section>
          </section>

          <section className="hive-dock__card hive-dock__card--budget" aria-label={translate('Провайдер и лимиты')}>
            <div className="hive-budget-card">
              <div className="hive-budget-card__head">
                <h3>{translate(budget.dailyCostAvailable ? 'Бюджет и провайдер' : 'Провайдер и лимиты')}</h3>
                <span className="hive-budget-card__provider-tag">{snapshotBudgetLabel}</span>
              </div>
              {hiveLoading ? (
                <div className="hive-budget-skeleton" aria-label={translate('Загрузка бюджета')}>
                  <HiveSkeleton className="hive-skeleton--line" /><HiveSkeleton className="hive-skeleton--bar" /><HiveSkeleton className="hive-skeleton--line hive-skeleton--wide" />
                </div>
              ) : budget.dailyCostAvailable ? (
                <>
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
                  <p className="hive-budget-card__limit">
                    {translate('Лимит:')} ${dailyBudgetUsd.toFixed(2)}{translate('/день')} • {translate('Остаток:')} ${remainingUsd.toFixed(2)}
                  </p>
                </>
              ) : null}
              {isRoutProvider ? (
                <p className="hive-budget-card__requests">
                  {translate('Дневной лимит: 650 000 токенов (сброс в 00:00 UTC)')}
                </p>
              ) : (
                <p className="hive-budget-card__requests">
                  {translate('Лимит запросов провайдера')}: {requestSummary}
                </p>
              )}
              <p className="hive-budget-card__model">
                {budget.model ? `${translate('Текущая модель:')} ${budget.model}` : translate('Модель ещё не известна')}
                {effectiveFallbackModel && !routmyStatusText ? ` · ${translate('Модель резерва:')} ${effectiveFallbackModel}` : ''}
                {!isRoutProvider && !configuredCaps ? ` · ${translate('лимиты запросов не настроены')}` : ''}
                {!budget.historyAvailable ? ` · ${translate('история отправок недоступна')}` : ''}
              </p>
              {routmyStatusText ? <p className="hive-budget-card__routmy">{routmyStatusText}</p> : null}
              {budget.exhausted ? <p className="hive-budget-card__notice">{translate('Лимит запросов исчерпан — Core приостановит отправку до сброса окна.')}</p> : null}
            </div>

            <div className="hive-activity-bars" aria-hidden="true">
              {departments.slice(0, 8).map((department) => {
                const loadPercent = hivePercent(department.activeCount, department.maxCapacity)
                const heightPercent = department.activeCount > 0 ? Math.max(24, loadPercent) : 14
                return (
                  <div
                    key={department.key}
                    className={`hive-activity-bars__col${department.activeCount > 0 ? ' hive-activity-bars__col--active' : ''}`}
                    title={`${translate(department.label)}: ${department.activeCount}`}
                  >
                    <span style={{ height: `${heightPercent}%` }} />
                  </div>
                )
              })}
            </div>
          </section>

          <section
            className={`hive-dock__card hive-dock__card--logs${allLogs ? ' hive-dock__card--logs-expanded' : ''}`}
            aria-label={translate('Логи улья')}
          >
            <div className="hive-panel__head">
              <h3>{translate('Логи улья')}</h3>
              <div className="hive-panel__actions">
                {logs.length === 0 ? <span className="hive-badge">{translate('нет событий')}</span> : null}
                <button
                  type="button"
                  className="hive-link-button"
                  disabled={logs.length === 0}
                  onClick={() => void copyHiveLogs()}
                >
                  {translate(
                    logsCopyState === 'copied'
                      ? 'Скопировано'
                      : logsCopyState === 'failed'
                        ? 'Ошибка копирования'
                        : 'Копировать'
                  )}
                </button>
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
            <ol ref={logListRef} className={`hive-log-list${allLogs ? ' hive-log-list--expanded' : ''}`}>
              {logs.map((entry) => (
                <li key={entry.id} className={`hive-log-row hive-log-row--${entry.level}`}>
                  <span className="hive-log-row__icon" aria-hidden="true"><UiIcon name={HIVE_LOG_ICONS[entry.level]} size={14} /></span>
                  <time className="hive-log-row__time">{entry.at}</time>
                  <span className="hive-log-row__text" title={translate(entry.text)}>{translate(entry.text)}</span>
                </li>
              ))}
            </ol>
            {logs.length === 0 ? <p className="hive-empty hive-empty--compact">{translate('Событий пока нет.')}</p> : null}
          </section>

          <div className="hive-dock__card hive-dock__card--launcher">
            <button
              type="button"
              className={`hive-chat-launcher${hiveChatOpen ? ' hive-chat-launcher--open' : ''}`}
              aria-label={translate('Диалог с Королевой Евой')}
              aria-expanded={hiveChatOpen}
              aria-controls="hive-chat-drawer"
              onClick={() => setHiveChatOpen((open) => !open)}
            >
              <span className="hive-chat-launcher__glow" aria-hidden="true">
                <HiveLotusMark size={38} />
              </span>
              <span className="hive-chat-launcher__body">
                <strong>{translate('Диалог с Королевой Евой')}</strong>
                <small>
                  {translate(
                    queenTyping
                      ? 'Королева печатает'
                      : waitingApproval
                        ? 'Ждёт подтверждения'
                        : 'Открыть диалог'
                  )}
                </small>
              </span>
              <span className="hive-chat-launcher__meta">
                <span className="hive-badge">{translate(queenTyping ? 'в процессе' : 'на связи')}</span>
                {effectiveActiveApprovals.length > 0 ? (
                  <span className="hive-chat-launcher__alert">{effectiveActiveApprovals.length}</span>
                ) : null}
              </span>
            </button>
          </div>
        </div>

        {hiveChatOpen ? (
          <>
            <div
              className="hive-chat-drawer__backdrop"
              aria-hidden="true"
              onClick={() => setHiveChatOpen(false)}
            />
            <aside id="hive-chat-drawer" className="hive-chat-drawer" aria-label={translate('Диалог с Королевой Евой')}>
              <div className="hive-panel__head hive-chat-drawer__head">
                <div className="hive-chat-drawer__title">
                  <HiveLotusMark size={20} />
                  <h2>{translate('Диалог с Королевой Евой')}</h2>
                </div>
                <div className="hive-panel__actions">
                  <button
                    type="button"
                    className="hive-link-button"
                    onClick={() => void openHiveChat()}
                  >
                    {translate('Открыть чат Улья')}
                  </button>
                  <span className="hive-badge">
                    {translate(queenTyping ? 'в процессе' : 'на связи')}
                  </span>
                  <button
                    type="button"
                    className="hive-chat-drawer__close"
                    aria-label={translate('Закрыть диалог')}
                    onClick={() => setHiveChatOpen(false)}
                  >
                    <UiIcon name="x" size={16} />
                  </button>
                </div>
              </div>

              <div className="hive-chat__messages" role="log" aria-live="polite">
                {hiveChatMessages.length === 0 && effectiveActiveApprovals.length === 0 && !queenTyping ? (
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
                {effectiveActiveApprovals.map((approval) => (
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
                    <small className="hive-chat__activity" data-testid="queen-activity"> · {translate(queenActivity)}</small>
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
                  <span className="hive-chat__send-icon" aria-hidden="true">➤</span>{translate('Отправить')}
                </button>
              </form>
            </aside>
          </>
        ) : null}
      </div>

      {hiveSettingsOpen ? (
        <div
          className="settings-modal"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setHiveSettingsOpen(false)
          }}
          onKeyDown={(event) => {
            if (event.key === 'Escape') setHiveSettingsOpen(false)
          }}
        >
          <section className="settings-modal__window hive-settings-modal" role="dialog" aria-modal="true" aria-labelledby="hive-settings-title">
            <header className="settings-modal__header">
              <div>
                <p className="settings-modal__eyebrow">Hive / Eva</p>
                <h2 id="hive-settings-title">{translate('Настройки улья')}</h2>
              </div>
              <div className="hive-console__controls">
                <span className={`hive-badge ${evaWorkspacePath ? 'hive-badge--active' : 'hive-badge--inactive'}`}>
                  {translate(evaWorkspacePath ? 'включено' : 'выключено')}
                </span>
                <button
                  type="button"
                  className="settings-modal__close"
                  aria-label={translate('Закрыть настройки')}
                  onClick={() => setHiveSettingsOpen(false)}
                >
                  ×
                </button>
              </div>
            </header>
            <div className="settings-modal__content hive-settings-modal__body">
              <p className="hive-settings-modal__lead">
                {translate('Укажи отдельный изолированный Git checkout. Ева сможет исследовать и исправлять только его; Core не изменяет установленный продукт.')}
              </p>

              <section className="hive-settings-section">
                <label className="hive-provider" htmlFor="eva-workspace-path-input">
                  <span>{translate('Путь к исходникам Евы (eva_workspace_path)')}</span>
                  <div className="hive-settings-modal__row">
                    <input
                      id="eva-workspace-path-input"
                      type="text"
                      aria-label={translate('Путь к исходникам Евы (eva_workspace_path)')}
                      value={evaWorkspaceDraft}
                      onChange={(changeEvent) => {
                        setEvaWorkspaceDraft(changeEvent.target.value)
                        setEvaWorkspaceValidation('idle')
                        setEvaWorkspaceValidationMessage(null)
                      }}
                      placeholder="C:\\EvoHime-self-development"
                    />
                    <button
                      type="button"
                      disabled={!connected || evaWorkspaceValidation === 'checking' || evaWorkspaceBusy}
                      onClick={() => void validateEvaWorkspacePath()}
                    >
                      {translate(evaWorkspaceValidation === 'checking' ? 'Проверка…' : 'Проверить')}
                    </button>
                    <button
                      type="button"
                      className="hive-settings-btn--primary"
                      disabled={!connected || evaWorkspaceBusy}
                      onClick={() => void saveEvaWorkspacePath()}
                    >
                      {translate(evaWorkspaceBusy ? 'Сохранение…' : 'Сохранить')}
                    </button>
                    {evaWorkspaceValidation === 'valid' ? (
                      <span
                        className="hive-settings-status hive-settings-status--valid"
                        data-testid="eva-workspace-valid"
                        aria-label={translate('Путь проверен: валидный Git-репозиторий')}
                      >
                        ✓
                      </span>
                    ) : null}
                    {evaWorkspaceValidation === 'invalid' ? (
                      <span
                        className="hive-settings-status hive-settings-status--invalid"
                        data-testid="eva-workspace-invalid"
                        aria-label={translate('Путь невалиден')}
                      >
                        ✕
                      </span>
                    ) : null}
                  </div>
                </label>
                <small className="hive-settings-hint">
                  {translate(evaWorkspacePath ? 'Инструменты eva_workspace_* доступны Королеве Еве.' : 'Путь не задан — инструменты саморазвития недоступны.')}
                </small>
                {evaWorkspaceValidationMessage ? (
                  <p role="alert" className="hive-console__error">{translate(evaWorkspaceValidationMessage)}</p>
                ) : null}
                {evaWorkspaceError ? (
                  <p role="alert" className="hive-console__error">{translate(evaWorkspaceError)}</p>
                ) : null}
              </section>

              <section className="hive-settings-section" data-testid="watched-repositories-section">
                <div className="hive-provider">
                  <span>{translate('Проекты Улья (watched_repositories)')}</span>
                  {watchedRepositories.length > 0 ? (
                    <ul className="hive-watched-repos">
                      {watchedRepositories.map((repo) => (
                        <li key={repo} className="hive-watched-repos__item">
                          <span className="hive-watched-repos__path" title={repo}>{repo}</span>
                          <button
                            type="button"
                            className="hive-watched-repos__remove"
                            aria-label={`${translate('Удалить проект')} ${repo}`}
                            disabled={!connected || watchedReposBusy}
                            onClick={() => void removeWatchedRepository(repo)}
                          >
                            {translate('Удалить')}
                          </button>
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <small className="hive-settings-hint">{translate('Список сторонних проектов пуст.')}</small>
                  )}
                  <div className="hive-settings-modal__row">
                    <input
                      id="watched-repo-input"
                      type="text"
                      aria-label={translate('Путь к проекту')}
                      value={newRepoDraft}
                      onChange={(changeEvent) => {
                        setNewRepoDraft(changeEvent.target.value)
                        setWatchedReposError(null)
                      }}
                      placeholder="C:\\Projects\\my-repo"
                    />
                    <button
                      type="button"
                      disabled={!connected || watchedReposBusy}
                      onClick={() => void addWatchedRepository()}
                    >
                      {translate('Добавить проект')}
                    </button>
                  </div>
                  {watchedReposError ? (
                    <p role="alert" className="hive-console__error">{translate(watchedReposError)}</p>
                  ) : null}
                </div>
              </section>

              <section className="hive-settings-section">
                <div className={`hive-settings-modal__grid${hiveProviderProfile.trim() === 'literouter' ? ' hive-settings-modal__grid--with-tier' : ''}`}>
                  <label className="hive-provider" htmlFor="hive-provider-profile-select">
                    <span>{translate('Провайдер Улья')}</span>
                    <select
                      id="hive-provider-profile-select"
                      aria-label={translate('Провайдер Улья')}
                      value={hiveProviderProfile}
                      disabled={!connected || hiveProviderBusy}
                      onChange={(changeEvent) => {
                        setHiveProviderProfile(changeEvent.target.value)
                        setHiveProviderError(null)
                        setHiveProviderSaved(false)
                      }}
                    >
                      <option value="">{translate('По умолчанию (Rout.my)')}</option>
                      <option value="literouter">LiteRouter</option>
                      <option value="rout">Rout.my</option>
                      <option value="deepseek">DeepSeek</option>
                      <option value="openai_compatible">OpenAI-Compatible</option>
                      <option value="openai_responses">OpenAI Responses</option>
                      <option value="ollama">Ollama</option>
                    </select>
                  </label>
                  {hiveProviderProfile.trim() === 'literouter' ? (
                    <label className="hive-provider" htmlFor="hive-tier-select">
                      <span>{translate('Режим моделей Улья')}</span>
                      <select
                        id="hive-tier-select"
                        aria-label={translate('Режим моделей Улья')}
                        value={hiveTier}
                        disabled={!connected || hiveProviderBusy}
                        onChange={(changeEvent) => {
                          const nextTier = changeEvent.target.value as ModelTier
                          setHiveTier(nextTier)
                          if (hiveModelDraft.trim().length > 0) {
                            const isFree = hiveModelDraft.trim().endsWith(':free')
                            if ((nextTier === 'free' && !isFree) || (nextTier === 'paid' && isFree)) {
                              setHiveModelDraft('')
                            }
                          }
                          setHiveProviderError(null)
                          setHiveProviderSaved(false)
                        }}
                      >
                        <option value="free">{translate('Бесплатные (:free)')}</option>
                        <option value="paid">{translate('Платные')}</option>
                      </select>
                    </label>
                  ) : null}
                  <label className="hive-provider" htmlFor="hive-model-input">
                    <span>{translate('Модель Улья')}</span>
                    <div className="hive-settings-modal__row">
                      <select
                        id="hive-model-input"
                        aria-label={translate('Модель Улья')}
                        value={hiveModelDraft}
                        disabled={!connected || hiveProviderBusy}
                        onChange={(changeEvent) => {
                          setHiveModelDraft(changeEvent.target.value)
                          setHiveProviderError(null)
                          setHiveProviderSaved(false)
                        }}
                      >
                        <option value="">{translate('По умолчанию провайдера')}</option>
                        {hiveModelDraft.trim().length > 0 &&
                          !hiveCatalogModels.includes(hiveModelDraft.trim()) &&
                          (hiveProviderProfile.trim() !== 'literouter' ||
                            (hiveTier === 'free' ? hiveModelDraft.trim().endsWith(':free') : !hiveModelDraft.trim().endsWith(':free'))) ? (
                          <option value={hiveModelDraft.trim()}>{hiveModelDraft.trim()}</option>
                        ) : null}
                        {hiveCatalogModels.map((modelId) => (
                          <option key={modelId} value={modelId}>{modelId}</option>
                        ))}
                      </select>
                      <button
                        type="button"
                        disabled={!connected || hiveProviderBusy}
                        onClick={() => void saveHiveProviderSettings()}
                      >
                        {translate(hiveProviderBusy ? 'Сохранение…' : 'Сохранить провайдер Улья')}
                      </button>
                    </div>
                  </label>
                </div>
                {hiveCatalogLoading ? <small role="status" className="hive-settings-hint">{translate('Загрузка каталога моделей…')}</small> : null}
                {hiveCatalogError ? <p role="alert" className="hive-console__error">{translate('Каталог моделей:')}{' '}{translate(hiveCatalogError)}</p> : null}
                {hiveProviderError ? (
                  <p role="alert" className="hive-console__error">{translate(hiveProviderError)}</p>
                ) : hiveProviderSaved ? (
                  <small role="status" className="hive-settings-hint">✓ {translate('Провайдер и модель Улья сохранены')}</small>
                ) : null}
                {providerLoading || (!providerError && providerSummary === null) ? (
                  <div className="hive-provider">
                    <span>{translate('API-ключ Улья (обязателен для работы Улья)')}</span>
                    <small role="status" className="hive-settings-hint">{translate('Загрузка настроек API-ключа…')}</small>
                  </div>
                ) : providerError ? (
                  <div className="hive-provider">
                    <span>{translate('API-ключ Улья (обязателен для работы Улья)')}</span>
                    <div className="hive-settings-modal__row">
                      <p role="alert" className="hive-console__error">{translate(providerError)}</p>
                      <button type="button" disabled={!connected} onClick={() => void refreshProviderSummary()}>
                        {translate('Повторить')}
                      </button>
                    </div>
                  </div>
                ) : hiveApiKeyConfigured && !isEditingMainKey ? (
                  <div className="hive-provider">
                    <span>{translate('API-ключ Улья (обязателен для работы Улья)')}</span>
                    <div className="hive-settings-modal__row">
                      <small role="status" className="hive-settings-hint hive-settings-hint--success">{translate('API-ключ сохранен ✓')}</small>
                      <button type="button" disabled={!connected} onClick={() => setIsEditingMainKey(true)}>{translate('Изменить')}</button>
                    </div>
                  </div>
                ) : (
                  <label className="hive-provider" htmlFor="hive-api-key-input">
                    <span>{translate('API-ключ Улья (обязателен для работы Улья)')}</span>
                    <div className="hive-settings-modal__row">
                      <input
                        id="hive-api-key-input"
                        type="password"
                        aria-label={translate('API-ключ Улья (обязателен для работы Улья)')}
                        value={hiveApiKeyDraft}
                        disabled={!connected || hiveApiKeyBusy}
                        onChange={(changeEvent) => {
                          setHiveApiKeyDraft(changeEvent.target.value)
                          setHiveApiKeyError(null)
                          setHiveApiKeySaved(false)
                        }}
                        placeholder="sk-..."
                      />
                      <button type="button" disabled={!connected || hiveApiKeyBusy} onClick={() => void saveHiveApiKey()}>
                        {translate(hiveApiKeyBusy ? 'Сохранение…' : 'Сохранить ключ')}
                      </button>
                      {isEditingMainKey ? <button type="button" disabled={hiveApiKeyBusy} onClick={() => { setHiveApiKeyDraft(''); setIsEditingMainKey(false) }}>{translate('Отмена')}</button> : null}
                    </div>
                  </label>
                )}
                {hiveApiKeyError ? (
                  <p role="alert" className="hive-console__error">{translate(hiveApiKeyError)}</p>
                ) : hiveApiKeySaved ? (
                  <small role="status" className="hive-settings-hint">✓ {translate('API-ключ Улья сохранён')}</small>
                ) : null}
              </section>

              <section className="hive-settings-section">
                <div className={`hive-settings-modal__grid${hiveFallbackProviderProfile.trim() === 'literouter' ? ' hive-settings-modal__grid--with-tier' : ''}`}>
                  <label className="hive-provider" htmlFor="hive-fallback-provider-profile-select">
                    <span>{translate('Резервный провайдер Улья')}</span>
                    <select
                      id="hive-fallback-provider-profile-select"
                      aria-label={translate('Резервный провайдер Улья')}
                      value={hiveFallbackProviderProfile}
                      disabled={!connected || hiveFallbackProviderBusy}
                      onChange={(changeEvent) => {
                        setHiveFallbackProviderProfile(changeEvent.target.value)
                        setHiveFallbackProviderError(null)
                        setHiveFallbackProviderSaved(false)
                      }}
                    >
                      <option value="">{translate('Отключён')}</option>
                      <option value="rout">Rout.my</option>
                      <option value="literouter">LiteRouter</option>
                      <option value="deepseek">DeepSeek</option>
                      <option value="openai_compatible">OpenAI-Compatible</option>
                      <option value="openai_responses">OpenAI Responses</option>
                      <option value="ollama">Ollama</option>
                    </select>
                  </label>
                  {hiveFallbackProviderProfile.trim() === 'literouter' ? (
                    <label className="hive-provider" htmlFor="hive-fallback-tier-select">
                      <span>{translate('Режим резервных моделей')}</span>
                      <select
                        id="hive-fallback-tier-select"
                        aria-label={translate('Режим резервных моделей')}
                        value={hiveFallbackTier}
                        disabled={!connected || hiveFallbackProviderBusy}
                        onChange={(changeEvent) => {
                          const nextTier = changeEvent.target.value as ModelTier
                          setHiveFallbackTier(nextTier)
                          if (hiveFallbackModelDraft.trim().length > 0) {
                            const isFree = hiveFallbackModelDraft.trim().endsWith(':free')
                            if ((nextTier === 'free' && !isFree) || (nextTier === 'paid' && isFree)) {
                              setHiveFallbackModelDraft('')
                            }
                          }
                          setHiveFallbackProviderError(null)
                          setHiveFallbackProviderSaved(false)
                        }}
                      >
                        <option value="free">{translate('Бесплатные (:free)')}</option>
                        <option value="paid">{translate('Платные')}</option>
                      </select>
                    </label>
                  ) : null}
                  <label className="hive-provider" htmlFor="hive-fallback-model-select">
                    <span>{translate('Резервная модель Улья')}</span>
                    <div className="hive-settings-modal__row">
                      <select
                        id="hive-fallback-model-select"
                        aria-label={translate('Резервная модель Улья')}
                        value={hiveFallbackModelDraft}
                        disabled={!connected || hiveFallbackProviderBusy || hiveFallbackProviderProfile.trim().length === 0}
                        onChange={(changeEvent) => {
                          setHiveFallbackModelDraft(changeEvent.target.value)
                          setHiveFallbackProviderError(null)
                          setHiveFallbackProviderSaved(false)
                        }}
                      >
                        <option value="">{translate('По умолчанию провайдера')}</option>
                        {hiveFallbackModelDraft.trim().length > 0 &&
                          !hiveFallbackCatalogModels.includes(hiveFallbackModelDraft.trim()) &&
                          (hiveFallbackProviderProfile.trim() !== 'literouter' ||
                            (hiveFallbackTier === 'free' ? hiveFallbackModelDraft.trim().endsWith(':free') : !hiveFallbackModelDraft.trim().endsWith(':free'))) ? (
                          <option value={hiveFallbackModelDraft.trim()}>{hiveFallbackModelDraft.trim()}</option>
                        ) : null}
                        {hiveFallbackCatalogModels.map((modelId) => (
                          <option key={modelId} value={modelId}>{modelId}</option>
                        ))}
                      </select>
                      <button
                        type="button"
                        disabled={!connected || hiveFallbackProviderBusy}
                        onClick={() => void saveHiveFallbackProviderSettings()}
                      >
                        {translate(hiveFallbackProviderBusy ? 'Сохранение…' : 'Сохранить резервный провайдер')}
                      </button>
                    </div>
                  </label>
                </div>
                {hiveFallbackCatalogLoading ? <small role="status" className="hive-settings-hint">{translate('Загрузка каталога моделей…')}</small> : null}
                {hiveFallbackCatalogError ? <p role="alert" className="hive-console__error">{translate('Каталог резервных моделей:')}{' '}{translate(hiveFallbackCatalogError)}</p> : null}
                {hiveFallbackProviderError ? (
                  <p role="alert" className="hive-console__error">{translate(hiveFallbackProviderError)}</p>
                ) : hiveFallbackProviderSaved ? (
                  <small role="status" className="hive-settings-hint">✓ {translate('Резервный провайдер и модель Улья сохранены')}</small>
                ) : null}
                {providerSummary?.hiveFallbackApiKeyConfigured && !isEditingFallbackKey ? (
                  <div className="hive-provider">
                    <span>{translate('Резервный API-ключ Улья')}</span>
                    <div className="hive-settings-modal__row">
                      <small role="status" className="hive-settings-hint hive-settings-hint--success">{translate('Резервный API-ключ сохранен ✓')}</small>
                      <button type="button" disabled={!connected} onClick={() => setIsEditingFallbackKey(true)}>{translate('Изменить')}</button>
                    </div>
                  </div>
                ) : (
                  <label className="hive-provider" htmlFor="hive-fallback-api-key-input">
                    <span>{translate('Резервный API-ключ Улья')}</span>
                    <div className="hive-settings-modal__row">
                      <input
                        id="hive-fallback-api-key-input"
                        type="password"
                        aria-label={translate('Резервный API-ключ Улья')}
                        value={hiveFallbackApiKeyDraft}
                        disabled={!connected || hiveFallbackApiKeyBusy}
                        onChange={(changeEvent) => {
                          setHiveFallbackApiKeyDraft(changeEvent.target.value)
                          setHiveFallbackApiKeyError(null)
                          setHiveFallbackApiKeySaved(false)
                        }}
                        placeholder="sk-..."
                      />
                      <button type="button" disabled={!connected || hiveFallbackApiKeyBusy} onClick={() => void saveHiveFallbackApiKey()}>
                        {translate(hiveFallbackApiKeyBusy ? 'Сохранение…' : 'Сохранить резервный ключ')}
                      </button>
                      {isEditingFallbackKey ? <button type="button" disabled={hiveFallbackApiKeyBusy} onClick={() => { setHiveFallbackApiKeyDraft(''); setIsEditingFallbackKey(false) }}>{translate('Отмена')}</button> : null}
                    </div>
                  </label>
                )}
                {hiveFallbackApiKeyError ? (
                  <p role="alert" className="hive-console__error">{translate(hiveFallbackApiKeyError)}</p>
                ) : hiveFallbackApiKeySaved ? (
                  <small role="status" className="hive-settings-hint">✓ {translate('Резервный API-ключ Улья сохранён')}</small>
                ) : null}
              </section>

              <section className="hive-settings-section">
                <h3>{translate('Лимиты запросов и токенов Улья')}</h3>
                <small className="hive-settings-hint">{translate('Настройки действуют только на изолированные основной и резервный маршруты Улья. Ноль означает без ограничения.')}</small>
                <HiveRequestLimitsEditor
                  title="Лимиты основного маршрута Улья"
                  prefix="hive-primary"
                  limits={hivePrimaryLimitsDraft}
                  quota={hivePrimaryQuotaDraft}
                  resetAtUtcMidnight={hivePrimaryQuotaResetDraft}
                  disabled={!connected || hiveLimitsBusy}
                  onLimitsChange={setHivePrimaryLimitsDraft}
                  onQuotaChange={setHivePrimaryQuotaDraft}
                  onResetChange={setHivePrimaryQuotaResetDraft}
                />
                <HiveRequestLimitsEditor
                  title="Лимиты резервного маршрута Улья"
                  prefix="hive-fallback"
                  limits={hiveFallbackLimitsDraft}
                  quota={hiveFallbackQuotaDraft}
                  resetAtUtcMidnight={hiveFallbackQuotaResetDraft}
                  disabled={!connected || hiveLimitsBusy}
                  onLimitsChange={setHiveFallbackLimitsDraft}
                  onQuotaChange={setHiveFallbackQuotaDraft}
                  onResetChange={setHiveFallbackQuotaResetDraft}
                />
                <button type="button" disabled={!connected || hiveLimitsBusy} onClick={() => void saveHiveRequestLimits()}>
                  {translate(hiveLimitsBusy ? 'Сохранение…' : 'Сохранить лимиты Улья')}
                </button>
                {hiveLimitsError ? <p role="alert" className="hive-console__error">{translate(hiveLimitsError)}</p> : null}
                {hiveLimitsSaved ? <small role="status" className="hive-settings-hint">✓ {translate('Лимиты Улья сохранены')}</small> : null}
              </section>

              <section className="hive-settings-section">
                <div className="hive-settings-modal__grid hive-settings-modal__grid--balanced">
                  <label className="hive-provider" htmlFor="daily-budget-usd-input">
                    <span>{translate('Дневной лимит бюджета ($)')}</span>
                    <div className="hive-settings-modal__row">
                      <input
                        id="daily-budget-usd-input"
                        type="number"
                        min="0"
                        max="10000"
                        step="0.01"
                        aria-label={translate('Дневной лимит бюджета ($)')}
                        value={dailyBudgetDraft}
                        onChange={(changeEvent) => {
                          setDailyBudgetDraft(changeEvent.target.value)
                          setDailyBudgetError(null)
                          setDailyBudgetSaved(false)
                        }}
                      />
                      <button
                        type="button"
                        disabled={!connected || dailyBudgetBusy}
                        onClick={() => void saveDailyBudget()}
                      >
                        {translate(dailyBudgetBusy ? 'Сохранение…' : 'Сохранить лимит')}
                      </button>
                    </div>
                  </label>
                  <label className="hive-settings-toggle" htmlFor="proactive-evolution-toggle">
                    <span>{translate('Проактивное саморазвитие')}</span>
                    <input
                      id="proactive-evolution-toggle"
                      type="checkbox"
                      role="switch"
                      aria-label={translate('Проактивное саморазвитие')}
                      checked={proactiveEvolutionEnabled}
                      disabled={!connected || proactiveEvolutionBusy}
                      onChange={(changeEvent) => void toggleProactiveEvolution(changeEvent.target.checked)}
                    />
                  </label>
                </div>
                {dailyBudgetError ? (
                  <p role="alert" className="hive-console__error">{translate(dailyBudgetError)}</p>
                ) : dailyBudgetSaved ? (
                  <small role="status" className="hive-settings-hint">✓ {translate('Дневной лимит сохранён')}</small>
                ) : !budget.dailyCostAvailable ? (
                  <small className="hive-settings-hint">
                    {translate('Текущий провайдер не использует денежный биллинг — ограничение идёт по лимитам запросов и токенов, значение 0 не блокирует работу.')}
                  </small>
                ) : null}
                {proactiveEvolutionError ? (
                  <p role="alert" className="hive-console__error">{translate(proactiveEvolutionError)}</p>
                ) : null}
              </section>
            </div>
          </section>
        </div>
      ) : null}
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
            <span className="operations-card__eyebrow">Autonomous Cycle</span>
            <span className="operations-card__phase">{translate(!projectionReady ? 'syncing' : pulseFailed ? 'warning' : 'healthy')}</span>
          </div>
          <h3>{translate("Наблюдение Улья")}</h3>
          <div className="operations-card__metric">
            <strong>{translate(!projectionReady ? (CONNECTED_STATES.includes(connection) ? translate("Синхронизация") : translate("Недоступно")) : pulseFailed ? translate("Внимание") : 'OK')}</strong>
            <span>
              {translate(!projectionReady
                ? translate("состояние цикла не подтверждено")
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
      <AnalysisKernelPanel connection={connection} events={events} />
    </section>
  )
}
