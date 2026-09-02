import { useState } from 'react'
import type { ConnectionState } from '@shared/api'
import { useShellApi } from './shell-api'

export function DependencyAwareTaskGraphPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi(); const [payload, setPayload] = useState('{}'); const [message, setMessage] = useState('')
  const send = async (operation: 'create' | 'get' | 'validate' | 'apply_patch') => { if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }; const result = await api.invoke('core.dependencyAwareTaskGraph', { operation, graphId: 'task-graph', payload, expectedRevision: 1, grants: [] }); setMessage(result.ok ? 'Запрос принят Core.' : result.message) }
  return <section aria-label="Dependency-aware Task Graph"><h3>Task Graph</h3><p>Core-owned DAG, ready-set и downstream invalidation; renderer только показывает projection.</p><textarea aria-label="Task graph JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={512 * 1024} /><div>{(['create', 'get', 'validate', 'apply_patch'] as const).map(operation => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}</div>{message ? <p role="status">{message}</p> : null}</section>
}
