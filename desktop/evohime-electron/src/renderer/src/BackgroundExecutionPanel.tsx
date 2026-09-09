import type { BackgroundExecutionProjection, ConnectionState, CoreEvent, ShellEvent } from '@shared/api'
import { useEffect, useMemo, useState } from 'react'
import type { JSX } from 'react'
import { useShellApi } from './shell-api'

const OPERATIONS = ['list_runs', 'list_schedules', 'list_queues', 'upsert_queue', 'list_attempts', 'wake_due', 'dispatch_once', 'resume_run', 'set_schedule_enabled'] as const
type Operation = typeof OPERATIONS[number]

function latestProjection(events: readonly CoreEvent[]): BackgroundExecutionProjection | null {
  return events.find((event) => event.backgroundExecution !== null && event.backgroundExecution !== undefined)?.backgroundExecution ?? null
}

export function BackgroundExecutionPanel({ connection, events }: { readonly connection: ConnectionState; readonly events: readonly CoreEvent[] }): JSX.Element {
  const api = useShellApi()
  const [operation, setOperation] = useState<Operation>('list_runs')
  const [runId, setRunId] = useState('')
  const [payload, setPayload] = useState('')
  const [message, setMessage] = useState('')
  const [projection, setProjection] = useState<BackgroundExecutionProjection | null>(() => latestProjection(events))
  const eventProjection = useMemo(() => latestProjection(events), [events])

  useEffect(() => {
    if (eventProjection) setProjection(eventProjection)
  }, [eventProjection])

  useEffect(() => api?.subscribe((event: ShellEvent) => {
    if (event.kind === 'core-event' && event.event.backgroundExecution) setProjection(event.event.backgroundExecution)
  }), [api])

  const send = async (): Promise<void> => {
    if (!api || connection !== 'connected') {
      setMessage('Нет подключения к Core; состояние остаётся неизвестным.')
      return
    }
    const result = await api.invoke('core.backgroundExecution', {
      operation,
      runId: runId.trim(),
      ownerScope: 'local',
      payload: operation === 'set_schedule_enabled' && !payload.trim() ? '{"enabled":true}' : payload,
      expectedRevision: 0,
      ...(operation === 'dispatch_once' ? { idempotencyKey: `ui-${runId.trim()}` } : {}),
    })
    setMessage(result.ok ? 'Запрос принят Core; projection обновится событием.' : result.message)
  }

  const body = projection?.projection
  return <section className="panel" aria-label="Durable Background Execution">
    <h2>Durable Background Execution</h2>
    <p>Core-owned detached runs, schedules, waits, queues и immutable attempt history. Панель показывает только redacted metadata.</p>
    <div className="panel__actions">
      <label>Операция <select value={operation} onChange={(event) => setOperation(event.target.value as Operation)}>{OPERATIONS.map((item) => <option key={item} value={item}>{item}</option>)}</select></label>
      <label>Run ID <input value={runId} onChange={(event) => setRunId(event.target.value)} maxLength={128} placeholder="для операции run-level" /></label>
      <label>Bounded JSON <textarea value={payload} onChange={(event) => setPayload(event.target.value)} maxLength={64 * 1024} aria-label="Background execution JSON" /></label>
      <button type="button" onClick={() => void send()} disabled={!api || connection !== 'connected'}>Запросить</button>
    </div>
    <p role="status">Соединение: {connection} · Projection: {projection?.status ?? 'unknown'} · Ошибка: {projection?.errorCode || 'нет'}</p>
    {message ? <p role="status">{message}</p> : null}
    {body ? <pre aria-label="Background execution projection">{JSON.stringify(body, null, 2)}</pre> : <p>Данных ещё нет. При отключённом Core verdict не вычисляется в renderer.</p>}
  </section>
}
