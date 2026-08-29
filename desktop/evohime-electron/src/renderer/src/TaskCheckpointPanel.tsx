import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent, TaskCheckpointProjection } from '@shared/api'

import { useShellApi } from './shell-api'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

const STATUS_LABELS: Record<string, string> = {
  in_progress: 'В работе',
  paused: 'Пауза',
  waiting_approval: 'Ожидает подтверждения',
  resumable: 'Можно продолжить',
  blocked: 'Заблокирован',
  completed: 'Завершён',
  failed: 'Завершён с ошибкой',
  stale: 'Устарел',
  conflicted: 'Конфликт'
}

interface TaskCheckpointPanelProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly taskId: string | null
  readonly workspace: string | null
}

/**
 * Displays only the typed Core projection. It never reconstructs checkpoint
 * state from generic event payloads and never starts a task: explicit actions
 * go back through the authenticated main/Core path.
 */
export function TaskCheckpointPanel({
  connection,
  events,
  taskId,
  workspace
}: TaskCheckpointPanelProps): React.JSX.Element | null {
  const api = useShellApi()
  const [localProjection, setLocalProjection] = useState<TaskCheckpointProjection | null>(null)
  const [actionMessage, setActionMessage] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const projected = useMemo(
    () => events.find((event) => event.taskId === taskId && event.taskCheckpoint)?.taskCheckpoint ?? null,
    [events, taskId]
  )
  const actionResult = useMemo(
    () => events.find((event) => event.taskId === taskId && event.taskCheckpointAction)?.taskCheckpointAction ?? null,
    [events, taskId]
  )
  const projection = projected ?? localProjection

  useEffect(() => {
    setLocalProjection(null)
    setActionMessage(null)
  }, [taskId, workspace])

  useEffect(() => {
    if (projected) setLocalProjection(projected)
  }, [projected])

  useEffect(() => {
    if (!api || !taskId || !workspace || !CONNECTED_STATES.includes(connection)) return
    void api.invoke('core.getTaskCheckpoint', {
      taskId,
      workspacePath: workspace,
      maxReplayEvents: 64
    })
  }, [api, connection, taskId, workspace])

  if (
    !taskId ||
    !projection ||
    (projection.recoveryDisposition === 'no_checkpoint' && !projection.errorCode)
  ) {
    return null
  }

  const requestAction = async (action: 'acknowledge_recovery' | 'request_resume'): Promise<void> => {
    if (!api || !CONNECTED_STATES.includes(connection)) {
      setActionMessage('Core не подключён: действие checkpoint невозможно.')
      return
    }
    setBusy(true)
    setActionMessage(null)
    const outcome = await api.invoke('core.resolveTaskCheckpoint', {
      taskId: projection.taskId,
      workspacePath: workspace ?? '',
      checkpointId: projection.checkpointId,
      expectedSourceEventSeq: projection.sourceEventSeq,
      action,
      idempotencyKey: makeIdempotencyKey(action, projection.checkpointId)
    })
    setBusy(false)
    if (!outcome.ok) setActionMessage(outcome.message)
    else setActionMessage('Действие отправлено в Core; жду typed-результат.')
  }

  const disposition = projection.recoveryDisposition
  const warning = projection.recoveryWarning || (projection.errorCode ? 'Проекция checkpoint требует внимания.' : '')
  const showAcknowledge = disposition === 'blocked' || disposition === 'terminal' || Boolean(projection.errorCode)

  return (
    <section className={`task-checkpoint task-checkpoint--${disposition}`} aria-label="Checkpoint задачи">
      <div className="task-checkpoint__heading">
        <div>
          <span className="task-checkpoint__eyebrow">Core checkpoint</span>
          <h3>{(STATUS_LABELS[projection.status] ?? projection.status) || 'Состояние задачи'}</h3>
        </div>
        <code>seq {projection.sourceEventSeq}</code>
      </div>

      <dl className="task-checkpoint__summary">
        <div><dt>Прогресс</dt><dd>{projection.completedCount} завершено · {projection.remainingCount} осталось</dd></div>
        <div><dt>Recovery</dt><dd>{disposition}</dd></div>
        <div><dt>Checkpoint</dt><dd>{projection.checkpointId}</dd></div>
      </dl>

      {warning ? <p className="task-checkpoint__warning" role="alert">{warning}</p> : null}
      {projection.blockers.length > 0 ? (
        <div className="task-checkpoint__group">
          <strong>Блокеры</strong>
          <ul>{projection.blockers.map((blocker, index) => <li key={`${blocker}-${index}`}>{blocker}</li>)}</ul>
        </div>
      ) : null}
      {projection.refs.length > 0 ? (
        <details className="task-checkpoint__refs">
          <summary>Ссылки и policy ({projection.refs.length})</summary>
          <ul>
            {projection.refs.map((reference) => (
              <li key={`${reference.kind}-${reference.id}`}>
                <span>{reference.kind}</span>
                <code>{reference.id}</code>
                {reference.contentHash ? <small>{reference.contentHash}</small> : null}
              </li>
            ))}
          </ul>
        </details>
      ) : null}
      {projection.replayedEventCount > 0 ? (
        <small className="task-checkpoint__replay">
          Replay metadata: {projection.replayedEventCount} событий · {projection.replayedEventTypes.join(', ')}
        </small>
      ) : null}

      <div className="task-checkpoint__actions">
        {projection.canRequestResume ? (
          <button type="button" onClick={() => void requestAction('request_resume')} disabled={busy}>
            Запросить reconciliation
          </button>
        ) : null}
        {showAcknowledge ? (
          <button type="button" onClick={() => void requestAction('acknowledge_recovery')} disabled={busy}>
            Подтвердить состояние
          </button>
        ) : null}
      </div>
      {actionMessage ? <p className="task-checkpoint__result" role="status">{actionMessage}</p> : null}
      {actionResult ? (
        <p className={`task-checkpoint__result${actionResult.applied ? '' : ' task-checkpoint__result--error'}`} role="status">
          {actionResult.errorMessage || (actionResult.applied ? 'Действие применено Core.' : 'Core отклонил действие.')}
          {actionResult.deduplicated ? ' Повтор запроса безопасно дедуплицирован.' : ''}
        </p>
      ) : null}
    </section>
  )
}

function makeIdempotencyKey(action: string, checkpointId: string): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return `checkpoint:${action}:${checkpointId}:${crypto.randomUUID()}`
  }
  return `checkpoint:${action}:${checkpointId}:${Date.now()}`
}
