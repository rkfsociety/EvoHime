import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

type Projection = { status?: string; registry_version?: number; default_backend_id?: string; backends?: Array<{ id: string; kind: string; health: string; capability_count: number; has_auth_ref: boolean }>; error_code?: string }

/** Metadata-only view of Core-owned backend registrations and handshake health. */
export function ExecutionBackendRegistryPanel(): React.JSX.Element {
  const api = useShellApi(); const [projection, setProjection] = useState<Projection | null>(null)
  useEffect(() => { if (!api) return; const unsubscribe = api.subscribe((event) => { if (event.kind !== 'core-event' || event.event.eventType !== 'execution_backend_registry.result') return; try { setProjection(JSON.parse(event.event.payload) as Projection) } catch { setProjection({ status: 'invalid_projection' }) } }); void api.invoke('executionBackendRegistry.list', { requestId: crypto.randomUUID(), ownerScope: 'execution-backend-registry', idempotencyKey: crypto.randomUUID() }); return unsubscribe }, [api])
  return <section className="panel" aria-label="Execution Backend Registry"><h2>Среды выполнения</h2>{!projection && <p>Ожидание состояния Core…</p>}{projection?.status === 'ok' && <><p>Версия реестра: {projection.registry_version} · default: {projection.default_backend_id}</p><ul>{projection.backends?.map((backend) => <li key={backend.id}>{backend.id} — {backend.kind}, {backend.health}, capabilities: {backend.capability_count}{backend.has_auth_ref ? ', auth ref' : ''}</li>)}</ul></>}{projection && projection.status !== 'ok' && <p>Состояние: {projection.status} ({projection.error_code || 'unknown'})</p>}<p>Core повторно проверяет capabilities; секреты, prompt, output и executable identities в UI не передаются.</p></section>
}
