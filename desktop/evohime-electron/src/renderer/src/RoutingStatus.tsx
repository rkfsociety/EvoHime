import { useEffect, useMemo, useState } from 'react'
import type { ConnectionState, CoreEvent } from '@shared/api'
import { parsePendingRoutingApproval, parseRoutingTrace, routingText, routingViewState, safeActionText, type RouteId } from '@shared/routing-trace'
import { useShellApi } from './shell-api'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

export function RoutingStatus({ events, connection }: { readonly events: readonly CoreEvent[]; readonly connection: ConnectionState }): React.JSX.Element | null {
  const api = useShellApi()
  const [preferred, setPreferred] = useState<RouteId | null>(() => {
    const value = window.localStorage.getItem('evohime.preferred-route')
    return value === 'local' || value === 'cloud' ? value : null
  })
  const trace = useMemo(() => {
    const event = events.find((item) => item.eventType === 'routing.trace' || item.eventType === 'routing.terminal')
    return event ? parseRoutingTrace(event.payload) : null
  }, [events])
  const pending = useMemo(() => {
    const event = events.find((item) => item.eventType === 'routing.pending_approval')
    return event ? parsePendingRoutingApproval(event.payload) : null
  }, [events])
  const [nowMs, setNowMs] = useState(() => Date.now())
  const [dismissedTrace, setDismissedTrace] = useState<string | null>(null)
  const [retryStatus, setRetryStatus] = useState<string | null>(null)
  useEffect(() => {
    if (!pending) return
    const timer = window.setInterval(() => setNowMs(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [pending])
  if (!trace && !pending && !CONNECTED_STATES.includes(connection)) {
    const retry = async (): Promise<void> => {
      if (!api) { setRetryStatus('Мост оболочки недоступен.'); return }
      const outcome = await api.invoke('shell.requestResync', {})
      setRetryStatus(outcome.ok && outcome.value.accepted ? 'Повторная синхронизация запрошена.' : 'Core пока недоступен; повтори позже.')
    }
    return <div className="routing-status routing-status--core_unavailable" role="alert" aria-live="assertive">
      <span>Связь с Core потеряна: состояние маршрутизации недоступно.</span>
      {retryStatus ? <span role="status">{retryStatus}</span> : null}
      <button type="button" onClick={() => void retry()}>Повторить подключение</button>
    </div>
  }
  if (!trace && !pending) return null
  const resolve = async (approve: boolean) => {
    if (!api || !pending || !CONNECTED_STATES.includes(connection)) return
    await api.invoke('core.resolveRoutingDecision', { traceId: pending.traceId, approve })
  }
  if (pending) {
    const remaining = Math.max(0, pending.expiresAtMs - nowMs)
    return <div className="routing-status routing-status--degraded" role="alert" aria-live="polite">
      <span>Core просит подтвердить перенаправление на маршрут: {pending.routeId}.</span>
      <span>Осталось: {Math.ceil(remaining / 1000)} с</span>
      <button type="button" disabled={!api || !CONNECTED_STATES.includes(connection)} onClick={() => void resolve(true)}>Подтвердить</button>
      <button type="button" disabled={!api || !CONNECTED_STATES.includes(connection)} onClick={() => void resolve(false)}>Отклонить</button>
    </div>
  }
  if (!trace) return null
  if (trace.trace_id === dismissedTrace) return null
  const state = routingViewState(trace, preferred)
  const role = state === 'normal' || state === 'degraded' || state === 'partial_fallback' ? 'status' : 'alert'
  return <div className={`routing-status routing-status--${state}`} role={role} aria-live="polite">
    <label>Предпочтение <select aria-label="Предпочтительный маршрут" value={preferred ?? ''} onChange={(event) => { const next = event.target.value === 'local' || event.target.value === 'cloud' ? event.target.value : null; setPreferred(next); if (next) window.localStorage.setItem('evohime.preferred-route', next); else window.localStorage.removeItem('evohime.preferred-route') }}><option value="">Авто</option><option value="local">Локальный</option><option value="cloud">Облачный</option></select></label>
    <span>{routingText(trace)}</span>
    {trace.selected_route ? <span className="routing-status__route">Маршрут: {trace.selected_route}</span> : null}
    {state === 'degraded' ? <span>⚠ Резервный локальный режим</span> : null}
    {state === 'partial_fallback' && trace.fallback_count > 0 ? <span>Использован резервный маршрут</span> : null}
    {trace.candidates.filter((candidate) => candidate.reject_reason).map((candidate) => <span key={candidate.route_id}>Маршрут {candidate.route_id}: {candidate.reject_reason}</span>)}
    {safeActionText(trace.safe_next_action) ? <span>{safeActionText(trace.safe_next_action)}</span> : null}
    {state === 'degraded' ? <button type="button" onClick={() => setDismissedTrace(trace.trace_id)}>Скрыть предупреждение</button> : null}
  </div>
}
