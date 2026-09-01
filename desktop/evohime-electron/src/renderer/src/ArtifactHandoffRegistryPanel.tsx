import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

type Projection = { status?: string; error_code?: string; projection_json?: { artifacts?: Array<{ artifact_id?: string; revision?: number; state?: string; content_hash?: string }>; raw_payload?: boolean } }

/** Core projection of semantic artifact revisions; no local state machine or payload bytes. */
export function ArtifactHandoffRegistryPanel(): React.JSX.Element {
  const api = useShellApi(); const [projection, setProjection] = useState<Projection | null>(null)
  useEffect(() => {
    if (!api) return
    const unsubscribe = api.subscribe((event) => { if (event.kind !== 'core-event' || event.event.eventType !== 'artifact_handoff_registry.result') return; try { setProjection(JSON.parse(event.event.payload) as Projection) } catch { setProjection({ status: 'invalid_projection' }) } })
    void api.invoke('artifactHandoffRegistry.list', { requestId: crypto.randomUUID(), projectId: 'default', correlationId: crypto.randomUUID(), idempotencyKey: crypto.randomUUID() })
    return unsubscribe
  }, [api])
  const artifacts = projection?.projection_json?.artifacts ?? []
  return <section className="panel" aria-label="Artifact Handoff Registry"><h2>Реестр передачи артефактов</h2><p role="status">Ревизий: {artifacts.length} · состояние: {projection?.status ?? 'ожидание Core'}</p><ul>{artifacts.map((item) => <li key={`${item.artifact_id}:${item.revision}`}>{item.artifact_id} · rev {item.revision} · {item.state} · {item.content_hash}</li>)}</ul><p>Показываются только bounded metadata/projection; bytes, prompts, outputs и credentials не передаются.</p>{projection && projection.status !== 'ok' && <p>Ошибка: {projection.error_code || projection.status}</p>}</section>
}
