import { translate } from './i18n'
import { useEffect, useState } from 'react'
import type { ConnectionState, DurableRemoteTaskBridgeProjection, ShellEvent } from '@shared/api'
import { useShellApi } from './shell-api'

const OPERATIONS = ['status', 'submit', 'poll', 'result', 'cancel'] as const
type Operation = typeof OPERATIONS[number]

export function DurableRemoteTaskBridgePanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [operation, setOperation] = useState<Operation>('status')
  const [remoteTaskId, setRemoteTaskId] = useState('remote-task-1')
  const [payload, setPayload] = useState('')
  const [projection, setProjection] = useState<DurableRemoteTaskBridgeProjection | null>(null)
  const [message, setMessage] = useState('')
  useEffect(() => api?.subscribe((event: ShellEvent) => {
    if (event.kind === 'core-event' && event.event.durableRemoteTaskBridge) setProjection(event.event.durableRemoteTaskBridge)
  }), [api])
  const send = async (): Promise<void> => {
    if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }
    const result = await api.invoke('core.durableRemoteTaskBridge', { operation, remoteTaskId, payload })
    setMessage(result.ok ? 'Запрос принят Core.' : result.message)
  }
  const data = projection?.projection && typeof projection.projection === 'object' && projection.projection !== null ? projection.projection as Record<string, unknown> : null
  return <section aria-label="Durable Remote Task Bridge"><h3>Durable Remote Task Bridge</h3><p>{translate("Долгий MCP/provider task живёт в Core и хранит только bounded metadata и artifact refs; credentials и raw output не передаются в renderer.")}</p><div><strong>Task:</strong> {translate(String(data?.['remote_task_id'] ?? projection?.remoteTaskId ?? '—'))} <strong>Status:</strong> {translate(String(data?.['status'] ?? '—'))} <strong>Version:</strong> {translate(String(data?.['version'] ?? projection?.version ?? '—'))} <strong>Polls:</strong> {translate(String(data?.['poll_attempts'] ?? '—'))}</div><label>{translate("Операция")}<select value={operation} onChange={event => setOperation(event.target.value as Operation)}>{OPERATIONS.map(item => <option key={item}>{translate(item)}</option>)}</select></label><label>Remote task ID<input value={remoteTaskId} onChange={event => setRemoteTaskId(event.target.value)} /></label><label>Payload JSON<textarea aria-label="Durable Remote Task Bridge JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={64 * 1024} /></label><button type="button" onClick={() => void send()}>{translate("Отправить в Core")}</button>{message ? <p role="status">{translate(message)}</p> : null}</section>
}
