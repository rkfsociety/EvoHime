import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
import type { ConnectionState } from '@shared/api'

/** Metadata-only projection of the Core-owned MemoryView/recall contract. */
export function MemoryViewsAndAdaptiveRecallPanel({ connection, events }: { readonly connection: ConnectionState; readonly events: readonly { readonly eventType: string; readonly payload: string }[] }): React.JSX.Element {
  const api = useShellApi()
  const [viewId, setViewId] = useState('')
  const [payload, setPayload] = useState('')
  const [result, setResult] = useState<unknown>(null)
  const [message, setMessage] = useState('')
  useEffect(() => {
    const event = events.find((item) => item.eventType === 'memory_views_and_adaptive_recall.result')
    if (!event) return
    try { setResult(JSON.parse(event.payload)); setMessage('') } catch { setMessage('Core вернул некорректную memory projection.') }
  }, [events])
  const send = async (operation: 'save_view' | 'inspect' | 'recall'): Promise<void> => {
    if (!api || connection !== 'connected' || !viewId.trim() || (operation !== 'inspect' && !payload.trim())) { setMessage('Нужны подключение, view ID и bounded JSON payload.'); return }
    const response = await api.invoke('core.memoryViewsAndAdaptiveRecall', { operation, viewId: viewId.trim(), payload: payload.trim(), expectedVersion: 0, idempotencyKey: crypto.randomUUID() })
    if (!response.ok) setMessage(response.message)
  }
  return <section aria-label="Memory Views and Adaptive Recall"><h3>Memory Views &amp; Adaptive Recall</h3><p>Core ограничивает scope, read/write права и глубину retrieval; renderer получает только metadata projection и причины решения.</p><label>View ID <input value={viewId} onChange={(event) => setViewId(event.target.value)} maxLength={128} /></label><label>View/recall JSON <textarea value={payload} onChange={(event) => setPayload(event.target.value)} maxLength={256 * 1024} /></label><div>{(['save_view', 'inspect', 'recall'] as const).map((operation) => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}</div>{result ? <pre>{JSON.stringify(result, null, 2)}</pre> : null}{message ? <p role="status">{message}</p> : null}</section>
}
