import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
import type { ConnectionState } from '@shared/api'

export function RevisionSafeWorkspaceFilesPanel({ connection, events }: { readonly connection: ConnectionState; readonly events: readonly { readonly eventType: string; readonly payload: string }[] }): React.JSX.Element {
  const api = useShellApi()
  const [projectId, setProjectId] = useState('')
  const [logicalPath, setLogicalPath] = useState('workspace/README.md')
  const [hash, setHash] = useState('')
  const [projection, setProjection] = useState<Record<string, unknown> | null>(null)
  const [message, setMessage] = useState('')
  useEffect(() => { const event = events.find((item) => item.eventType === 'revision_safe_workspace_files.result'); if (!event) return; try { setProjection(JSON.parse(event.payload) as Record<string, unknown>); setMessage('') } catch { setMessage('Core вернул некорректную bounded projection.') } }, [events])
  const send = async (): Promise<void> => {
    if (!api || connection !== 'connected' || !projectId.trim() || !logicalPath.trim()) { setMessage('Нужны подключение к Core, project ID и logical path.'); return }
    const result = await api.invoke('core.revisionSafeWorkspaceFiles', { operation: 'read', projectId: projectId.trim(), logicalPath: logicalPath.trim(), expectedHash: hash.trim(), idempotencyKey: crypto.randomUUID() })
    if (!result.ok) setMessage(result.message)
  }
  return <section aria-label="Revision-safe workspace files" className="revision-safe-files-panel"><h3>Revision-safe workspace files</h3><p>Core владеет namespace и hash; renderer показывает только bounded ref/preview. Изменения выполняются только через одобренные Core tools.</p><label>Project ID <input value={projectId} onChange={(event) => setProjectId(event.target.value)} maxLength={128} /></label><label>Logical path <input value={logicalPath} onChange={(event) => setLogicalPath(event.target.value)} maxLength={4096} /></label><label>Expected SHA-256 hash <input value={hash} onChange={(event) => setHash(event.target.value)} maxLength={128} /></label><button type="button" onClick={() => void send()}>Прочитать</button>{projection ? <pre>{JSON.stringify(projection, null, 2)}</pre> : null}{message ? <p role="status">{message}</p> : null}</section>
}
