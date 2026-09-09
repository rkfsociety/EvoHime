import { useState } from 'react'

import type { ConnectionState } from '@shared/api'
import { useShellApi } from './shell-api'

export function CodeReviewLanePanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [reviewId, setReviewId] = useState('review')
  const [payload, setPayload] = useState('{}')
  const [message, setMessage] = useState('')
  const send = async (operation: 'get' | 'save' | 'reconcile' | 'interrupt'): Promise<void> => {
    if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }
    const result = await api.invoke('core.codeReviewLane', { operation, reviewId, payload, expectedRevision: 0, idempotencyKey: crypto.randomUUID() })
    setMessage(result.ok ? 'Запрос принят Core.' : result.message)
  }
  return <section className="panel" aria-label="Code Review Lane"><h2>Code Review Lane</h2><p>Core хранит target identity, coverage, findings и безопасный verdict; UI показывает только metadata.</p><input value={reviewId} onChange={event => setReviewId(event.target.value)} maxLength={256} aria-label="Review ID"/><textarea value={payload} onChange={event => setPayload(event.target.value)} maxLength={512 * 1024} aria-label="Review JSON"/><div>{(['get', 'save', 'reconcile', 'interrupt'] as const).map(operation => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}</div>{message ? <p role="status">{message}</p> : null}</section>
}
