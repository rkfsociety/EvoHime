import { useState } from 'react'
import type { ConnectionState } from '@shared/api'
import { useShellApi } from './shell-api'

export function StaticAnalysisPacksPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi(); const [packId, setPackId] = useState('pack'); const [payload, setPayload] = useState('{}'); const [message, setMessage] = useState('')
  const send = async (operation: 'register' | 'inspect' | 'evaluate'): Promise<void> => { if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }; const result = await api.invoke('core.staticAnalysisPacks', { operation, packId, payload, expectedRevision: 0, idempotencyKey: crypto.randomUUID() }); setMessage(result.ok ? 'Запрос принят Core.' : result.message) }
  return <section className="panel" aria-label="Static Analysis Packs"><h2>Static Analysis Packs</h2><p>Пакеты, coverage и adoption остаются evidence metadata; analyzer execution не подразумевается.</p><input value={packId} onChange={event => setPackId(event.target.value)} maxLength={256} aria-label="Pack ID"/><textarea value={payload} onChange={event => setPayload(event.target.value)} maxLength={512 * 1024} aria-label="Pack JSON"/><div>{(['register', 'inspect', 'evaluate'] as const).map(operation => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}</div>{message ? <p role="status">{message}</p> : null}</section>
}
