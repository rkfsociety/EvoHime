import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'

import type { ChatProviderMode, ChatRecord, ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'
import { ModelPicker } from './ModelPicker'
import { HomeScreen } from './HomeScreen'
import { RepositoryBar } from './RepositoryBar'
import { ActivityLine } from './ActivityLine'
import { buildTranscript } from './transcript'
import { MarkdownMessage } from './MarkdownMessage'
import { RecoveryBanner } from './RecoveryBanner'
import { PermissionModePicker } from './PermissionModePicker'
import { ContextUsage } from './ContextUsage'
import { RoutingStatus } from './RoutingStatus'
import { ChatProviderPicker } from './ChatProviderPicker'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']
const MAX_RENDERED_ITEMS = 80
const MAX_COMPOSER_HEIGHT_PX = 200
const MESSAGE_TIME_FORMATTER = new Intl.DateTimeFormat('ru-RU', {
  hour: '2-digit',
  minute: '2-digit'
})

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
}

export function TaskTimeline({
  connection,
  events,
  workspace,
  chatId,
  onChatTouched,
  onChatOpened,
  identityName,
  chatRevision
}: TaskTimelineProps): React.JSX.Element {
  const api = useShellApi()
  const [chat, setChat] = useState<ChatRecord | null>(null)
  const [prompt, setPrompt] = useState('')
  const [taskId, setTaskId] = useState<string | null>(null)
  const [sentPrompt, setSentPrompt] = useState<string | null>(null)
  const [sentPromptAtMs, setSentPromptAtMs] = useState<number | null>(null)
  const [commandError, setCommandError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [providerMode, setProviderMode] = useState<ChatProviderMode>(() => {
    const stored = window.localStorage.getItem('evohime.chat-provider-mode')
    return stored === 'codex_cli' || stored === 'openai_compatible' || stored === 'openai_responses' || stored === 'literouter'
      ? stored
      : 'literouter'
  })
  const [copiedMessageId, setCopiedMessageId] = useState<string | null>(null)
  const promptRef = useRef<HTMLTextAreaElement | null>(null)
  const bottomRef = useRef<HTMLDivElement | null>(null)
  const entryTimes = useRef(new Map<string, number>())

  useEffect(() => {
    // Chat-local transient state must not leak into another conversation.
    // Without this reset an empty chat still rendered the previous prompt and
    // task because both were kept outside the persisted ChatRecord.
    setChat(null)
    setTaskId(null)
    setSentPrompt(null)
    setSentPromptAtMs(null)
    setCommandError(null)

    if (!api || chatId === null) {
      return
    }

    let cancelled = false
    void api.invoke('chat.open', { chatId }).then((outcome) => {
      // A fast second click may complete before the first open request. Never
      // let an older response restore a previously selected chat.
      if (!cancelled && outcome.ok) setChat(outcome.value)
    })

    return () => {
      cancelled = true
    }
  }, [api, chatId])

  useLayoutEffect(() => {
    const textarea = promptRef.current
    if (!textarea) return
    // Reset first so deleting text shrinks the field as well as adding text
    // grows it. The CSS max-height remains the final safety limit.
    textarea.style.height = 'auto'
    const contentHeight = Math.max(textarea.scrollHeight, 24)
    textarea.style.height = `${Math.min(contentHeight, MAX_COMPOSER_HEIGHT_PX)}px`
    textarea.style.overflowY = contentHeight > MAX_COMPOSER_HEIGHT_PX ? 'auto' : 'hidden'
  }, [prompt])

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

  const conversation = useMemo(() => {
    const messages = [...(chat?.messages ?? [])]
    if (sentPrompt !== null && taskId !== null && !messages.some((message) => message.taskId === taskId)) {
      messages.push({ taskId, prompt: sentPrompt, atMs: sentPromptAtMs ?? Date.now() })
    }
    // The event list is shared by every message in the open chat. Index it
    // once so rendering a long conversation does not rescan the same events
    // for each message.
    const eventsByTask = new Map<string, CoreEvent[]>()
    for (const event of taskEvents) {
      const taskEventsForId = eventsByTask.get(event.taskId)
      if (taskEventsForId) taskEventsForId.push(event)
      else eventsByTask.set(event.taskId, [event])
    }
    return messages.map((message) => ({
      message,
      transcript: buildTranscript(eventsByTask.get(message.taskId) ?? [])
    }))
  }, [chat?.messages, sentPrompt, sentPromptAtMs, taskId, taskEvents])

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
      workspacePath: workspace,
      preferredRouteHint: providerMode === 'codex_cli' ? 'codex_cli' : 'cloud',
      executionKind: providerMode === 'codex_cli' ? 'coding' : 'dialogue'
    })
    setBusy(false)
    if (!outcome.ok) {
      setCommandError(outcome.message)
      return
    }
    setTaskId(nextTaskId)
    setSentPrompt(text)
    setSentPromptAtMs(Date.now())
    setPrompt('')
    const stored = await api.invoke('chat.appendPrompt', {
      chatId: targetChatId,
      taskId: nextTaskId,
      prompt: text
    })
    if (stored.ok && stored.value) setChat(stored.value)
    onChatTouched()
  }, [api, chatId, onChatOpened, onChatTouched, prompt, providerMode, workspace])

  const stop = useCallback(async () => {
    if (!api || !taskId) return
    setBusy(true)
    const outcome = await api.invoke('core.stopTask', { taskId })
    setBusy(false)
    if (!outcome.ok) setCommandError(outcome.message)
  }, [api, taskId])

  const resolveApproval = useCallback(
    async (granted: boolean, cancel = false) => {
      if (!api || !approval) return
      setBusy(true)
      const outcome = await api.invoke('core.resolveApproval', {
        approvalId: approval.approvalId,
        granted,
        idempotencyKey: `approval:${approval.approvalId}:${granted ? 'grant' : cancel ? 'cancel' : 'reject'}`,
        ...(granted ? {} : { rejectionReason: cancel ? 'cancelled_by_user' : 'rejected_by_user' }),
        cancel
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
      <RecoveryBanner
        connection={connection}
        events={taskEvents}
        onOpenTask={() => {}}
        showOpenTask={false}
      />
      <RoutingStatus events={taskEvents} connection={connection} />
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
            {conversation.flatMap(({ message, transcript }) => {
              const messageId = `user-${message.taskId}-${message.atMs}`
              return [
                <li key={messageId} className="message message--user">
                  <div className="message__bubble">{message.prompt}</div>
                  <MessageActions
                    id={messageId}
                    text={message.prompt}
                    atMs={message.atMs}
                    copied={copiedMessageId === messageId}
                    onCopy={setCopiedMessageId}
                  />
                </li>,
                ...transcript.entries.map((entry, index) =>
                  renderTranscriptEntry(entry, `${message.taskId}-${index}`, entryTimes, copiedMessageId, setCopiedMessageId)
                )
              ]
            })}

            {conversation.length === 0
              ? entries.map((entry, index) =>
                  renderTranscriptEntry(entry, String(index), entryTimes, copiedMessageId, setCopiedMessageId)
                )
              : null}

            {conversation.length > 0 && running && !approval && !conversation.at(-1)?.transcript.entries.some(
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
              <li className="approval task-timeline__approval" role="alert">
                <strong>Нужно разрешение: {approval.toolName}</strong>
                <span>{approval.permission} · {approval.scope}</span>
                <strong>{approval.preview.summary}</strong>
                {approval.preview.command ? <code>Команда: {approval.preview.command}</code> : null}
                {approval.preview.cwd ? <code>cwd: {approval.preview.cwd}</code> : null}
                {approval.preview.path ? <code>Файл: {approval.preview.path}</code> : null}
                {approval.preview.details ? <pre className="approval__details">{approval.preview.details}</pre> : null}
                {approval.preview.truncated ? <small>Preview ограничен по размеру.</small> : null}
                <div>
                  <button type="button" onClick={() => void resolveApproval(true)} disabled={busy}>Разрешить</button>
                  <button type="button" onClick={() => void resolveApproval(false)} disabled={busy}>Отклонить</button>
                  <button type="button" onClick={() => void resolveApproval(false, true)} disabled={busy}>Отменить</button>
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
          />
          <div className="composer__box">
            <label htmlFor="task-prompt" className="visually-hidden">Задача</label>
            <textarea
              id="task-prompt"
              ref={promptRef}
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
            <ContextUsage events={taskEvents} />
            <PermissionModePicker connection={connection} workspace={workspace} />
            <ChatProviderPicker
              connection={connection}
              value={providerMode}
              onChange={setProviderMode}
              disabled={busy}
            />
            <ModelPicker connection={connection} events={events} provider={providerMode} />
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

function renderTranscriptEntry(
  entry: ReturnType<typeof buildTranscript>['entries'][number],
  keySuffix: string,
  entryTimes: React.MutableRefObject<Map<string, number>>,
  copiedMessageId: string | null,
  onCopy: (id: string) => void
): React.JSX.Element {
  if (entry.kind === 'activity') {
    return (
      <li key={`${entry.kind}-${entry.id}-${keySuffix}`} className="message message--activity">
        <ActivityLine calls={entry.calls} running={entry.running} />
      </li>
    )
  }
  if (entry.kind === 'stopped') {
    return (
      <li key={`${entry.kind}-${entry.id}-${keySuffix}`} className="message message--note">
        <span className="message__note">Задача остановлена</span>
      </li>
    )
  }
  const messageId = `${entry.kind}-${entry.id}-${keySuffix}`
  return (
    <li
      key={messageId}
      className={`message message--agent${entry.kind === 'result' && entry.failed ? ' message--error' : ''}`}
    >
      <div className="message__bubble"><MarkdownMessage text={entry.text} /></div>
      <MessageActions
        id={messageId}
        text={entry.text}
        atMs={messageTime(entryTimes, messageId)}
        copied={copiedMessageId === messageId}
        onCopy={onCopy}
      />
    </li>
  )
}

function messageTime(times: React.MutableRefObject<Map<string, number>>, id: string): number {
  const existing = times.current.get(id)
  if (existing !== undefined) return existing
  const now = Date.now()
  times.current.set(id, now)
  return now
}

interface MessageActionsProps {
  readonly id: string
  readonly text: string
  readonly atMs: number | null
  readonly copied: boolean
  readonly onCopy: (id: string) => void
}

function MessageActions({ id, text, atMs, copied, onCopy }: MessageActionsProps): React.JSX.Element {
  const api = useShellApi()
  const copyResetTimer = useRef<number | null>(null)

  useEffect(() => {
    return () => {
      if (copyResetTimer.current === null) return
      window.clearTimeout(copyResetTimer.current)
      copyResetTimer.current = null
    }
  }, [])

  return (
    <div className="message__actions">
      <button
        type="button"
        className="message__copy"
        aria-label={copied ? 'Сообщение скопировано' : 'Скопировать сообщение'}
        title={copied ? 'Скопировано' : 'Скопировать'}
        onClick={() => {
          if (!api) return
          void api.writeClipboardText(text).then((ok) => {
            if (!ok) return
            onCopy(id)
            if (copyResetTimer.current !== null) window.clearTimeout(copyResetTimer.current)
            copyResetTimer.current = window.setTimeout(() => {
              copyResetTimer.current = null
              onCopy('')
            }, 1400)
          })
        }}
      >
        {copied ? '✓' : '▣'}
      </button>
      {atMs !== null ? <time dateTime={new Date(atMs).toISOString()}>{formatMessageTime(atMs)}</time> : null}
    </div>
  )
}

function formatMessageTime(atMs: number): string {
  return MESSAGE_TIME_FORMATTER.format(atMs)
}

function makeTaskId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  return `task-${Date.now()}-${Math.random().toString(16).slice(2)}`
}
