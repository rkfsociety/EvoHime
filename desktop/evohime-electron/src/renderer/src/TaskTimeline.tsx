import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'
import { ModelPicker } from './ModelPicker'
import { ActivityLine } from './ActivityLine'
import { buildTranscript } from './transcript'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']
const MAX_RENDERED_ITEMS = 80

export interface TaskTimelineProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  /**
   * Workspace owned by the shell. It arrives as a prop rather than being read
   * once on mount, so picking a folder in the sidebar unlocks the composer
   * immediately.
   */
  readonly workspace: string | null
}

export function TaskTimeline({
  connection,
  events,
  workspace
}: TaskTimelineProps): React.JSX.Element {
  const api = useShellApi()
  const [prompt, setPrompt] = useState('')
  const [taskId, setTaskId] = useState<string | null>(null)
  const [sentPrompt, setSentPrompt] = useState<string | null>(null)
  const [commandError, setCommandError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const bottomRef = useRef<HTMLDivElement | null>(null)

  const taskEvents = useMemo(
    () =>
      events
        .filter((event) => event.taskId.length > 0 && (!taskId || event.taskId === taskId))
        .slice(0, MAX_RENDERED_ITEMS),
    [events, taskId]
  )

  const { entries, approval, finished } = useMemo(
    () => buildTranscript(taskEvents),
    [taskEvents]
  )

  useEffect(() => {
    // scrollIntoView отсутствует в jsdom, поэтому вызов защищён проверкой.
    const anchor = bottomRef.current
    if (typeof anchor?.scrollIntoView === 'function') {
      anchor.scrollIntoView({ block: 'end' })
    }
  }, [entries.length, approval])

  const start = useCallback(async () => {
    if (!api || !workspace || prompt.trim().length === 0) return
    const nextTaskId = makeTaskId()
    const text = prompt.trim()
    setBusy(true)
    setCommandError(null)
    const outcome = await api.invoke('core.startTask', {
      taskId: nextTaskId,
      prompt: text,
      workspacePath: workspace
    })
    setBusy(false)
    if (!outcome.ok) {
      setCommandError(outcome.message)
      return
    }
    setTaskId(nextTaskId)
    setSentPrompt(text)
    setPrompt('')
  }, [api, prompt, workspace])

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
  const canStart = connected && workspace !== null && prompt.trim().length > 0 && !busy
  const running = taskId !== null && !finished
  // Запрос разрешения может прийти раньше любой другой записи ленты.
  const empty = entries.length === 0 && sentPrompt === null && approval === null

  return (
    <section className="chat" aria-label="Ход задачи">
      <div className="chat__scroll">
        {empty ? (
          <div className="chat__empty">
            <span className="chat__empty-logo" aria-hidden="true">E</span>
            <h2>Чем займёмся?</h2>
            <p>
              {workspace
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

            {entries.map((entry, index) => {
              if (entry.kind === 'activity') {
                return (
                  <li key={`${entry.kind}-${entry.id}-${index}`} className="message message--activity">
                    <ActivityLine calls={entry.calls} running={entry.running} />
                  </li>
                )
              }
              if (entry.kind === 'stopped') {
                return (
                  <li key={`${entry.kind}-${entry.id}-${index}`} className="message message--note">
                    <span className="message__note">Задача остановлена</span>
                  </li>
                )
              }
              if (entry.kind === 'result') {
                return (
                  <li
                    key={`${entry.kind}-${entry.id}-${index}`}
                    className={`message message--agent${entry.failed ? ' message--error' : ''}`}
                  >
                    <div className="message__bubble">{entry.text}</div>
                  </li>
                )
              }
              return (
                <li key={`${entry.kind}-${entry.id}-${index}`} className="message message--agent">
                  <div className="message__bubble">{entry.text}</div>
                </li>
              )
            })}

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
              placeholder={workspace ? 'Опиши задачу для агента…' : 'Сначала выбери рабочую папку'}
              disabled={!connected || workspace === null || busy}
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
          {workspace === null ? (
            <p className="shell__reason">Выбери рабочую папку перед запуском задачи.</p>
          ) : null}
          {commandError ? <p role="alert" className="shell__reason">{commandError}</p> : null}
        </div>
      </div>
    </section>
  )
}

function makeTaskId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  return `task-${Date.now()}-${Math.random().toString(16).slice(2)}`
}
