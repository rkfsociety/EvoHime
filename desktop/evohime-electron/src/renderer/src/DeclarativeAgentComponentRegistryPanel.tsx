import { useState } from 'react'
import type { ConnectionState } from '@shared/api'
import { useShellApi } from './shell-api'

export function DeclarativeAgentComponentRegistryPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi(); const [payload, setPayload] = useState('{}'); const [message, setMessage] = useState('')
  const send = async (operation: 'create' | 'get' | 'validate' | 'replace' | 'diff') => { if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }; const result = await api.invoke('core.declarativeAgentComponentRegistry', { operation, registryId: 'component-registry', payload, expectedRevision: 1 }); setMessage(result.ok ? 'Запрос принят Core.' : result.message) }
  return <section aria-label="Declarative Agent Component Registry"><h3>Component Registry</h3><p>Versioned built-in providers, schema validation и migration без dynamic code loading.</p><textarea aria-label="Component registry JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={64 * 1024} /><div>{(['create', 'get', 'validate', 'replace', 'diff'] as const).map(operation => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}</div>{message ? <p role="status">{message}</p> : null}</section>
}
