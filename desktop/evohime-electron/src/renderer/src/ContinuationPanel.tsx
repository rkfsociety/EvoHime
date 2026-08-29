import { useMemo } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'
import { useShellApi } from './shell-api'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

/** Bounded projection of Core-owned continuation runs. */
export function ContinuationPanel({ connection, events }: Props): React.JSX.Element {
  const api = useShellApi()
  const runs = useMemo(() => events
    .filter((event) => event.continuation || event.eventType === 'continuation.run')
    .slice(-32)
    .map((event) => {
      if (event.continuation) return event.continuation
      try { return JSON.parse(event.payload) as Record<string, unknown> } catch { return null }
    })
    .filter((run): run is Record<string, unknown> | NonNullable<CoreEvent['continuation']> =>
      run !== null && (('run_id' in run && typeof run.run_id === 'string') || ('runId' in run && typeof run.runId === 'string'))), [events])

  return <section className="panel continuation-panel">
    <div className="panel__header">
      <div><span className="panel__eyebrow">Core Continuation Policy v1</span><h3>Автономные продолжения</h3></div>
      <span className={`status-pill status-pill--${connection}`}>{connection}</span>
    </div>
    <p className="panel__muted">Решение, лимиты и остановка принадлежат Core. Renderer только показывает projection.</p>
    {runs.length === 0 ? <div className="empty-state">Нет активных или недавно просмотренных запусков.</div> : <div className="stack-list">
      {runs.map((run) => {
        const runId = String('runId' in run ? run.runId : run.run_id)
        const state = String(run.state ?? ('decision' in run ? run.decision : 'unknown'))
        const gates = 'gates' in run && Array.isArray(run.gates) ? run.gates as readonly { gateId?: string; status?: string }[] : []
        const stopReason = 'stopReason' in run && typeof run.stopReason === 'string' ? run.stopReason : ''
        const stopped = ['stopped', 'completed', 'blocked', 'failed', 'budget_limited'].includes(state)
        return <article className="stack-list__item" key={runId}>
          <div><strong>{runId}</strong><span className="panel__muted"> · {state}</span></div>
          <div className="panel__muted">Продолжений: {String('continuationIndex' in run ? run.continuationIndex : run.continuation_index ?? '—')} · turns: {String('modelTurns' in run ? run.modelTurns : run.used_model_turns ?? '—')}</div>
          {gates.length > 0 ? <div className="panel__muted">Проверки: {gates.map((gate) => `${gate.gateId ?? ''}: ${gate.status ?? ''}`).join(', ')}</div> : null}
          {stopReason ? <div className="panel__muted">Причина: {stopReason}</div> : null}
          {!stopped && api ? <button type="button" onClick={() => { void api.invoke('core.stopContinuation', { runId, expectedState: 'running', idempotencyKey: `ui-stop-${runId}-${Date.now()}` }) }}>Остановить</button> : null}
          {'error_code' in run && typeof run.error_code === 'string' && run.error_code ? <div className="error-text">{run.error_code}</div> : null}
        </article>
      })}
    </div>}
  </section>
}
