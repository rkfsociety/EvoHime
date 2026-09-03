import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
import type { ConnectionState } from '@shared/api'

export function DeclarativeRuntimeComponentsPanel({ connection, events }: { readonly connection: ConnectionState; readonly events: readonly { readonly eventType: string; readonly payload: string }[] }): React.JSX.Element {
  const api = useShellApi(); const [id, setId] = useState(''); const [payload, setPayload] = useState(''); const [result, setResult] = useState(''); const [message, setMessage] = useState('')
  useEffect(() => { const event = events.find(item => item.eventType === 'declarative_runtime_components.result'); if (event) setResult(event.payload) }, [events])
  const send = async (operation: 'save' | 'inspect' | 'rehydrate' | 'transition'): Promise<void> => { if (!api || connection !== 'connected' || !id.trim() || (operation !== 'inspect' && !payload.trim())) { setMessage('Нужны подключение, component ID и bounded JSON.'); return }; const response = await api.invoke('core.declarativeRuntimeComponents', { operation, componentId: id.trim(), payload: payload.trim(), expectedVersion: 0, idempotencyKey: crypto.randomUUID() }); if (!response.ok) setMessage(response.message) }
  return <section aria-label="Declarative Runtime Components"><h3>Declarative Runtime Components</h3><p>Core хранит только versioned data-конфигурацию; provider выбирается из существующего Core registry.</p><label>Component ID <input value={id} onChange={event => setId(event.target.value)} maxLength={128} /></label><label>Config/action JSON <textarea value={payload} onChange={event => setPayload(event.target.value)} maxLength={512 * 1024} /></label><div>{(['save', 'inspect', 'rehydrate', 'transition'] as const).map(operation => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}</div>{result ? <pre>{result}</pre> : null}{message ? <p role="status">{message}</p> : null}</section>
}
