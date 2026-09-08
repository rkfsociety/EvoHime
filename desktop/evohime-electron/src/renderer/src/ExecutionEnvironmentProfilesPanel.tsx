import { useEffect, useState } from 'react'
import type { ConnectionState, ExecutionEnvironmentProfileProjection, ShellEvent } from '@shared/api'
import { useShellApi } from './shell-api'

const OPERATIONS = ['list', 'get', 'create', 'revise', 'preflight', 'activate', 'rollback', 'current', 'history'] as const
type Operation = typeof OPERATIONS[number]

/** A projection-only entry point: resolution and activation remain in Core. */
export function ExecutionEnvironmentProfilesPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi(); const [operation, setOperation] = useState<Operation>('list'); const [profileId, setProfileId] = useState(''); const [scope, setScope] = useState('application:application'); const [payload, setPayload] = useState('{}'); const [message, setMessage] = useState(''); const [projection, setProjection] = useState<ExecutionEnvironmentProfileProjection | null>(null)
  useEffect(() => api?.subscribe((event: ShellEvent) => { if (event.kind === 'core-event' && event.event.executionEnvironmentProfile) setProjection(event.event.executionEnvironmentProfile) }), [api])
  const send = async (): Promise<void> => { if (!api || connection !== 'connected') { setMessage('Нужно подключение к Core.'); return }; const result = await api.invoke('core.executionEnvironmentProfile', { operation, profileId, ownerScope: scope, payload, expectedRevision: 0, idempotencyKey: crypto.randomUUID() }); setMessage(result.ok ? 'Запрос передан в Core.' : result.message) }
  return <section className="panel" aria-label="Execution Environment Profiles"><h2>Среды выполнения</h2><p>Профили содержат только bounded refs. Проверка совместимости, safe boundary и activation выполняются исключительно Core; credentials и owner-конфигурации не отображаются.</p><label>Операция<select value={operation} onChange={event => setOperation(event.target.value as Operation)}>{OPERATIONS.map(item => <option key={item}>{item}</option>)}</select></label><label>Profile ID<input value={profileId} maxLength={128} onChange={event => setProfileId(event.target.value)} /></label><label>Scope<input value={scope} maxLength={128} onChange={event => setScope(event.target.value)} /></label><label>Payload JSON<textarea value={payload} maxLength={64 * 1024} onChange={event => setPayload(event.target.value)} /></label><button type="button" onClick={() => void send()}>Отправить в Core</button><p role="status">{message || (projection ? `${projection.operation}: ${projection.status}` : 'Ожидание проекции Core.')}</p>{projection ? <pre>{JSON.stringify(projection.projection, null, 2)}</pre> : null}</section>
}
