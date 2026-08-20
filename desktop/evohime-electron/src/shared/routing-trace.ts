/** Versioned, Core-owned routing trace projection consumed by the renderer. */
export const ROUTING_SCHEMA_MAJOR = 1 as const
export const ROUTES = ['local', 'cloud'] as const
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

export function parseRoutingTrace(raw: string): RoutingTrace | null {
  let value: unknown
  try { value = JSON.parse(raw) } catch { return null }
  if (!value || typeof value !== 'object') return null
  const v = value as Record<string, unknown>
  const version = v.schema_version
  const major = typeof version === 'number' ? version : typeof version === 'string' ? Number(version.split('.')[0]) : NaN
  if (major !== ROUTING_SCHEMA_MAJOR || typeof v.terminal_status !== 'string' || typeof v.selected_route === undefined || !Array.isArray(v.candidates) || typeof v.fallback_count !== 'number' || typeof v.privacy_label !== 'string' || typeof v.trace_id !== 'string' || typeof v.run_id !== 'string' || typeof v.sequence !== 'number') return null
  if (!(v.terminal_status in STATUS_TEXT) || !['sensitive', 'non_sensitive', 'unknown'].includes(v.privacy_label)) return { schema_version: version as number | string, terminal_status: 'internal_error', selected_route: null, candidates: [], reason_code: 'unsupported_enum', fallback_count: 0, privacy_label: 'unknown', trace_id: String(v.trace_id), run_id: String(v.run_id), sequence: Number(v.sequence) } as RoutingTrace
  if (v.terminal_status === 'success' && typeof v.selected_route !== 'string') return null
  if (v.terminal_status !== 'success' && v.selected_route !== null) return null
  const candidates = v.candidates.filter((candidate): candidate is RoutingCandidate => {
    if (!candidate || typeof candidate !== 'object') return false
    const c = candidate as Record<string, unknown>
    return typeof c.route_id === 'string' && ['healthy', 'degraded', 'unavailable'].includes(String(c.health_state))
  })
  if (candidates.length !== v.candidates.length) return null
  return v as unknown as RoutingTrace
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
