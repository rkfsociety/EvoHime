import { useState } from 'react'
import type { ConnectionState } from '@shared/api'
import { useShellApi } from './shell-api'

const OPERATIONS = ['register_descriptor', 'get', 'start', 'stop', 'restart', 'session', 'query', 'proposal'] as const
export function LanguageIntelligencePanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi(); const [requestId, setRequestId] = useState('language-request-1'); const [payload, setPayload] = useState('{}'); const [message, setMessage] = useState('')
  const send = async (operation: typeof OPERATIONS[number]): Promise<void> => { if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }; const result = await api.invoke('core.languageIntelligence', { operation, requestId, payload, expectedRevision: 0, idempotencyKey: crypto.randomUUID() }); setMessage(result.ok ? 'Запрос принят Core; результат остаётся revision-bound metadata projection.' : result.message) }
  return <section className="panel" aria-label="Language Intelligence"><h2>Language Intelligence</h2><p>Core управляет managed LSP lifecycle, revision/freshness и proposal gate. Renderer не получает raw LSP JSON или workspace authority.</p><label>Request ID <input value={requestId} onChange={event => setRequestId(event.target.value)} maxLength={256} /></label><label>Bounded metadata JSON <textarea aria-label="Language metadata JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={512 * 1024} /></label><div>{OPERATIONS.map(operation => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}</div>{message ? <p role="status">{message}</p> : null}</section>
}
