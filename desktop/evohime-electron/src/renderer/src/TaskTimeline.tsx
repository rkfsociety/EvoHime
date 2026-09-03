import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'

import type { ChatMessage, ChatProviderMode, ChatRecord, ConnectionState, CoreEvent } from '@shared/api'

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
import {
  addOptimisticMessage,
  applyConversationEvents,
  conversationEventsToCoreEvents,
  createConversationProjection,
  markOptimisticFailed,
  markOptimisticRetry,
  resumeAtRetainedBoundary,
  type ConversationProjectionState
} from './conversation-projection'

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
  const [startingTaskId, setStartingTaskId] = useState<string | null>(null)
  const [stopRequested, setStopRequested] = useState(false)
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
  const [conversationLog, setConversationLog] = useState<ConversationProjectionState | null>(null)
  const promptRef = useRef<HTMLTextAreaElement | null>(null)
  const bottomRef = useRef<HTMLDivElement | null>(null)
  const entryTimes = useRef(new Map<string, number>())
  const cancelRequestedTaskId = useRef<string | null>(null)

  useEffect(() => {
    // Chat-local transient state must not leak into another conversation.
    // Without this reset an empty chat still rendered the previous prompt and
    // task because both were kept outside the persisted ChatRecord.
    setChat(null)
    setTaskId(null)
    setStartingTaskId(null)
    setStopRequested(false)
    cancelRequestedTaskId.current = null
    setSentPrompt(null)
    setSentPromptAtMs(null)
    setCommandError(null)
    setConversationLog((current) => chatId === null
      ? null
      : current?.conversationId === chatId
        ? current
        : createConversationProjection(chatId))

    if (!api || chatId === null) {
      return
    }

    let cancelled = false
    void api.invoke('chat.open', { chatId }).then((outcome) => {
      // A fast second click may complete before the first open request. Never
      // let an older response restore a previously selected chat.
      if (!cancelled && outcome.ok) setChat(outcome.value)
    })
    void api.invoke('core.getConversationEvents', {
      conversationId: chatId,
      limit: 200
    })

    return () => {
      cancelled = true
    }
  }, [api, chatId])

  useEffect(() => {
    if (chatId === null) return
    const pageEnvelopes = events.filter((event) => event.conversationEventLog != null && event.conversationEventLog.conversationId === chatId)
    const newest = pageEnvelopes[0]
    const cacheKey = newest
      ? `${newest.coreInstanceId ?? 'legacy'}:${newest.sessionEpoch ?? 0}:${newest.conversationEventLog?.schemaVersion ?? 0}`
      : ''
    const pages = pageEnvelopes
      .filter((event) => `${event.coreInstanceId ?? 'legacy'}:${event.sessionEpoch ?? 0}:${event.conversationEventLog?.schemaVersion ?? 0}` === cacheKey)
      .map((event) => event.conversationEventLog!)
      .reverse()
    if (pages.length === 0) return
    let resumeAfter: number | null = null
    setConversationLog((current) => {
      let next = current !== null && current.conversationId === chatId && current.cacheKey === cacheKey
        ? current : createConversationProjection(chatId, cacheKey)
      for (const page of pages) {
        if (page.errorCode === 'cursor_expired') {
          next = { ...next, sync: { state: 'cursor-expired', earliestAvailableSequence: page.earliestAvailableSequence } }
          continue
        }
        if (page.errorCode === 'idempotency_conflict') {
          next = { ...next, sync: { state: 'conflict', sequence: next.lastSequence + 1 } }
          continue
        }
        if (page.errorCode.length > 0) {
          next = { ...next, optimistic: next.optimistic.map((message) => ({ ...message, status: 'failed' as const })) }
          continue
        }
        if (next.sync.state === 'cursor-expired' && page.events[0]?.sequence === page.earliestAvailableSequence) {
          next = resumeAtRetainedBoundary(next, page.earliestAvailableSequence)
        }
        if (next.lastSequence === 0 && page.oldestSequence > 1) {
          next = resumeAtRetainedBoundary(next, page.oldestSequence)
        }
        const previousSequence = next.lastSequence
        next = applyConversationEvents(next, page.events)
        if (next.sync.state === 'complete' && next.lastSequence > previousSequence) {
          resumeAfter = next.lastSequence
        }
      }
      if (resumeAfter === null) resumeAfter = next.lastSequence
      return next
    })
    if (resumeAfter !== null) {
      void api?.invoke('core.subscribeConversationEvents', {
        conversationId: chatId,
        afterSequence: resumeAfter,
        limit: 200
      })
    }
  }, [api, chatId, events])

  useEffect(() => {
    if (!api || chatId === null || !conversationLog) return
    const afterSequence = conversationLog.sync.state === 'gap'
      ? conversationLog.lastSequence
      : conversationLog.sync.state === 'cursor-expired'
        ? Math.max(0, conversationLog.sync.earliestAvailableSequence - 1)
        : null
    if (afterSequence === null) return
    void api.invoke('core.getConversationEvents', {
      conversationId: chatId,
      afterSequence,
      limit: 200
    })
  }, [api, chatId, conversationLog])

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
    if (conversationLog?.events.length) {
      return [...conversationEventsToCoreEvents(conversationLog.events)]
        .reverse()
        .slice(0, MAX_RENDERED_ITEMS)
    }
    return events
      .filter((event) => event.taskId.length > 0 && known.has(event.taskId))
      .slice(0, MAX_RENDERED_ITEMS)
  }, [chat?.taskIds, conversationLog?.events, events, taskId])

  const activeTaskEvents = useMemo(
    () => taskId === null ? taskEvents : taskEvents.filter((event) => event.taskId === taskId),
    [taskEvents, taskId]
  )

  // `taskId` is transient renderer state. After a reconnect or a renderer
  // reload Core may already be working while the component has not restored
  // that state yet. Recover the newest non-terminal task from Core events so
  // the stop control cannot disappear while work is still running.
  const inferredRunningTaskId = useMemo(() => {
    const terminal = new Set<string>()
    const activeEventTypes = new Set(['task.started', 'tool.started', 'agent.message.delta', 'approval.required'])
    for (const event of taskEvents) {
      if (event.taskId.length === 0) continue
      if (event.eventType === 'task.completed' || event.eventType === 'task.failed' || event.eventType === 'task.stopped') {
        terminal.add(event.taskId)
        continue
      }
      if (activeEventTypes.has(event.eventType) && !terminal.has(event.taskId)) return event.taskId
    }
    return null
  }, [taskEvents])

  const activeTaskId = taskId ?? startingTaskId ?? inferredRunningTaskId

  const { entries, approval, finished } = useMemo(
    () => buildTranscript(activeTaskId === null ? activeTaskEvents : activeTaskEvents.filter((event) => event.taskId === activeTaskId)),
    [activeTaskEvents, activeTaskId]
  )

  const conversation = useMemo(() => {
    const authoritativeMessages = (conversationLog?.events ?? [])
      .filter((event) => event.kind === 'user_message_accepted')
      .map((event): ChatMessage => ({
        taskId: event.taskId,
        clientMessageId: event.clientMessageId,
        prompt: payloadText(event.payload, 'content'),
        atMs: event.timestampMs
      }))
      .filter((message) => message.prompt.length > 0)
    const messages = [...(authoritativeMessages.length > 0 ? authoritativeMessages : chat?.messages ?? [])]
    for (const optimistic of conversationLog?.optimistic ?? []) {
      if (!messages.some((message) => message.clientMessageId === optimistic.clientMessageId)) {
        messages.push({
          taskId: optimistic.taskId,
          clientMessageId: optimistic.clientMessageId,
          prompt: optimistic.content,
          atMs: Date.now()
        })
      }
    }
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
      delivery: conversationLog?.optimistic.find(
        (item) => item.clientMessageId === message.clientMessageId
      ) ?? null,
      transcript: buildTranscript(eventsByTask.get(message.taskId) ?? [])
    }))
  }, [chat?.messages, conversationLog?.events, conversationLog?.optimistic, sentPrompt, sentPromptAtMs, taskId, taskEvents])

  const retryMessage = useCallback(async (clientMessageId: string) => {
    if (!api || !workspace || !conversationLog) return
    const message = conversationLog.optimistic.find((item) => item.clientMessageId === clientMessageId)
    if (!message) return
    setConversationLog((current) => current ? markOptimisticRetry(current, clientMessageId) : current)
    setCommandError(null)
    const outcome = await api.invoke('core.startTask', {
      taskId: message.taskId,
      prompt: message.content,
      workspacePath: workspace,
      conversationId: conversationLog.conversationId,
      clientMessageId,
      preferredRouteHint: providerMode === 'codex_cli' ? 'codex_cli' : 'cloud',
      executionKind: providerMode === 'codex_cli' ? 'coding' : 'dialogue'
    })
    if (!outcome.ok) {
      setConversationLog((current) => current ? markOptimisticFailed(current, clientMessageId) : current)
      setCommandError(outcome.message)
    }
  }, [api, conversationLog, providerMode, workspace])

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
    const clientMessageId = globalThis.crypto.randomUUID()
    const text = prompt.trim()
    cancelRequestedTaskId.current = null
    setTaskId(nextTaskId)
    setStartingTaskId(nextTaskId)
    setStopRequested(false)
    setBusy(true)
    setCommandError(null)

    // Typing is the intent to start a conversation, so the first prompt of a
    // session creates the chat instead of demanding it be made first.
    let targetChatId = chatId
    if (targetChatId === null) {
      const created = await api.invoke('chat.create', { workspacePath: workspace })
      if (!created.ok) {
        setTaskId(null)
        setStartingTaskId(null)
        setBusy(false)
        setCommandError(created.message)
        return
      }
      targetChatId = created.value.id
      setChat(created.value)
      onChatOpened(targetChatId)
    }

    setConversationLog((current) => addOptimisticMessage(
      current?.conversationId === targetChatId ? current : createConversationProjection(targetChatId),
      { clientMessageId, taskId: nextTaskId, content: text, status: 'sending' }
    ))

    const outcome = await api.invoke('core.startTask', {
      taskId: nextTaskId,
      prompt: text,
      workspacePath: workspace,
      conversationId: targetChatId,
      clientMessageId,
      preferredRouteHint: providerMode === 'codex_cli' ? 'codex_cli' : 'cloud',
      executionKind: providerMode === 'codex_cli' ? 'coding' : 'dialogue'
    })
    setBusy(false)
    if (!outcome.ok) {
      setTaskId(null)
      setStartingTaskId(null)
      setStopRequested(false)
      cancelRequestedTaskId.current = null
      setConversationLog((current) => current ? markOptimisticFailed(current, clientMessageId) : current)
      setCommandError(outcome.message)
      return
    }
    setStartingTaskId(null)
    setTaskId(nextTaskId)
    setSentPrompt(text)
    setSentPromptAtMs(Date.now())
    setPrompt('')
    const stored = await api.invoke('chat.appendPrompt', {
      chatId: targetChatId,
      taskId: nextTaskId,
      clientMessageId,
      prompt: text
    })
    if (stored.ok && stored.value) setChat(stored.value)
    onChatTouched()
    if (cancelRequestedTaskId.current === nextTaskId) {
      await api.invoke('core.stopTask', { taskId: nextTaskId })
    }
  }, [api, chatId, onChatOpened, onChatTouched, prompt, providerMode, workspace])

  const stop = useCallback(async () => {
    if (!api || !activeTaskId) return
    cancelRequestedTaskId.current = activeTaskId
    setStopRequested(true)
    setBusy(true)
    const outcome = await api.invoke('core.stopTask', { taskId: activeTaskId })
    setBusy(false)
    if (!outcome.ok) setCommandError(outcome.message)
  }, [activeTaskId, api])

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
  const running = activeTaskId !== null && (startingTaskId === activeTaskId || !finished)
  // Запрос разрешения может прийти раньше любой другой записи ленты.
  const empty =
    entries.length === 0 && sentPrompt === null && approval === null && conversation.length === 0

  return (
    <section className="chat" aria-label="Ход задачи">
      <RecoveryBanner
        connection={connection}
        events={taskEvents}
        onOpenTask={() => {}}
        showOpenTask={false}
      />
      <RoutingStatus events={taskEvents} connection={connection} />
      {conversationLog?.sync.state === 'gap' ? (
        <p role="alert" className="shell__reason">История неполна, восстанавливаю пропущенные события…</p>
      ) : null}
      {conversationLog?.sync.state === 'conflict' ? (
        <p role="alert" className="shell__reason">Обнаружен конфликт последовательности истории.</p>
      ) : null}
      {conversationLog?.sync.state === 'cursor-expired' ? (
        <p role="alert" className="shell__reason">Старая часть истории свёрнута; загружаю доступный диапазон…</p>
      ) : null}
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
            {conversation.flatMap(({ message, transcript, delivery }) => {
              const messageId = `user-${message.taskId}-${message.atMs}`
              return [
                <li key={messageId} className="message message--user">
                  <div className="message__bubble">{message.prompt}</div>
                  {delivery ? (
                    <small className="message__delivery" role="status">
                      {delivery.status === 'sending' ? 'Отправляется…' : null}
                      {delivery.status === 'retry' ? 'Повторная отправка…' : null}
                      {delivery.status === 'failed' ? (
                        <button type="button" onClick={() => void retryMessage(delivery.clientMessageId)}>
                          Повторить отправку
                        </button>
                      ) : null}
                    </small>
                  ) : null}
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
              aria-label={running ? (stopRequested ? 'Остановка задачи' : 'Остановить задачу') : 'Запустить задачу'}
              onClick={() => {
                if (running) void stop()
                else if (canStart) void start()
              }}
              disabled={running ? stopRequested || !connected : !canStart}
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
            <ModelPicker connection={connection} events={events} provider={providerMode} use="agent" />
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

function payloadText(payload: unknown, key: string): string {
  if (typeof payload !== 'object' || payload === null) return ''
  const value = (payload as Record<string, unknown>)[key]
  return typeof value === 'string' ? value : ''
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
