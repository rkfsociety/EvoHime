import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import type { ConnectionState, CoreEvent, WorkspaceSelection } from '@shared/api'

import { useShellApi } from './shell-api'
import { ModelPicker } from './ModelPicker'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']
const MAX_RENDERED_ITEMS = 80
const MAX_PAYLOAD_CHARS = 8_192

type TimelineItem = {
  readonly event: CoreEvent
  readonly payload: Record<string, unknown>
}

type PendingApproval = {
  readonly approvalId: string
  readonly toolName: string
  readonly permission: string
  readonly scope: string
}

export interface TaskTimelineProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

export function TaskTimeline({ connection, events }: TaskTimelineProps): React.JSX.Element {
  const api = useShellApi()
  const [workspace, setWorkspace] = useState<WorkspaceSelection | null>(null)
  const [prompt, setPrompt] = useState('')
  const [taskId, setTaskId] = useState<string | null>(null)
  const [sentPrompt, setSentPrompt] = useState<string | null>(null)
  const [commandError, setCommandError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const bottomRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!api) return
    void api.invoke('workspace.list', {}).then((outcome) => {
      if (outcome.ok) setWorkspace(outcome.value)
    })
  }, [api])

  const taskEvents = useMemo(
    () =>
      events
        .filter((event) => event.taskId.length > 0 && (!taskId || event.taskId === taskId))
        .slice(0, MAX_RENDERED_ITEMS)
        .map((event) => ({ event, payload: parsePayload(event.payload) })),
    [events, taskId]
  )

  // Поток событий приходит от новых к старым, а читается сверху вниз.
  const stream = useMemo(() => [...taskEvents].reverse(), [taskEvents])

  const approval = useMemo(() => {
    const required = taskEvents.find((item) => item.event.eventType === 'approval.required')
    if (!required) return null
    const approvalId = stringField(required.payload, 'approval_id')
    if (!approvalId) return null
    return {
      approvalId,
      toolName: stringField(required.payload, 'tool_name') ?? 'операция Core',
      permission: stringField(required.payload, 'permission') ?? 'требуется разрешение',
      scope: stringField(required.payload, 'scope') ?? 'область не указана'
    } satisfies PendingApproval
  }, [taskEvents])

  useEffect(() => {
    // scrollIntoView отсутствует в jsdom, поэтому вызов защищён проверкой.
    const anchor = bottomRef.current
    if (typeof anchor?.scrollIntoView === 'function') {
      anchor.scrollIntoView({ block: 'end' })
    }
  }, [stream.length, approval])

  const start = useCallback(async () => {
    const selected = workspace?.selected
    if (!api || !selected || prompt.trim().length === 0) return
    const nextTaskId = makeTaskId()
    const text = prompt.trim()
    setBusy(true)
    setCommandError(null)
    const outcome = await api.invoke('core.startTask', {
      taskId: nextTaskId,
      prompt: text,
      workspacePath: selected
    })
    setBusy(false)
    if (!outcome.ok) {
      setCommandError(outcome.message)
      return
    }
    setTaskId(nextTaskId)
    setSentPrompt(text)
    setPrompt('')
  }, [api, prompt, workspace?.selected])

  const stop = useCallback(async () => {
    if (!api || !taskId) return
    setBusy(true)
    const outcome = await api.invoke('core.stopTask', { taskId })
    setBusy(false)
    if (!outcome.ok) setCommandError(outcome.message)
  }, [api, taskId])

  const resolveApproval = useCallback(
    async (granted: boolean) => {
      if (!api || !approval) return
      setBusy(true)
      const outcome = await api.invoke('core.resolveApproval', {
        approvalId: approval.approvalId,
        granted
      })
      setBusy(false)
      if (!outcome.ok) setCommandError(outcome.message)
    },
    [api, approval]
  )

  const connected = CONNECTED_STATES.includes(connection)
  const canStart = connected && workspace?.selected != null && prompt.trim().length > 0 && !busy
  const running = taskId !== null && !hasTerminalEvent(taskEvents)
  const empty = stream.length === 0 && sentPrompt === null

  return (
    <section className="chat" aria-label="Ход задачи">
      <div className="chat__scroll">
        {empty ? (
          <div className="chat__empty">
            <span className="chat__empty-logo" aria-hidden="true">E</span>
            <h2>Чем займёмся?</h2>
            <p>
              {workspace?.selected
                ? 'Опиши задачу — агент выполнит её в выбранной рабочей папке и покажет каждый шаг здесь.'
                : 'Сначала выбери рабочую папку в левой панели, затем поставь задачу агенту.'}
            </p>
          </div>
        ) : (
          <ol className="chat__stream">
            {sentPrompt ? (
              <li className="message message--user">
                <div className="message__bubble">{sentPrompt}</div>
              </li>
            ) : null}

            {stream.map(({ event, payload }) => (
              <li
                key={`${event.sequenceId}-${event.eventType}`}
                className={`message message--${role(event.eventType)} message--${tone(event.eventType)}`}
              >
                <div className="message__meta">
                  <span className="message__dot" aria-hidden="true" />
                  <span className="message__author">{labelFor(event.eventType)}</span>
                  <span className="message__sequence">#{event.sequenceId}</span>
                </div>
                <div className="message__bubble">{summaryFor(event, payload)}</div>
              </li>
            ))}

            {approval ? (
              <li className="approval" role="alert">
                <strong>Нужно разрешение: {approval.toolName}</strong>
                <span>{approval.permission} · {approval.scope}</span>
                <div>
                  <button type="button" onClick={() => void resolveApproval(true)} disabled={busy}>Разрешить</button>
                  <button type="button" onClick={() => void resolveApproval(false)} disabled={busy}>Отклонить</button>
                </div>
              </li>
            ) : null}
          </ol>
        )}
        <div ref={bottomRef} />
      </div>

      <div className="composer">
        <div className="composer__inner">
          <div className="composer__box">
            <label htmlFor="task-prompt" className="visually-hidden">Задача</label>
            <textarea
              id="task-prompt"
              value={prompt}
              onChange={(event) => setPrompt(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !event.shiftKey) {
                  event.preventDefault()
                  if (canStart) void start()
                }
              }}
              placeholder={
                workspace?.selected ? 'Опиши задачу для агента…' : 'Сначала выбери рабочую папку'
              }
              disabled={!connected || workspace?.selected === null || busy}
              rows={1}
            />
            <button
              type="button"
              className="composer__send"
              aria-label="Запустить задачу"
              onClick={() => void start()}
              disabled={!canStart}
            >
              ↑
            </button>
          </div>

          <p className="composer__hint">
            <ModelPicker connection={connection} events={events} />
            <span className="composer__keys">
              <kbd>Enter</kbd> отправить · <kbd>Shift</kbd>+<kbd>Enter</kbd> перенос строки
            </span>
            {running ? (
              <button type="button" onClick={() => void stop()} disabled={busy || !connected}>
                Остановить
              </button>
            ) : null}
          </p>


          {!connected ? (
            <p className="shell__reason">Core недоступен: запуск и управление задачей приостановлены.</p>
          ) : null}
          {workspace?.selected === null ? (
            <p className="shell__reason">Выбери рабочую папку перед запуском задачи.</p>
          ) : null}
          {commandError ? <p role="alert" className="shell__reason">{commandError}</p> : null}
        </div>
      </div>
    </section>
  )
}

