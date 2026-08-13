import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import type { ChatRecord, ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'
import { ModelPicker } from './ModelPicker'
import { HomeScreen } from './HomeScreen'
import { RepositoryBar } from './RepositoryBar'
import { ActivityLine } from './ActivityLine'
import { buildTranscript } from './transcript'
import { MarkdownMessage } from './MarkdownMessage'

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
  /** Open conversation; null means the user has not picked one yet. */
  readonly chatId: string | null
  /** Told when a prompt changed a chat, so the sidebar reloads its list. */
  readonly onChatTouched: () => void
  /** A chat created from the composer becomes the open one. */
  readonly onChatOpened: (chatId: string) => void
  readonly identityName: string | null
  readonly chatRevision: number
  /** Opens the Files and Git section from the project row. */
  readonly onOpenGit: () => void
}

export function TaskTimeline({
  connection,
  events,
  workspace,
  chatId,
  onChatTouched,
  onChatOpened,
  identityName,
  chatRevision,
  onOpenGit
}: TaskTimelineProps): React.JSX.Element {
  const api = useShellApi()
  const [chat, setChat] = useState<ChatRecord | null>(null)
  const [prompt, setPrompt] = useState('')
  const [taskId, setTaskId] = useState<string | null>(null)
  const [sentPrompt, setSentPrompt] = useState<string | null>(null)
  const [commandError, setCommandError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const bottomRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!api || chatId === null) {
      setChat(null)
      return
    }
    void api.invoke('chat.open', { chatId }).then((outcome) => {
      if (outcome.ok) setChat(outcome.value)
    })
  }, [api, chatId])

  // A chat shows only its own tasks; before the first prompt only the task
  // just started from here belongs to it.
  const taskEvents = useMemo(() => {
    const known = new Set(chat?.taskIds ?? [])
    if (taskId) known.add(taskId)
    return events
      .filter((event) => event.taskId.length > 0 && known.has(event.taskId))
      .slice(0, MAX_RENDERED_ITEMS)
  }, [chat?.taskIds, events, taskId])

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

    // Typing is the intent to start a conversation, so the first prompt of a
    // session creates the chat instead of demanding it be made first.
    let targetChatId = chatId
    if (targetChatId === null) {
      const created = await api.invoke('chat.create', { workspacePath: workspace })
      if (!created.ok) {
        setBusy(false)
        setCommandError(created.message)
        return
      }
      targetChatId = created.value.id
      setChat(created.value)
      onChatOpened(targetChatId)
    }

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
    const stored = await api.invoke('chat.appendPrompt', {
      chatId: targetChatId,
      taskId: nextTaskId,
      prompt: text
    })
    if (stored.ok && stored.value) setChat(stored.value)
    onChatTouched()
  }, [api, chatId, onChatOpened, onChatTouched, prompt, workspace])

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
  const history = chat?.messages ?? []
  // Запрос разрешения может прийти раньше любой другой записи ленты.
  const empty =
    entries.length === 0 && sentPrompt === null && approval === null && history.length === 0

  return (
    <section className="chat" aria-label="Ход задачи">
      <div className="chat__scroll">
        {empty ? (
          <HomeScreen
            workspace={workspace}
            identityName={identityName}
            onOpenChat={onChatOpened}
            onPickSuggestion={setPrompt}
            revision={chatRevision}
          />
        ) : (
          <ol className="chat__stream">
            {history.map((message) => (
              <li key={`${message.taskId}-${message.atMs}`} className="message message--user">
                <div className="message__bubble">{message.prompt}</div>
              </li>
            ))}
            {sentPrompt && !history.some((message) => message.prompt === sentPrompt) ? (
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
                    <div className="message__bubble"><MarkdownMessage text={entry.text} /></div>
                  </li>
                )
              }
              return (
                <li key={`${entry.kind}-${entry.id}-${index}`} className="message message--agent">
                  <div className="message__bubble"><MarkdownMessage text={entry.text} /></div>
                </li>
              )
            })}

            {running && !approval && !entries.some(
              (entry) => entry.kind === 'activity' && entry.running
            ) ? (
              <li className="message message--working" role="status" aria-label="Агент формирует ответ">
                <span className="working-indicator" aria-hidden="true">
                  <span />
                  <span />
                  <span />
                </span>
              </li>
            ) : null}

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

      {workspace === null ? null : (
      <div className="composer">
        <div className="composer__inner">
          <RepositoryBar
            workspace={workspace}
            refreshKey={finished ? entries.length : 0}
            onOpenGit={onOpenGit}
          />
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
              placeholder="Опиши задачу для агента…"
              disabled={!connected || busy}
              rows={1}
            />
            <button
              type="button"
              className={`composer__send${running ? ' composer__send--stop' : ''}`}
              aria-label={running ? 'Остановить задачу' : 'Запустить задачу'}
              onClick={() => {
                if (running) void stop()
                else if (canStart) void start()
              }}
              disabled={running ? busy || !connected : !canStart}
            >
              {running ? '■' : '↑'}
            </button>
          </div>

          <div className="composer__hint">
            <ModelPicker connection={connection} events={events} />
            <span className="composer__keys">
              <kbd>Enter</kbd> отправить · <kbd>Shift</kbd>+<kbd>Enter</kbd> перенос строки
            </span>
          </div>


          {!connected ? (
            <p className="shell__reason">Core недоступен: запуск и управление задачей приостановлены.</p>
          ) : null}

          {commandError ? <p role="alert" className="shell__reason">{commandError}</p> : null}
        </div>
      </div>
      )}
    </section>
  )
}

function makeTaskId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  return `task-${Date.now()}-${Math.random().toString(16).slice(2)}`
}
