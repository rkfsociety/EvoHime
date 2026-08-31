import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

type Projection = { status?: string; state?: string; protocol?: string; control_level?: string; projection_json?: { contract_id?: string; contract_version?: number; preset_count?: number; active_runs?: number; core_control_level?: string; raw_payload?: boolean; credentials?: string }; error_code?: string }

export function ExternalCodingAgentAdapterPanel(): React.JSX.Element {
  const api = useShellApi(); const [projection, setProjection] = useState<Projection | null>(null)
  useEffect(() => { if (!api) return; const unsubscribe = api.subscribe((event) => { if (event.kind !== 'core-event' || event.event.eventType !== 'external_coding_agent_adapter.result') return; try { setProjection(JSON.parse(event.event.payload) as Projection) } catch { setProjection({ status: 'invalid_projection' }) } }); void api.invoke('externalCodingAgentAdapter.status', { requestId: crypto.randomUUID(), ownerScope: 'external-coding-agent-adapter', idempotencyKey: crypto.randomUUID() }); return unsubscribe }, [api])
  const meta = projection?.projection_json
  return <section className="panel" aria-label="External Coding Agent Adapter"><h2>Внешние coding-agent</h2><p role="status">Состояние: {projection?.state ?? 'ожидание Core'} · контроль: {projection?.control_level ?? meta?.core_control_level ?? 'unavailable'}</p><p>Протокол: {projection?.protocol ?? meta?.contract_id ?? 'нет'} · presets: {meta?.preset_count ?? 0} · активные run: {meta?.active_runs ?? 0}</p><p>Credentials: только declared slots; raw payload: {meta?.raw_payload === false ? 'не передаётся' : 'запрещён'}. Opaque executor не считается fully approval controlled.</p>{projection && projection.status !== 'ok' && <p>Ошибка: {projection.error_code || projection.status}</p>}</section>
}
