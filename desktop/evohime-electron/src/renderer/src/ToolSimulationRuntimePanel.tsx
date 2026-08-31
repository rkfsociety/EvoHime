import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

type Projection = {
  status?: string
  mode?: string
  state?: string
  provenance?: string
  projection_json?: { contract_id?: string; contract_version?: number; ephemeral?: boolean; fixture_count?: number; completed_count?: number; real_fallback?: boolean; raw_payload?: boolean }
  error_code?: string
}

/** Metadata-only Core projection. Synthetic state is always visible. */
export function ToolSimulationRuntimePanel(): React.JSX.Element {
  const api = useShellApi(); const [projection, setProjection] = useState<Projection | null>(null)
  useEffect(() => {
    if (!api) return
    const unsubscribe = api.subscribe((event) => {
      if (event.kind !== 'core-event' || event.event.eventType !== 'tool_simulation_runtime.result') return
      try { setProjection(JSON.parse(event.event.payload) as Projection) } catch { setProjection({ status: 'invalid_projection' }) }
    })
    void api.invoke('toolSimulationRuntime.status', { requestId: crypto.randomUUID(), ownerScope: 'tool-simulation-runtime', idempotencyKey: crypto.randomUUID() })
    return unsubscribe
  }, [api])
  const meta = projection?.projection_json
  return <section className="panel" aria-label="Tool Simulation Runtime">
    <h2>Симуляция инструментов</h2>
    <p role="status">Режим: {projection?.mode ?? 'ожидание Core'} · состояние: {projection?.state ?? 'нет данных'}</p>
    <p>Provenance: {projection?.provenance ?? 'synthetic_or_fixture'} · это не подтверждение реального эффекта.</p>
    {projection?.status === 'ok' && <p>Контракт {meta?.contract_id} v{meta?.contract_version}; fixtures: {meta?.fixture_count ?? 0}; завершено: {meta?.completed_count ?? 0}.</p>}
    {projection && projection.status !== 'ok' && <p>Состояние: {projection.status} ({projection.error_code || 'unknown'})</p>}
    <p>Ephemeral: {meta?.ephemeral === false ? 'нет' : 'да'} · real fallback: запрещён · raw payload: не передаётся.</p>
  </section>
}
