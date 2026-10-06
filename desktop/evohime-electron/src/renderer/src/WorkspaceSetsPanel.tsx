import { translate } from './i18n'
import { useEffect, useState } from 'react'
import type { ConnectionState, ShellEvent, WorkspaceSetsProjection } from '@shared/api'
import { useShellApi } from './shell-api'

const OPERATIONS = ['get', 'create', 'update', 'bind', 'search'] as const
type Operation = typeof OPERATIONS[number]

export function WorkspaceSetsPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [operation, setOperation] = useState<Operation>('get')
  const [setId, setSetId] = useState('set-1')
  const [payload, setPayload] = useState('')
  const [projection, setProjection] = useState<WorkspaceSetsProjection | null>(null)
  const [message, setMessage] = useState('')
  useEffect(() => api?.subscribe((event: ShellEvent) => {
    if (event.kind === 'core-event' && event.event.workspaceSets) setProjection(event.event.workspaceSets)
  }), [api])
  const send = async (): Promise<void> => {
    if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }
    const result = await api.invoke('core.workspaceSets', { operation, setId, payload })
    setMessage(result.ok ? 'Запрос принят Core.' : result.message)
  }
  const data = projection?.projection && typeof projection.projection === 'object' && projection.projection !== null ? projection.projection as Record<string, unknown> : null
  return <section aria-label="Workspace Sets"><h3>Workspace Sets</h3><p>{translate("Core-owned multi-root boundary: каждый root сохраняет отдельные grants, VCS и revision identity. Renderer только показывает projection и отправляет явные actions.")}</p><div aria-label="Workspace set summary"><strong>Set:</strong> {translate(String(data?.['id'] ?? projection?.setId ?? '—'))} <strong>Version:</strong> {translate(String(data?.['version'] ?? projection?.version ?? '—'))} <strong>Roots:</strong> {translate(String(data?.['root_count'] ?? 0))} <strong>Hash:</strong> {translate(String(data?.['content_hash'] ?? '—'))}</div><label>{translate("Операция")}<select value={operation} onChange={event => setOperation(event.target.value as Operation)}>{OPERATIONS.map(item => <option key={item}>{translate(item)}</option>)}</select></label><label>Set ID<input value={setId} onChange={event => setSetId(event.target.value)} /></label><label>Workspace Set JSON<textarea aria-label="Workspace Sets JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={64 * 1024} /></label><button type="button" onClick={() => void send()}>{translate("Отправить в Core")}</button>{message ? <p role="status">{translate(message)}</p> : null}</section>
}
