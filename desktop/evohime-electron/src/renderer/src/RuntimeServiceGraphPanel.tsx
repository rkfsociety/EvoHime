import { useState } from 'react'
import type { ConnectionState } from '@shared/api'
import { useShellApi } from './shell-api'

export function RuntimeServiceGraphPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [graphId, setGraphId] = useState('runtime-graph')
  const [payload, setPayload] = useState('{}')
  const [message, setMessage] = useState('')
  const send = async (operation: 'save' | 'get' | 'pin' | 'activate' | 'supersede') => {
    if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }
    const result = await api.invoke('core.runtimeServiceGraph', { operation, graphId, payload, expectedRevision: 0, idempotencyKey: crypto.randomUUID() })
    setMessage(result.ok ? 'Запрос принят Core; graph остаётся metadata-only projection.' : result.message)
  }
  return <section className="panel" aria-label="Runtime Service Graph"><h2>Runtime Service Graph</h2><p>Core владеет revision, policy и runtime pin; renderer не исполняет граф.</p><input value={graphId} onChange={event => setGraphId(event.target.value)} maxLength={128} /><textarea value={payload} onChange={event => setPayload(event.target.value)} maxLength={512 * 1024} /><div>{(['save', 'get', 'pin', 'activate', 'supersede'] as const).map(operation => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}</div>{message ? <p role="status">{message}</p> : null}</section>
}
