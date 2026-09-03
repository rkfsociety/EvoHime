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
    <h3>Контрольные точки проекта (Workspace Checkpoints)</h3>
    <p className="plan-artifact-panel__intro">Это сохранённый снимок состояния файлов проекта. Он нужен, чтобы перед рискованными изменениями зафиксировать рабочее состояние, сравнить его с текущим и при необходимости безопасно вернуться назад.</p>
    <div className="plan-artifact-panel__guide">
      <h4>Как это работает</h4>
      <ol>
        <li>Укажите идентификатор проекта и нажмите «Создать контрольную точку».</li>
        <li>После изменений укажите ID этой точки и нажмите «Сравнить с точкой».</li>
        <li>Если нужно вернуться назад, выберите восстановление workspace, задачи или обоих состояний.</li>
      </ol>
      <p className="plan-artifact-panel__hint">Сохраняются только ограниченные метаданные и состояние обычных файлов. `.git`, зависимости, build-кэши и ссылки не включаются.</p>
      <p className="plan-artifact-panel__hint">Восстановление может изменить файлы. Если файл успел измениться после создания точки, Core остановит операцию и покажет конфликт вместо молчаливой перезаписи.</p>
    </div>
    <div className="plan-artifact-panel__lookup">
      <label htmlFor="workspace-checkpoint-project">Идентификатор проекта</label>
      <input id="workspace-checkpoint-project" value={projectId} onChange={(event) => setProjectId(event.target.value)} maxLength={256} placeholder="Проект, например project-1" />
      <label htmlFor="workspace-checkpoint-task">Идентификатор задачи <span className="plan-artifact-panel__hint">необязательно</span></label>
      <input id="workspace-checkpoint-task" value={taskId} onChange={(event) => setTaskId(event.target.value)} maxLength={256} placeholder="Заполняйте для состояния конкретной задачи" />
      <label htmlFor="workspace-checkpoint-id">Идентификатор контрольной точки <span className="plan-artifact-panel__hint">нужен для сравнения и восстановления</span></label>
      <input id="workspace-checkpoint-id" value={checkpointId} onChange={(event) => setCheckpointId(event.target.value)} maxLength={256} placeholder="ID точки, созданной ранее" />
    </div>
    <div className="plan-artifact-panel__lookup-row">
      <button type="button" onClick={() => void invoke('create')}>Создать контрольную точку</button>
      <button type="button" onClick={() => void invoke('compare')}>Сравнить с точкой</button>
      <button type="button" onClick={() => void invoke('restore')}>Восстановить файлы проекта</button>
      <button type="button" onClick={() => void invoke('restore_task')}>Восстановить состояние задачи</button>
      <button type="button" onClick={() => void invoke('restore_both')}>Восстановить всё</button>
    </div>
    {projection ? <p role="status">{projection.state} · файлов: {projection.fileCount} · конфликтов: {projection.conflictCount} · {projection.snapshotHash}</p> : null}
    {message ? <p role="status">{message}</p> : null}
  </section>
}
