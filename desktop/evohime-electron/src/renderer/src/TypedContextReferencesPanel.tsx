import { useState } from 'react'
import type { ConnectionState } from '@shared/api'
import { useShellApi } from './shell-api'

export function TypedContextReferencesPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi(); const [payload, setPayload] = useState('{}'); const [message, setMessage] = useState('')
  const send = async (operation: 'resolve' | 'budget' | 'kinds') => { if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }; const result = await api.invoke('core.typedContextReferences', { operation, refId: 'context-ref', payload }); setMessage(result.ok ? 'Запрос принят Core.' : result.message) }
  return <section aria-label="Typed Context References"><h3>Context References</h3><p>Typed lazy refs без raw content в renderer; Core фиксирует revision/hash и budget.</p><textarea aria-label="Context reference JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={128 * 1024} /><div>{(['resolve', 'budget', 'kinds'] as const).map(operation => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}</div>{message ? <p role="status">{message}</p> : null}</section>
}
