import { useEffect, useState } from 'react'
import type { ConnectionState, KnowledgeSourceRegistryProjection, ShellEvent } from '@shared/api'
import { useShellApi } from './shell-api'

const OPERATIONS = ['collection_get', 'research_session', 'research_session_transition', 'research_artifact', 'research_artifact_get', 'research_delta', 'research_artifact_promote'] as const
type Operation = typeof OPERATIONS[number]

export function ResearchWorkspacePanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [operation, setOperation] = useState<Operation>('collection_get')
  const [sourceId, setSourceId] = useState('collection-1')
  const [payload, setPayload] = useState('{}')
  const [projection, setProjection] = useState<KnowledgeSourceRegistryProjection | null>(null)
  const [message, setMessage] = useState('')

  useEffect(() => api?.subscribe((event: ShellEvent) => {
    if (event.kind === 'core-event' && event.event.knowledgeSourceRegistry) setProjection(event.event.knowledgeSourceRegistry)
  }), [api])

  const send = async (): Promise<void> => {
    if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }
    const result = await api.invoke('core.knowledgeSourceRegistry', { operation, sourceId, payload })
    setMessage(result.ok ? 'Запрос принят Core.' : result.message)
  }

  return <section aria-label="Grounded Research Workspace">
    <h3>Grounded Research Workspace</h3>
    <p>Collections, revisions, evidence, coverage и artifacts принадлежат Core; UI показывает только bounded projection.</p>
    <label>Операция<select value={operation} onChange={event => setOperation(event.target.value as Operation)}>{OPERATIONS.map(item => <option key={item}>{item}</option>)}</select></label>
    <label>Collection / artifact ID<input value={sourceId} onChange={event => setSourceId(event.target.value)} maxLength={128} /></label>
    <label>Payload JSON<textarea aria-label="Research payload JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={32 * 1024} /></label>
    <button type="button" onClick={() => void send()}>Отправить в Core</button>
    {projection ? <pre>{JSON.stringify(projection, null, 2)}</pre> : null}
    {message ? <p role="status">{message}</p> : null}
  </section>
}
