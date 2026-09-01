import { useEffect, useState } from 'react'
import type { ConnectionState, PlanArtifactProjection } from '@shared/api'
import { useShellApi } from './shell-api'

export function PlanArtifactPanel({ connection, events }: { readonly connection: ConnectionState; readonly events: readonly { readonly eventType: string; readonly payload: string }[] }): React.JSX.Element {
  const api = useShellApi()
  const [id, setId] = useState('')
  const [projection, setProjection] = useState<PlanArtifactProjection | null>(null)
  const [message, setMessage] = useState('')
  useEffect(() => { const event = events.find((item) => item.eventType === 'plan_artifact.result'); if (!event) return; try { const value = JSON.parse(event.payload) as PlanArtifactProjection; if (value && typeof value.id === 'string') setProjection(value) } catch { setMessage('Core вернул некорректную bounded projection.') } }, [events])
  const read = async (): Promise<void> => { if (!api || !id.trim() || connection !== 'connected') { setMessage('Нужны подключение к Core и идентификатор.'); return }; await api.invoke('core.planArtifactRead', { artifactId: id.trim() }) }
  const action = async (operation: 'transition' | 'execute', status = ''): Promise<void> => { if (!api || !projection) return; await api.invoke('core.planArtifactAction', { operation, artifactId: projection.id, expectedVersion: projection.version, status, policySnapshotHash: 'ui-policy-request', correlationId: crypto.randomUUID(), idempotencyKey: crypto.randomUUID() }) }
  return <section aria-label="Plan Artifact" className="plan-artifact-panel"><h3>Plan Artifact</h3><p>Core-owned versioned план. Renderer только показывает projection и отправляет явные действия.</p><label>Идентификатор <input value={id} onChange={(event) => setId(event.target.value)} maxLength={256} /></label><button type="button" onClick={() => void read()}>Прочитать</button>{projection ? <><p><strong>{projection.status}</strong> · revision {projection.revision} · version {projection.version}</p><ul>{projection.steps.map((step) => <li key={step.id}>{step.description} ({step.risk})</li>)}</ul><ul>{projection.acceptanceCriteria.map((criterion) => <li key={criterion.id}>{criterion.description} · {criterion.evidenceKind}</li>)}</ul>{projection.status === 'draft' ? <button type="button" onClick={() => void action('transition', 'accepted')}>Принять план</button> : null}{projection.status === 'accepted' ? <button type="button" onClick={() => void action('execute')}>Execute Plan</button> : null}</> : null}{message ? <p role="status">{message}</p> : null}</section>
}
