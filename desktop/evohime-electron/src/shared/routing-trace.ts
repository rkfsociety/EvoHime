/** Versioned, Core-owned routing trace projection consumed by the renderer. */
export const ROUTING_SCHEMA_MAJOR = 1 as const
export const ROUTES = ['local', 'cloud'] as const
export const MAX_TRACE_METADATA_BYTES = 128
export const MAX_TRACE_CANDIDATES = 64
export type RouteId = (typeof ROUTES)[number]
export type TerminalStatus = 'success' | 'cancelled' | 'no_routes_configured' | 'both_routes_unavailable' | 'classification_incomplete' | 'context_limit_exceeded' | 'policy_violation' | 'budget_unavailable' | 'context_assembly_failed' | 'fallback_limit_reached' | 'run_deadline_exceeded' | 'reroute_approval_declined' | 'internal_error'
export type PrivacyLabel = 'sensitive' | 'non_sensitive' | 'unknown'
export type HealthState = 'healthy' | 'degraded' | 'unavailable'

const REFUSALS = new Set<TerminalStatus>([
  'no_routes_configured', 'both_routes_unavailable', 'classification_incomplete', 'context_limit_exceeded',
  'policy_violation', 'budget_unavailable', 'context_assembly_failed', 'fallback_limit_reached',
  'run_deadline_exceeded', 'reroute_approval_declined', 'internal_error'
])
const SAFE_ACTION: Record<string, string> = {
  retry_later: 'Повторить позже', clarify_request: 'Уточнить задачу', contact_support: 'Обратиться в поддержку', manual_review: 'Нужна ручная проверка'
}
const SAFE_ACTION_KEYS = new Set(Object.keys(SAFE_ACTION))
const STATUS_TEXT: Record<string, string> = {
  success: 'Ответ готов', cancelled: 'Задача отменена', no_routes_configured: 'Маршруты не настроены',
  both_routes_unavailable: 'Доступные маршруты не отвечают', classification_incomplete: 'Не удалось завершить классификацию',
  context_limit_exceeded: 'Задача не помещается в контекст', policy_violation: 'Правила безопасности не разрешают этот маршрут',
  budget_unavailable: 'Бюджет запуска недоступен', context_assembly_failed: 'Не удалось собрать контекст',
  fallback_limit_reached: 'Лимит резервных попыток исчерпан', run_deadline_exceeded: 'Время выполнения истекло',
  reroute_approval_declined: 'Перенаправление не подтверждено', internal_error: 'Внутренняя ошибка Core'
}

export interface RoutingCandidate { readonly route_id: string; readonly health_state: HealthState; readonly reject_reason?: string }
export interface RoutingTrace {
  readonly schema_version: number | string
  readonly terminal_status: TerminalStatus
  readonly selected_route: RouteId | null
  readonly reason_code: string
  readonly safe_next_action?: string | null
  readonly candidates: readonly RoutingCandidate[]
  readonly fallback_count: number
  readonly privacy_label: PrivacyLabel
  readonly trace_id: string
  readonly run_id: string
  readonly sequence: number
}
export type RoutingViewState = 'normal' | 'partial_fallback' | 'degraded' | 'refusal' | 'cancelled' | 'unknown_state' | 'core_unavailable'

export interface PendingRoutingApproval {
  readonly traceId: string
  readonly runId: string
  readonly routeId: RouteId
  readonly expiresAtMs: number
}

function isSafeTraceToken(value: string): boolean {
  return value.length > 0 && value.length <= MAX_TRACE_METADATA_BYTES && /^[A-Za-z0-9_.:-]+$/.test(value)
}

function safeSequence(value: unknown): number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : 0
}

function safeSchemaVersion(value: unknown): number | string {
  if (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0) return value
  if (typeof value === 'string' && value.length <= 32 && /^\d+(?:\.\d+)*$/.test(value)) return value
  return ROUTING_SCHEMA_MAJOR
}

function internalErrorProjection(value: Record<string, unknown>): RoutingTrace {
  return {
    schema_version: safeSchemaVersion(value.schema_version),
    terminal_status: 'internal_error',
    selected_route: null,
    candidates: [],
    reason_code: 'unsupported_enum',
    fallback_count: 0,
    privacy_label: 'unknown',
    trace_id: typeof value.trace_id === 'string' && isSafeTraceToken(value.trace_id) ? value.trace_id : 'redacted',
    run_id: typeof value.run_id === 'string' && isSafeTraceToken(value.run_id) ? value.run_id : 'redacted',
    sequence: safeSequence(value.sequence)
  }
}

