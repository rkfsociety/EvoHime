import { useEffect, useState } from 'react'
import type { ConnectionState, WorkspaceStateCheckpointProjection } from '@shared/api'
import { useShellApi } from './shell-api'

type Operation = 'create' | 'compare' | 'restore' | 'restore_task' | 'restore_both'

export function WorkspaceStateCheckpointsPanel({ connection, events }: { readonly connection: ConnectionState; readonly events: readonly { readonly eventType: string; readonly payload: string }[] }): React.JSX.Element {
  const api = useShellApi()
  const [projectId, setProjectId] = useState('')
  const [taskId, setTaskId] = useState('')
  const [checkpointId, setCheckpointId] = useState('')
  const [projection, setProjection] = useState<WorkspaceStateCheckpointProjection | null>(null)
  const [message, setMessage] = useState('')

  useEffect(() => {
    const event = events.find((item) => item.eventType === 'workspace_state_checkpoint.result')
    if (!event) return
    try {
      const raw = JSON.parse(event.payload) as Record<string, unknown>
      const value: WorkspaceStateCheckpointProjection = {
        schemaVersion: Number(raw['schema_version'] ?? raw['schemaVersion'] ?? 1),
        operation: String(raw['operation'] ?? ''),
        projectId: String(raw['project_id'] ?? raw['projectId'] ?? ''),
        taskId: String(raw['task_id'] ?? raw['taskId'] ?? ''),
        checkpointId: String(raw['checkpoint_id'] ?? raw['checkpointId'] ?? ''),
        state: String(raw['state'] ?? ''),
        conflictCount: Number(raw['conflict_count'] ?? raw['conflictCount'] ?? 0),
        fileCount: Number(raw['file_count'] ?? raw['fileCount'] ?? 0),
        snapshotHash: String(raw['snapshot_hash'] ?? raw['snapshotHash'] ?? ''),
        errorCode: String(raw['error_code'] ?? raw['errorCode'] ?? '')
      }
      if (value.operation) setProjection(value)
    } catch {
      setMessage('Core вернул некорректную bounded projection.')
    }
  }, [events])

  const invoke = async (operation: Operation): Promise<void> => {
    if (!api || connection !== 'connected' || !projectId.trim()) {
      setMessage('Нужны подключение к Core и идентификатор проекта.')
      return
    }
    const result = await api.invoke('core.workspaceStateCheckpoint', {
      operation,
      projectId: projectId.trim(),
      ...(taskId.trim() ? { taskId: taskId.trim() } : {}),
      ...(checkpointId.trim() ? { checkpointId: checkpointId.trim() } : {}),
      idempotencyKey: crypto.randomUUID()
    })
    if (!result.ok) setMessage('Команда отклонена: ' + result.message)
  }

  return <section aria-label="Workspace State Checkpoints" className="plan-artifact-panel">
    <h3>Workspace State Checkpoints</h3>
    <p>Bounded metadata-only projection состояния workspace. Восстановление требует явного действия и выполняется Core.</p>
    <label>Проект <input value={projectId} onChange={(event) => setProjectId(event.target.value)} maxLength={256} /></label>
    <label>Задача <input value={taskId} onChange={(event) => setTaskId(event.target.value)} maxLength={256} /></label>
    <label>Checkpoint <input value={checkpointId} onChange={(event) => setCheckpointId(event.target.value)} maxLength={256} /></label>
    <div><button type="button" onClick={() => void invoke('create')}>Создать</button><button type="button" onClick={() => void invoke('compare')}>Сравнить</button><button type="button" onClick={() => void invoke('restore')}>Восстановить workspace</button><button type="button" onClick={() => void invoke('restore_task')}>Восстановить задачу</button><button type="button" onClick={() => void invoke('restore_both')}>Восстановить оба</button></div>
    {projection ? <p role="status">{projection.state} · файлов: {projection.fileCount} · конфликтов: {projection.conflictCount} · {projection.snapshotHash}</p> : null}
    {message ? <p role="status">{message}</p> : null}
  </section>
}
