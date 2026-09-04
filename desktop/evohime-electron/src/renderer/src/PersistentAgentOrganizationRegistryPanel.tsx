import { useEffect, useState } from 'react'
import type { ConnectionState, PersistentAgentOrganizationRegistryProjection, ShellEvent } from '@shared/api'
import { useShellApi } from './shell-api'

const OPERATIONS = ['list', 'get', 'history', 'create', 'revise', 'activate', 'pause', 'suspend', 'resume', 'retire', 'reporting_set', 'goal_bind', 'goal_unbind', 'assignment_create', 'assignment_cancel', 'resolve', 'availability', 'activity', 'recover'] as const
type Operation = typeof OPERATIONS[number]

/** Core-owned metadata view. The renderer never interprets organization data as authority. */
export function PersistentAgentOrganizationRegistryPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [operation, setOperation] = useState<Operation>('list')
  const [agentId, setAgentId] = useState('')
  const [ownerScope, setOwnerScope] = useState('application:application')
  const [payload, setPayload] = useState('{}')
  const [projection, setProjection] = useState<PersistentAgentOrganizationRegistryProjection | null>(null)
  const [message, setMessage] = useState('')

  useEffect(() => api?.subscribe((event: ShellEvent) => {
    if (event.kind === 'core-event' && event.event.persistentAgentOrganizationRegistry) setProjection(event.event.persistentAgentOrganizationRegistry)
  }), [api])

  const send = async (): Promise<void> => {
    if (!api || connection !== 'connected') { setMessage('Нужно подключение к Core.'); return }
    const result = await api.invoke('core.persistentAgentOrganizationRegistry', { operation, agentId, ownerScope, payload, expectedRevision: 0, idempotencyKey: crypto.randomUUID() })
    setMessage(result.ok ? 'Запрос принят Core.' : result.message)
  }

  return <section className="panel" aria-label="Persistent Agent Organization Registry">
    <h2>Организация постоянных агентов</h2>
    <p>Durable identity, reporting graph, Goal revision и assignment snapshots принадлежат Core. Runtime, grants, credentials и raw output здесь не хранятся.</p>
    <label>Операция<select value={operation} onChange={event => setOperation(event.target.value as Operation)}>{OPERATIONS.map(item => <option key={item}>{item}</option>)}</select></label>
    <label>Agent ID<input value={agentId} onChange={event => setAgentId(event.target.value)} maxLength={128} /></label>
    <label>Owner scope<input value={ownerScope} onChange={event => setOwnerScope(event.target.value)} maxLength={128} /></label>
    <label>Payload JSON<textarea value={payload} onChange={event => setPayload(event.target.value)} maxLength={64 * 1024} /></label>
    <button type="button" onClick={() => void send()}>Отправить в Core</button>
    {projection ? <><p role="status">{projection.operation}: {projection.status} · revision {projection.revision}</p><pre>{JSON.stringify(projection.projection, null, 2)}</pre></> : <p role="status">Ожидание проекции Core.</p>}
    {message ? <p role="status">{message}</p> : null}
  </section>
}
