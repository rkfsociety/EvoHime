import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
import type { ArchitectureSnapshotProjection, ConnectionState, ShellEvent } from '@shared/api'

type Operation = 'current' | 'refresh' | 'rebuild' | 'inspect' | 'get' | 'evidence' | 'upstream' | 'downstream' | 'route' | 'compare' | 'review'

export function ArchitectureSnapshotPanel({ connection, workspace }: { readonly connection: ConnectionState; readonly workspace: string | null }): React.JSX.Element {
  const api = useShellApi()
  const [projection, setProjection] = useState<ArchitectureSnapshotProjection | null>(null)
  const [operation, setOperation] = useState<Operation>('current')
  const [payload, setPayload] = useState('{}')
  const [message, setMessage] = useState('')
  useEffect(() => api?.subscribe((event: ShellEvent) => { if (event.kind === 'core-event' && event.event.architectureSnapshot) setProjection(event.event.architectureSnapshot) }), [api])
  const send = async (): Promise<void> => {
    if (!api || connection !== 'connected' || !workspace) { setMessage('Нужно подключение и выбранная папка workspace.'); return }
    const result = await api.invoke('core.architectureSnapshot', { operation, workspaceRoot: workspace, payload, snapshotId: 'architecture-current', idempotencyKey: crypto.randomUUID() })
    setMessage(result.ok ? 'Запрос принят Core.' : result.message)
  }
  return <section aria-label="Architecture Snapshot"><h3>Architecture Snapshot</h3><p>Topology, evidence и drift принадлежат Core; renderer показывает только bounded projection.</p><label>Операция<select value={operation} onChange={event => setOperation(event.target.value as Operation)}>{(['current', 'refresh', 'rebuild', 'inspect', 'get', 'evidence', 'upstream', 'downstream', 'route', 'compare', 'review'] as const).map(item => <option key={item}>{item}</option>)}</select></label><label>Payload JSON<textarea aria-label="Architecture snapshot JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={256 * 1024} /></label><button type="button" onClick={() => void send()}>Отправить в Core</button>{projection?.projection ? <pre>{JSON.stringify(projection.projection, null, 2)}</pre> : null}{message ? <p role="status">{message}</p> : null}</section>
}
