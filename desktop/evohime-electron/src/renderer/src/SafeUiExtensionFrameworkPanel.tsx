import { useState } from 'react'
import type { ConnectionState } from '@shared/api'
import { useShellApi } from './shell-api'

export function SafeUiExtensionFrameworkPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi(); const [payload, setPayload] = useState('{}'); const [message, setMessage] = useState('')
  const send = async (operation: 'install' | 'get' | 'validate' | 'enable' | 'disable' | 'update') => { if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }; const result = await api.invoke('core.safeUiExtensionFramework', { operation, extensionId: 'ui-extension', payload, expectedRevision: 1 }); setMessage(result.ok ? 'Запрос принят Core.' : result.message) }
  return <section aria-label="Safe UI Extension Framework"><h3>UI Extensions</h3><p>Declarative host-rendered contributions; install и enable разделены, arbitrary code запрещён.</p><textarea aria-label="UI extension manifest JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={64 * 1024} /><div>{(['install', 'get', 'validate', 'enable', 'disable', 'update'] as const).map(operation => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}</div>{message ? <p role="status">{message}</p> : null}</section>
}
