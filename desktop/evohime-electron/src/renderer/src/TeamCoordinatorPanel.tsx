import { useEffect, useState } from 'react'
import type { ConnectionState, ShellEvent, TeamCoordinatorProjection } from '@shared/api'
import { useShellApi } from './shell-api'

const OPERATIONS = ['create', 'get', 'list', 'propose', 'assign', 'consult', 'review', 'decompose', 'reassign', 'cancel'] as const
type Operation = typeof OPERATIONS[number]
const WORK_ITEM = JSON.stringify({ schema_version: 1, id: 'work-item', objective: 'Проверить состояние репозитория', required_output_contract: 'report-v1', required_capabilities: ['repo.read'], preferred_role_tags: ['rust'], dependencies: [], priority: 1, estimated_cost_class: 'small', status: 'unassigned', assigned_instance_id: null, attempt: 0, max_attempts: 4, created_by: 'coordinator', evidence_refs: [], revision: 1 }, null, 2)

export function TeamCoordinatorPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [operation, setOperation] = useState<Operation>('create')
  const [payload, setPayload] = useState(WORK_ITEM)
  const [revision, setRevision] = useState(1)
  const [message, setMessage] = useState('')
  const [projection, setProjection] = useState<TeamCoordinatorProjection | null>(null)
  useEffect(() => api?.subscribe((event: ShellEvent) => {
    if (event.kind === 'core-event' && event.event.teamCoordinator) setProjection(event.event.teamCoordinator)
  }), [api])
  const send = async (): Promise<void> => {
    if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }
    const result = await api.invoke('core.teamCoordinator', { operation, workItemId: 'work-item', payload, expectedRevision: revision })
    setMessage(result.ok ? 'Запрос принят Core.' : result.message)
    if (result.ok && operation !== 'create') setRevision(value => value + 1)
  }
  const data = projection?.projection && typeof projection.projection === 'object' && projection.projection !== null ? projection.projection as Record<string, unknown> : null
  const queueCount = data?.['queue_count'] === undefined ? (data?.['work_item'] ? 1 : 0) : String(data['queue_count'])
  const candidateCount = data?.['candidate_count'] === undefined ? '0' : String(data['candidate_count'])
  const assignmentCount = data?.['assignment_count'] === undefined ? '0' : String(data['assignment_count'])
  return <section aria-label="Team Coordinator"><h3>Team Coordinator</h3><p>Очередь, roster, назначения и эскалации отображаются как bounded projection Core. Renderer не принимает решений и не получает секреты.</p><div aria-label="Team Coordinator state"><strong>Очередь:</strong> {queueCount} <strong>Roster:</strong> {candidateCount} <strong>Назначения:</strong> {assignmentCount} <strong>Эскалация:</strong> {data?.['escalation'] ? String(data['escalation']) : 'нет'}</div><label>Операция <select value={operation} onChange={event => setOperation(event.target.value as Operation)}>{OPERATIONS.map(item => <option key={item}>{item}</option>)}</select></label><label>Payload JSON<textarea aria-label="Team Coordinator JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={64 * 1024} /></label><button type="button" onClick={() => void send()}>Отправить в Core</button>{message ? <p role="status">{message}</p> : null}</section>
}