function parsePayload(payload: string): Record<string, unknown> {
  if (payload.length === 0 || payload.length > MAX_PAYLOAD_CHARS) return {}
  try {
    const value: unknown = JSON.parse(payload)
    return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
  } catch {
    return {}
  }
}

function stringField(payload: Record<string, unknown>, key: string): string | null {
  const value = payload[key]
  return typeof value === 'string' && value.length > 0 ? value : null
}

function summaryFor(event: CoreEvent, payload: Record<string, unknown>): string {
  const value = stringField(payload, 'content') ?? stringField(payload, 'output') ?? stringField(payload, 'error') ?? stringField(payload, 'final_message')
  return value ? value.slice(0, MAX_PAYLOAD_CHARS) : event.eventType
}

function labelFor(eventType: string): string {
  return ({
    'task.started': 'Задача запущена',
    'agent.message.delta': 'Ответ агента',
    'tool.started': 'Инструмент запущен',
    'tool.output': 'Результат инструмента',
    'approval.required': 'Ожидается разрешение',
    'task.completed': 'Задача завершена',
    'task.failed': 'Задача завершилась ошибкой',
    'task.stopped': 'Задача остановлена'
  } satisfies Record<string, string>)[eventType] ?? eventType
}

/** Реплика агента читается как текст, всё остальное — как служебная запись. */
function role(eventType: string): string {
  if (eventType === 'agent.message.delta') return 'agent'
  if (eventType.startsWith('tool.')) return 'tool'
  return 'event'
}

function tone(eventType: string): string {
  if (eventType === 'task.completed') return 'success'
  if (eventType === 'task.failed') return 'error'
  if (eventType === 'approval.required') return 'approval'
  return 'default'
}

function hasTerminalEvent(items: readonly TimelineItem[]): boolean {
  return items.some(({ event }) => ['task.completed', 'task.failed', 'task.stopped'].includes(event.eventType))
}

function makeTaskId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  return `task-${Date.now()}-${Math.random().toString(16).slice(2)}`
}
