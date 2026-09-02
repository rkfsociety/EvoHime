import { useEffect, useState } from 'react'
import type { AgentGitChangeSetsProjection, ConnectionState, ShellEvent } from '@shared/api'
import { useShellApi } from './shell-api'

const OPERATIONS = ['get_candidate', 'observe', 'candidate', 'commit', 'undo', 'keep'] as const
type Operation = typeof OPERATIONS[number]

export function AgentGitChangeSetsPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi(); const [operation, setOperation] = useState<Operation>('get_candidate'); const [changeSetId, setChangeSetId] = useState('set-1'); const [payload, setPayload] = useState(''); const [projection, setProjection] = useState<AgentGitChangeSetsProjection | null>(null); const [message, setMessage] = useState('')
  useEffect(() => api?.subscribe((event: ShellEvent) => { if (event.kind === 'core-event' && event.event.agentGitChangeSets) setProjection(event.event.agentGitChangeSets) }), [api])
  const send = async (): Promise<void> => { if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }; const result = await api.invoke('core.agentGitChangeSets', { operation, changeSetId, payload }); setMessage(result.ok ? 'Запрос принят Core.' : result.message) }
  const data = projection?.projection && typeof projection.projection === 'object' && projection.projection !== null ? projection.projection as Record<string, unknown> : null
  return <section aria-label="Agent Git Change Sets"><h3>Agent Git Change Sets</h3><p>Commit candidate показывает только доказуемые agent-owned пути. Shared index, ambiguous changes и эффекты commit/undo требуют отдельного Git preflight.</p><div><strong>Set:</strong> {String(projection?.changeSetId ?? '—')} <strong>Status:</strong> {String(projection?.status ?? '—')} <strong>Included:</strong> {String((data?.['included_paths'] as unknown[] | undefined)?.length ?? 0)}</div><label>Операция <select value={operation} onChange={event => setOperation(event.target.value as Operation)}>{OPERATIONS.map(item => <option key={item}>{item}</option>)}</select></label><label>Change set ID<input value={changeSetId} onChange={event => setChangeSetId(event.target.value)} /></label><label>Bounded JSON<textarea aria-label="Agent Git Change Sets JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={64 * 1024} /></label><button type="button" onClick={() => void send()}>Отправить в Core</button>{message ? <p role="status">{message}</p> : null}</section>
}