export function parseRoutingTrace(raw: string): RoutingTrace | null {
  let value: unknown
  try { value = JSON.parse(raw) } catch { return null }
  if (!value || typeof value !== 'object') return null
  const outer = value as Record<string, unknown>
  const v = outer.trace && typeof outer.trace === 'object' ? outer.trace as Record<string, unknown> : outer
  const version = v.schema_version
  const major = typeof version === 'number' ? version : typeof version === 'string' ? Number(version.split('.')[0]) : NaN
  if (major !== ROUTING_SCHEMA_MAJOR || typeof v.terminal_status !== 'string' || typeof v.selected_route === 'undefined' || !Array.isArray(v.candidates) || typeof v.fallback_count !== 'number' || typeof v.privacy_label !== 'string' || typeof v.trace_id !== 'string' || typeof v.run_id !== 'string' || typeof v.sequence !== 'number' || typeof v.reason_code !== 'string') return null
  if (!isSafeTraceToken(v.trace_id) || !isSafeTraceToken(v.run_id) || !isSafeTraceToken(v.reason_code) || !Number.isSafeInteger(v.sequence) || v.sequence < 0 || !Number.isSafeInteger(v.fallback_count) || v.fallback_count < 0 || v.fallback_count > MAX_TRACE_CANDIDATES || v.candidates.length > MAX_TRACE_CANDIDATES) return null
  if (!(v.terminal_status in STATUS_TEXT) || !['sensitive', 'non_sensitive', 'unknown'].includes(v.privacy_label)) return internalErrorProjection(v)
  if (v.terminal_status === 'success' && typeof v.selected_route !== 'string') return null
  if (v.terminal_status !== 'success' && v.selected_route !== null) return null
  if (typeof v.selected_route === 'string' && !ROUTES.includes(v.selected_route as RouteId)) return internalErrorProjection(v)
  const safeAction = v.safe_next_action === null || typeof v.safe_next_action === 'undefined'
    ? null
    : typeof v.safe_next_action === 'string' && SAFE_ACTION_KEYS.has(v.safe_next_action)
      ? v.safe_next_action
      : null
  if (v.safe_next_action !== null && typeof v.safe_next_action !== 'undefined' && safeAction === null) return null
  const candidates = v.candidates.filter((candidate): candidate is RoutingCandidate => {
    if (!candidate || typeof candidate !== 'object') return false
    const c = candidate as Record<string, unknown>
    return typeof c.route_id === 'string' && isSafeTraceToken(c.route_id) && ['healthy', 'degraded', 'unavailable'].includes(String(c.health_state)) && (typeof c.reject_reason === 'undefined' || c.reject_reason === null || (typeof c.reject_reason === 'string' && isSafeTraceToken(c.reject_reason)))
  })
  if (candidates.length !== v.candidates.length) return null
  if (['both_routes_unavailable', 'context_limit_exceeded', 'context_assembly_failed'].includes(v.terminal_status as string) && candidates.length === 0) return null
  return {
    schema_version: safeSchemaVersion(version),
    terminal_status: v.terminal_status as TerminalStatus,
    selected_route: v.selected_route as RouteId | null,
    reason_code: v.reason_code,
    safe_next_action: safeAction,
    candidates: candidates.map((candidate) => {
      const value = candidate as unknown as Record<string, unknown>
      return {
        route_id: value.route_id as string,
        health_state: value.health_state as HealthState,
        ...(typeof value.reject_reason === 'string' ? { reject_reason: value.reject_reason } : {})
      }
    }),
    fallback_count: v.fallback_count,
    privacy_label: v.privacy_label as PrivacyLabel,
    trace_id: v.trace_id,
    run_id: v.run_id,
    sequence: v.sequence
  }
}

export function parsePendingRoutingApproval(raw: string): PendingRoutingApproval | null {
  let value: unknown
  try { value = JSON.parse(raw) } catch { return null }
  if (!value || typeof value !== 'object') return null
  const v = value as Record<string, unknown>
  const traceId = typeof v.trace_id === 'string' ? v.trace_id : typeof v.traceId === 'string' ? v.traceId : null
  const runId = typeof v.run_id === 'string' ? v.run_id : typeof v.runId === 'string' ? v.runId : null
  const routeId = typeof v.route_id === 'string' ? v.route_id : typeof v.routeId === 'string' ? v.routeId : null
  const expiresAtMs = typeof v.expires_at_ms === 'number' ? v.expires_at_ms : typeof v.expiresAtMs === 'number' ? v.expiresAtMs : null
  if (!traceId || !runId || !isSafeTraceToken(traceId) || !isSafeTraceToken(runId) || (routeId !== 'local' && routeId !== 'cloud') || expiresAtMs === null || !Number.isSafeInteger(expiresAtMs) || expiresAtMs < 0) return null
  return { traceId, runId, routeId, expiresAtMs }
}

export function routingViewState(trace: RoutingTrace, preferred: RouteId | null): RoutingViewState {
  if (trace.reason_code === 'unsupported_enum') return 'unknown_state'
  if (trace.terminal_status === 'cancelled') return 'cancelled'
  if (REFUSALS.has(trace.terminal_status)) return 'refusal'
  if (trace.terminal_status !== 'success') return 'unknown_state'
  if (preferred && trace.selected_route === 'local' && preferred !== trace.selected_route && trace.privacy_label === 'non_sensitive') return 'degraded'
  if (preferred && trace.selected_route !== preferred) return 'partial_fallback'
  return 'normal'
}

export function routingText(trace: RoutingTrace): string {
  return STATUS_TEXT[trace.terminal_status] ?? 'Состояние маршрута неизвестно'
}
export function safeActionText(action: string | null | undefined): string | null { return action ? SAFE_ACTION[action] ?? 'Обратиться в поддержку' : null }
export function isRefusal(status: string): boolean { return REFUSALS.has(status as TerminalStatus) }

// Keep the closed schema lists executable so a future Core enum cannot silently
// become a user-visible internal string.
export const ROUTING_TERMINAL_STATUSES: readonly TerminalStatus[] = [
  'success', 'cancelled', 'no_routes_configured', 'both_routes_unavailable', 'classification_incomplete',
  'context_limit_exceeded', 'policy_violation', 'budget_unavailable', 'context_assembly_failed',
  'fallback_limit_reached', 'run_deadline_exceeded', 'reroute_approval_declined', 'internal_error'
]

if (Object.keys(STATUS_TEXT).sort().join('|') !== [...ROUTING_TERMINAL_STATUSES].sort().join('|')) {
  throw new Error('routing localization table is incomplete')
}
