import { useMemo, useState } from 'react'
import type { CoreEvent } from '@shared/api'
import { parseRoutingTrace, routingText, routingViewState, safeActionText, type RouteId } from '@shared/routing-trace'

export function RoutingStatus({ events }: { readonly events: readonly CoreEvent[] }): React.JSX.Element | null {
  const [preferred, setPreferred] = useState<RouteId | null>(() => {
    const value = window.localStorage.getItem('evohime.preferred-route')
    return value === 'local' || value === 'cloud' ? value : null
  })
  const trace = useMemo(() => {
    const event = events.find((item) => item.eventType === 'routing.trace' || item.eventType === 'routing.terminal')
    return event ? parseRoutingTrace(event.payload) : null
  }, [events])
  if (!trace) return null
  const state = routingViewState(trace, preferred)
  const role = state === 'normal' || state === 'degraded' || state === 'partial_fallback' ? 'status' : 'alert'
  return <div className={`routing-status routing-status--${state}`} role={role} aria-live="polite">
    <label>Предпочтение <select aria-label="Предпочтительный маршрут" value={preferred ?? ''} onChange={(event) => { const next = event.target.value === 'local' || event.target.value === 'cloud' ? event.target.value : null; setPreferred(next); if (next) window.localStorage.setItem('evohime.preferred-route', next); else window.localStorage.removeItem('evohime.preferred-route') }}><option value="">Авто</option><option value="local">Локальный</option><option value="cloud">Облачный</option></select></label>
    <span>{routingText(trace)}</span>
    {trace.selected_route ? <span className="routing-status__route">Маршрут: {trace.selected_route}</span> : null}
    {state === 'degraded' ? <span>⚠ Резервный локальный режим</span> : null}
    {state === 'partial_fallback' && trace.fallback_count > 0 ? <span>Использован резервный маршрут</span> : null}
    {safeActionText(trace.safe_next_action) ? <span>{safeActionText(trace.safe_next_action)}</span> : null}
  </div>
}
