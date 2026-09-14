import { useState } from 'react'
import type { ConnectionState } from '@shared/api'
import { useShellApi } from './shell-api'

const OPERATIONS = ['save_profile', 'start', 'status', 'reconcile', 'adjudicate', 'complete', 'cancel'] as const

export function MultiReviewerEnsemblePanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [ensembleId, setEnsembleId] = useState('ensemble-1')
  const [payload, setPayload] = useState('{}')
  const [message, setMessage] = useState('')
  const send = async (operation: typeof OPERATIONS[number]): Promise<void> => {
    if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }
    const result = await api.invoke('core.multiReviewerEnsemble', { operation, ensembleId, payload, expectedRevision: 0, idempotencyKey: crypto.randomUUID() })
    setMessage(result.ok ? 'Запрос принят Core. UI показывает только metadata/provenance projection.' : result.message)
  }
  return <section className="panel" aria-label="Multi-Reviewer Ensemble">
    <h2>Multi-Reviewer Ensemble</h2>
    <p>Core объединяет независимые reviewer slots и adjudication. Renderer не является authority и не получает raw review prose.</p>
    <label>Ensemble ID <input value={ensembleId} onChange={event => setEnsembleId(event.target.value)} maxLength={256} /></label>
    <label>Metadata JSON <textarea aria-label="Ensemble metadata JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={512 * 1024} /></label>
    <div>{OPERATIONS.map(operation => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}</div>
    {message ? <p role="status">{message}</p> : null}
  </section>
}
