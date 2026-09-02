import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
import type { ConnectionState } from '@shared/api'

export function WorkspaceBootstrapManifestPanel({ connection, events }: { readonly connection: ConnectionState; readonly events: readonly { readonly eventType: string; readonly payload: string }[] }): React.JSX.Element {
  const api = useShellApi()
  const [projectId, setProjectId] = useState('')
  const [workspaceId, setWorkspaceId] = useState('')
  const [payload, setPayload] = useState('')
  const [message, setMessage] = useState('')
  const [projection, setProjection] = useState<unknown>(null)

  useEffect(() => {
    const event = events.find((item) => item.eventType === 'workspace_bootstrap_manifest.result')
    if (!event) return
    try { setProjection(JSON.parse(event.payload)); setMessage('') } catch { setMessage('Core вернул некорректный результат.') }
  }, [events])

  const send = async (operation: 'validate' | 'discover' | 'save' | 'approve' | 'run'): Promise<void> => {
    if (!api || connection !== 'connected' || !projectId.trim() || !workspaceId.trim() || (operation !== 'discover' && !payload.trim())) {
      setMessage('Нужны подключение, project ID, workspace ID и manifest JSON (для discover его можно оставить пустым).')
      return
    }
    const result = await api.invoke('core.workspaceBootstrapManifest', { operation, projectId: projectId.trim(), workspaceId: workspaceId.trim(), payload: payload.trim(), idempotencyKey: crypto.randomUUID() })
    if (!result.ok) setMessage(result.message)
  }

  return <section aria-label="Workspace Bootstrap Manifest">
    <h3>Workspace Bootstrap Manifest</h3>
    <p>Core проверяет manifest, доверие, fingerprint и политику выполнения. Сырые команды и окружение не выводятся.</p>
    <label>Project ID <input value={projectId} onChange={(event) => setProjectId(event.target.value)} maxLength={128} /></label>
    <label>Workspace ID <input value={workspaceId} onChange={(event) => setWorkspaceId(event.target.value)} maxLength={128} /></label>
    <label>Manifest JSON <textarea value={payload} onChange={(event) => setPayload(event.target.value)} maxLength={64 * 1024} /></label>
    <div>
      {(['discover', 'validate', 'save', 'approve', 'run'] as const).map((operation) => <button key={operation} type="button" onClick={() => void send(operation)}>{operation}</button>)}
    </div>
    {projection ? <pre>{JSON.stringify(projection, null, 2)}</pre> : null}
    {message ? <p role="status">{message}</p> : null}
  </section>
}
