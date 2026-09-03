import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
import type { ConnectionState } from '@shared/api'

export function ExtensionConformanceKitPanel({ connection, events }: { readonly connection: ConnectionState; readonly events: readonly { readonly eventType: string; readonly payload: string }[] }): React.JSX.Element {
  const api = useShellApi(); const [id, setId] = useState(''); const [payload, setPayload] = useState(''); const [result, setResult] = useState(''); const [message, setMessage] = useState('')
  useEffect(() => { const event = events.find(item => item.eventType === 'extension_conformance_kit.result'); if (event) setResult(event.payload) }, [events])
  const send = async (operation: 'run' | 'register' | 'inspect'): Promise<void> => { if (!api || connection !== 'connected' || !id.trim() || (operation !== 'inspect' && !payload.trim())) { setMessage('Нужны подключение, subject ID и bounded JSON.'); return }; const response = await api.invoke('core.extensionConformanceKit', { operation, subjectId: id.trim(), payload: payload.trim(), expectedVersion: 0, idempotencyKey: crypto.randomUUID() }); if (!response.ok) setMessage(response.message) }
  return <section aria-label="Extension Conformance Kit"><h3>Extension Conformance Kit</h3><p>Ephemeral contract harness проверяет provider, adapter, workbench и extension descriptors без запуска production effects.</p><label>Subject ID <input value={id} onChange={event => setId(event.target.value)} maxLength={128} /></label><label>Descriptor/probe JSON <textarea value={payload} onChange={event => setPayload(event.target.value)} maxLength={512 * 1024} /></label><div>{(['run', 'register', 'inspect'] as const).map(operation => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}</div>{result ? <pre>{result}</pre> : null}{message ? <p role="status">{message}</p> : null}</section>
}
