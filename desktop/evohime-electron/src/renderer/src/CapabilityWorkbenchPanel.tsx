import { useState } from 'react'
import type { ConnectionState } from '@shared/api'
import { useShellApi } from './shell-api'

const OPERATIONS = ['create','get','start','ready','stop','stopped','reset','degraded','recover','heartbeat','list_tools','call_tool','cancel','resource','snapshot','restore'] as const
type Operation = typeof OPERATIONS[number]
const DESCRIPTOR = JSON.stringify({ schema_version: 1, id: 'repo', version: '1', kind: 'repository', scope: 'project_scoped', concurrency: 'serialized', max_in_flight: 2, lease_ttl_ms: 60000, tools: [{ id: 'status', capability: 'repo.read', title: 'Status' }], resources: [{ id: 'workspace', class: 'filesystem', available: true }] }, null, 2)

export function CapabilityWorkbenchPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [payload, setPayload] = useState(DESCRIPTOR)
  const [operation, setOperation] = useState<Operation>('create')
  const [revision, setRevision] = useState(1)
  const [message, setMessage] = useState('')
  const send = async (): Promise<void> => {
    if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }
    const result = await api.invoke('core.capabilityWorkbench', { operation, instanceId: 'repo-instance', ownerId: 'project-owner', payload, expectedRevision: revision, grants: ['repo.read'] })
    setMessage(result.ok ? 'Запрос принят Core.' : result.message)
    if (result.ok && operation !== 'create') setRevision(value => value + 1)
  }
  return <section aria-label="Capability Workbench"><h3>Capability Workbench</h3><p>Core-owned runtime instances, lifecycle, tools, resources and recovery. Authority и эффекты остаются в Core.</p><label>Операция <select value={operation} onChange={event => setOperation(event.target.value as Operation)}>{OPERATIONS.map(item => <option key={item}>{item}</option>)}</select></label><label>Payload JSON<textarea aria-label="Capability Workbench JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={256 * 1024} /></label><button type="button" onClick={() => void send()}>Отправить в Core</button>{message ? <p role="status">{message}</p> : null}</section>
}
