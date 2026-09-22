import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'

import type { ChatMessage, ChatProviderMode, ChatRecord, ConnectionState, ConversationEventProjection, CoreEvent, WorkspaceOption } from '@shared/api'

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
  applyInitialConversationHistory,
  applyConversationEvents,
  conversationEvents,
  conversationEventsToCoreEvents,
  createConversationProjection,
  markOptimisticFailed,
  markOptimisticRetry,
  prependConversationEvents,
  resumeAtRetainedBoundary,
  type ConversationProjectionState
} from './conversation-projection'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']
const MAX_RENDERED_ITEMS = 80
const TIMELINE_WINDOW_OVERSCAN = 16
const TIMELINE_ITEM_HEIGHT_ESTIMATE_PX = 72
const TIMELINE_BOTTOM_THRESHOLD_PX = 48
const MAX_COMPOSER_HEIGHT_PX = 200
const MESSAGE_TIME_FORMATTER = new Intl.DateTimeFormat('ru-RU', {
  hour: '2-digit',
  minute: '2-digit'
})

interface ComposerAttachment {
  readonly name: string
  readonly relativePath: string
}

function buildConversationPageKey(page: { conversationId: string; oldestSequence: number; earliestAvailableSequence?: number; errorCode?: string; events: readonly { eventId: string; sequence: number }[] }): string {
  const signature = page.events.map((entry) => `${entry.sequence}:${entry.eventId}`).join('|')
  return `${page.conversationId}:${page.oldestSequence}:${page.earliestAvailableSequence ?? 0}:${page.errorCode ?? ''}:${signature}`
}

interface ConversationEventCursor {
  readonly pages: Set<string>
  readonly eventsById: Map<string, ConversationEventProjection>
}

function createConversationEventCursor(): ConversationEventCursor {
  return { pages: new Set<string>(), eventsById: new Map<string, ConversationEventProjection>() }
}

function buildCoreEventKey(event: CoreEvent): string {
  const instance = `${event.coreInstanceId ?? 'legacy'}:${event.sessionEpoch ?? 0}`
  if (event.conversationEventLog) {
    return `conversation:${instance}:${JSON.stringify(event.conversationEventLog)}`
  }
  return `core:${instance}:${event.sequenceId}:${event.taskId}:${event.eventType}:${event.payload}`
}

function conversationEventIndexKey(event: ConversationEventProjection): string {
  return event.eventId.length > 0 ? event.eventId : `${event.sequence}:${event.kind}:${event.taskId}`
}

function sameConversationEvent(left: ConversationEventProjection, right: ConversationEventProjection): boolean {
  return JSON.stringify(left) === JSON.stringify(right)
}

export interface TaskTimelineProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  /**
   * Workspace owned by the shell. It arrives as a prop rather than being read
   * once on mount, so picking a folder in the sidebar unlocks the composer
   * immediately.
   */
  readonly workspace: string | null
  /** Project selection belongs to the composer, not the chat rail. */
  readonly onWorkspaceChange?: (workspace: string | null) => void
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
  onWorkspaceChange,
  chatId,
  onChatTouched,
  onChatOpened,
  identityName,
  chatRevision
}: TaskTimelineProps): React.JSX.Element {
  const api = useShellApi()
  const [chat, setChat] = useState<ChatRecord | null>(null)
  const [prompt, setPrompt] = useState('')
  const [attachments, setAttachments] = useState<readonly ComposerAttachment[]>([])
  const [webSearchEnabled, setWebSearchEnabled] = useState(false)
  const [toolsMenuOpen, setToolsMenuOpen] = useState(false)
  const [composerNotice, setComposerNotice] = useState<string | null>(null)
  const [taskId, setTaskId] = useState<string | null>(null)
  const [startingTaskId, setStartingTaskId] = useState<string | null>(null)
  const [stopRequested, setStopRequested] = useState(false)
  const [sentPrompt, setSentPrompt] = useState<string | null>(null)
  const [sentPromptAtMs, setSentPromptAtMs] = useState<number | null>(null)
  const [commandError, setCommandError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [providerMode, setProviderMode] = useState<ChatProviderMode>(() => {
    const stored = window.localStorage.getItem('evohime.chat-provider-mode')
    return stored === 'codex_cli' || stored === 'openai_compatible' || stored === 'openai_responses' || stored === 'literouter' || stored === 'ollama'
      ? stored
      : 'literouter'
  })
  const [copiedMessageId, setCopiedMessageId] = useState<string | null>(null)
  const [projects, setProjects] = useState<readonly WorkspaceOption[]>([])
  const [conversationLog, setConversationLog] = useState<ConversationProjectionState | null>(null)
  const [loadingOlderHistory, setLoadingOlderHistory] = useState(false)
  const promptRef = useRef<HTMLTextAreaElement | null>(null)
  const fileInputRef = useRef<HTMLInputElement | null>(null)
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const [timelineWindowStart, setTimelineWindowStart] = useState(0)
  const followLiveRef = useRef(true)
  const previousTimelineRef = useRef<{ firstKey: string | null; length: number; scrollHeight: number }>({
    firstKey: null,
    length: 0,
    scrollHeight: 0
  })
  const entryTimes = useRef(new Map<string, number>())
  const cancelRequestedTaskId = useRef<string | null>(null)
  const eventCursorRef = useRef<{ seenKeys: Set<string>; firstKey: string | null; length: number }>({
    seenKeys: new Set<string>(),
    firstKey: null,
    length: 0
  })
  const conversationEventCursorRef = useRef(new Map<string, ConversationEventCursor>())
  const subscriptionCursorRef = useRef<string | null>(null)
  const previousConnectionRef = useRef<ConnectionState>(connection)
  const conversationLogRef = useRef<ConversationProjectionState | null>(null)

  const setConversationProjection = useCallback((updater: (current: ConversationProjectionState | null) => ConversationProjectionState | null) => {
    setConversationLog((current) => {
      const next = updater(current)
      conversationLogRef.current = next
      return next
    })
  }, [])

  useEffect(() => {
    if (!api) return
    void api.invoke('workspace.list', {}).then((outcome) => {
      if (outcome.ok && !Array.isArray(outcome.value)) setProjects(outcome.value.options)
    })
  }, [api])

  const changeWorkspace = useCallback(async (path: string) => {
    if (!api || !onWorkspaceChange) return
    if (path.length === 0) {
      onWorkspaceChange(null)
      return
    }
    const outcome = await api.invoke('workspace.select', { path })
    if (outcome.ok && !Array.isArray(outcome.value)) {
      setProjects(outcome.value.options)
      onWorkspaceChange(outcome.value.selected)
    }
  }, [api, onWorkspaceChange])

  const pickWorkspace = useCallback(async () => {
    if (!api || !onWorkspaceChange) return
    const outcome = await api.invoke('workspace.pick', {})
    if (outcome.ok && !outcome.value.cancelled) {
      setProjects(outcome.value.selection.options)
      onWorkspaceChange(outcome.value.selection.selected)
    }
  }, [api, onWorkspaceChange])

  useEffect(() => {
    // Chat-local transient state must not leak into another conversation.
    // Without this reset an empty chat still rendered the previous prompt and
    // task because both were kept outside the persisted ChatRecord.
    setChat(null)
    setAttachments([])
    setWebSearchEnabled(false)
    setToolsMenuOpen(false)
    setComposerNotice(null)
    setTaskId(null)
    setStartingTaskId(null)
    setStopRequested(false)
    cancelRequestedTaskId.current = null
    setSentPrompt(null)
    setSentPromptAtMs(null)
    setCommandError(null)
    setTimelineWindowStart(0)
    followLiveRef.current = true
    previousTimelineRef.current = { firstKey: null, length: 0, scrollHeight: 0 }
    eventCursorRef.current.seenKeys.clear()
    eventCursorRef.current.firstKey = null
    eventCursorRef.current.length = 0
    conversationEventCursorRef.current.clear()
    subscriptionCursorRef.current = null
    const nextConversationLog = chatId === null
      ? null
      : conversationLogRef.current?.conversationId === chatId
        ? conversationLogRef.current
        : createConversationProjection(chatId)
    conversationLogRef.current = nextConversationLog
    setConversationLog(nextConversationLog)

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
    const previous = previousConnectionRef.current
    const resumed = !CONNECTED_STATES.includes(previous) && CONNECTED_STATES.includes(connection)
    previousConnectionRef.current = connection
    if (!resumed) return
    eventCursorRef.current.seenKeys.clear()
    eventCursorRef.current.firstKey = null
    eventCursorRef.current.length = 0
    conversationEventCursorRef.current.clear()
    subscriptionCursorRef.current = null
  }, [connection])

  useEffect(() => {
    if (chatId === null) return

    const cursor = conversationEventCursorRef.current.get(chatId) ?? createConversationEventCursor()
    conversationEventCursorRef.current.set(chatId, cursor)
    const newEvents: CoreEvent[] = []
    const eventCursor = eventCursorRef.current
    const firstKey = events[0] ? buildCoreEventKey(events[0]) : null
    if (eventCursor.seenKeys.size === 0) {
      newEvents.push(...events)
    } else if (firstKey === eventCursor.firstKey && events.length > eventCursor.length) {
      // The test/replay path can append events while the live App prepends
      // them. Both paths have a stable old boundary, so only inspect the new
      // suffix here.
      newEvents.push(...events.slice(eventCursor.length))
    } else if (firstKey !== eventCursor.firstKey) {
      // App prepends new events. Stop at the first known boundary instead of
      // walking the retained global history.
      for (const event of events) {
        const key = buildCoreEventKey(event)
        if (eventCursor.seenKeys.has(key)) break
        newEvents.push(event)
      }
    }
    for (const event of newEvents) {
      eventCursor.seenKeys.add(buildCoreEventKey(event))
    }
    eventCursor.firstKey = firstKey
    eventCursor.length = events.length

    const pageEnvelopes: Array<{ event: CoreEvent; page: NonNullable<CoreEvent['conversationEventLog']> }> = []
    for (const event of newEvents) {
      const page = event.conversationEventLog
      if (page == null || page.conversationId !== chatId) continue
      const pageKey = buildConversationPageKey(page)
      const isNewPage = !cursor.pages.has(pageKey)
      const newPageEvents = page.events.filter((entry) => {
        const indexKey = conversationEventIndexKey(entry)
        const previous = cursor.eventsById.get(indexKey)
        if (previous && sameConversationEvent(previous, entry)) return false
        cursor.eventsById.set(indexKey, entry)
        return true
      })
      cursor.pages.add(pageKey)
      if (!isNewPage && newPageEvents.length === 0) continue
      if (isNewPage && page.events.length > 0 && newPageEvents.length === 0) continue
      pageEnvelopes.push({
        event,
        page: newPageEvents.length === page.events.length ? page : { ...page, events: newPageEvents }
      })
    }
    if (pageEnvelopes.length === 0) return

    const newest = pageEnvelopes[0]
    const cacheKey = newest
      ? `${newest.event.coreInstanceId ?? 'legacy'}:${newest.event.sessionEpoch ?? 0}:${newest.page.schemaVersion ?? 0}`
      : ''
    const pages = pageEnvelopes
      .filter(({ event, page }) => `${event.coreInstanceId ?? 'legacy'}:${event.sessionEpoch ?? 0}:${page.schemaVersion ?? 0}` === cacheKey)
      .map(({ page }) => page)
      .reverse()
    let next = conversationLogRef.current !== null && conversationLogRef.current.conversationId === chatId && conversationLogRef.current.cacheKey === cacheKey
      ? conversationLogRef.current
      : createConversationProjection(chatId, cacheKey)
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
      const knownEvents = conversationEvents(next)
      const isOlderPage = knownEvents.length > 0 && page.events.length > 0
        && page.events.every((event) => event.sequence < (knownEvents[0]?.sequence ?? Number.MAX_SAFE_INTEGER))
      if (isOlderPage) {
        next = prependConversationEvents(next, page.events)
      } else if (page.operation === 'live' || page.operation === 'subscribed') {
        next = applyConversationEvents(next, page.events)
      } else if (next.historyEvents.length === 0 && next.liveEvents.length === 0) {
        next = applyInitialConversationHistory(next, page.events)
      } else {
        next = applyConversationEvents(next, page.events)
      }
    }
    conversationLogRef.current = next
    setConversationLog(next)
  }, [chatId, connection, events])

  useEffect(() => {
    if (chatId === null || !api || !CONNECTED_STATES.includes(connection)) return
    const current = conversationLogRef.current
    const afterSequence = current?.sync.state === 'gap'
      ? current.lastSequence
      : current?.sync.state === 'cursor-expired'
        ? Math.max(0, current.sync.earliestAvailableSequence - 1)
        : current?.lastSequence ?? 0
    const subscriptionKey = `${chatId}:${connection}`
    if (subscriptionCursorRef.current === subscriptionKey) return
    subscriptionCursorRef.current = subscriptionKey
    void api.invoke('core.subscribeConversationEvents', {
      conversationId: chatId,
      afterSequence,
      limit: 200
    })
  }, [api, chatId, connection])

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

  const loadOlderHistory = useCallback(() => {
    if (!api || chatId === null || !conversationLog || loadingOlderHistory) return
    const beforeSequence = conversationEvents(conversationLog)[0]?.sequence
    if (beforeSequence === undefined || beforeSequence <= 1) return
    setLoadingOlderHistory(true)
    void api.invoke('core.getConversationEvents', {
      conversationId: chatId,
      beforeSequence,
      limit: 200
    }).finally(() => setLoadingOlderHistory(false))
  }, [api, chatId, conversationLog, loadingOlderHistory])

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
  const projectedConversationEvents = useMemo(
    () => conversationLog === null ? [] : conversationEvents(conversationLog),
    [conversationLog?.historyEvents, conversationLog?.liveEvents]
  )

  const taskEvents = useMemo(() => {
    const known = new Set(chat?.taskIds ?? [])
    if (taskId) known.add(taskId)
    if (projectedConversationEvents.length) {
      return [...conversationEventsToCoreEvents(projectedConversationEvents)]
        .reverse()
    }
    return events
      .filter((event) => event.taskId.length > 0 && known.has(event.taskId))
  }, [chat?.taskIds, events, projectedConversationEvents, taskId])

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
    const authoritativeMessages = projectedConversationEvents
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
  }, [chat?.messages, conversationLog?.optimistic, projectedConversationEvents, sentPrompt, sentPromptAtMs, taskId, taskEvents])

  const retryMessage = useCallback(async (clientMessageId: string) => {
    if (!api || !conversationLog) return
    const message = conversationLog.optimistic.find((item) => item.clientMessageId === clientMessageId)
    if (!message) return
    setConversationProjection((current) => current ? markOptimisticRetry(current, clientMessageId) : current)
    setCommandError(null)
    const outcome = await api.invoke('core.startTask', {
      taskId: message.taskId,
      prompt: message.content,
      workspacePath: workspace ?? '',
      conversationId: conversationLog.conversationId,
      clientMessageId,
      preferredRouteHint: providerMode === 'codex_cli' ? 'codex_cli' : 'cloud',
      executionKind: providerMode === 'codex_cli' ? 'coding' : 'dialogue'
    })
    if (!outcome.ok) {
      setConversationProjection((current) => current ? markOptimisticFailed(current, clientMessageId) : current)
      setCommandError(outcome.message)
    }
  }, [api, conversationLog, providerMode, setConversationProjection, workspace])

  const timelineItems = useMemo(() => {
    if (conversation.length > 0) {
      return conversation.flatMap(({ message, transcript, delivery }) => {
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
      })
    }
    return entries.map((entry, index) =>
      renderTranscriptEntry(entry, String(index), entryTimes, copiedMessageId, setCopiedMessageId)
    )
  }, [conversation, copiedMessageId, entries, retryMessage])

  const timelineItemKeys = useMemo(
    () => timelineItems.map((item) => item.key === null ? '' : String(item.key)),
    [timelineItems]
  )
  const maxTimelineWindowStart = Math.max(0, timelineItems.length - MAX_RENDERED_ITEMS)
  const renderedTimelineStart = Math.min(timelineWindowStart, maxTimelineWindowStart)
  const renderedTimelineItems = timelineItems.slice(renderedTimelineStart, renderedTimelineStart + MAX_RENDERED_ITEMS)

  const handleTimelineScroll = useCallback((event: React.UIEvent<HTMLDivElement>) => {
    const element = event.currentTarget
    const maxStart = Math.max(0, timelineItems.length - MAX_RENDERED_ITEMS)
    const estimatedFirstVisible = Math.floor(element.scrollTop / TIMELINE_ITEM_HEIGHT_ESTIMATE_PX)
    setTimelineWindowStart(Math.min(
      maxStart,
      Math.max(0, estimatedFirstVisible - TIMELINE_WINDOW_OVERSCAN)
    ))
    followLiveRef.current = element.scrollHeight - element.scrollTop - element.clientHeight <= TIMELINE_BOTTOM_THRESHOLD_PX
  }, [timelineItems.length])

  useLayoutEffect(() => {
    const element = scrollRef.current
    const firstKey = timelineItemKeys[0] ?? null
    const previous = previousTimelineRef.current
    if (element && timelineItems.length > 0) {
      const previousFirstIndex = previous.firstKey === null ? -1 : timelineItemKeys.indexOf(previous.firstKey)
      const prependedCount = previous.firstKey !== null && previousFirstIndex >= 0
        ? previousFirstIndex
        : previous.firstKey !== null ? Math.max(0, timelineItems.length - previous.length) : 0
      if (prependedCount > 0) {
        setTimelineWindowStart((start) => Math.min(maxTimelineWindowStart, start + prependedCount))
        element.scrollTop += Math.max(0, element.scrollHeight - previous.scrollHeight)
      } else if (previous.length === 0 || followLiveRef.current) {
        setTimelineWindowStart(maxTimelineWindowStart)
        element.scrollTop = element.scrollHeight
      }
      previousTimelineRef.current = { firstKey, length: timelineItems.length, scrollHeight: element.scrollHeight }
    } else {
      previousTimelineRef.current = { firstKey, length: timelineItems.length, scrollHeight: element?.scrollHeight ?? 0 }
    }
  }, [maxTimelineWindowStart, timelineItemKeys, timelineItems.length])

  const start = useCallback(async () => {
    if (!api || prompt.trim().length === 0) return
    const nextTaskId = makeTaskId()
    const clientMessageId = globalThis.crypto.randomUUID()
    const text = composeTaskPrompt(prompt.trim(), attachments, webSearchEnabled)
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

    setConversationProjection((current) => addOptimisticMessage(
      current?.conversationId === targetChatId ? current : createConversationProjection(targetChatId),
      { clientMessageId, taskId: nextTaskId, content: text, status: 'sending' }
    ))

    const outcome = await api.invoke('core.startTask', {
      taskId: nextTaskId,
      prompt: text,
      workspacePath: workspace ?? '',
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
      setConversationProjection((current) => current ? markOptimisticFailed(current, clientMessageId) : current)
      setCommandError(outcome.message)
      return
    }
    setStartingTaskId(null)
    setTaskId(nextTaskId)
    setSentPrompt(text)
    setSentPromptAtMs(Date.now())
    setPrompt('')
    setAttachments([])
    setWebSearchEnabled(false)
    setComposerNotice(null)
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
  }, [api, attachments, chatId, onChatOpened, onChatTouched, prompt, providerMode, setConversationProjection, webSearchEnabled, workspace])

  const selectFiles = useCallback((fileList: FileList | null) => {
    if (!fileList || fileList.length === 0) return
    if (!workspace) {
      setComposerNotice('Сначала выбери проект, чтобы прикрепить файл.')
      return
    }
    const selected: ComposerAttachment[] = []
    let rejected = false
    for (const file of Array.from(fileList)) {
      const absolutePath = api?.pathForFile?.(file) ?? ''
      const relativePath = absolutePath.length > 0 ? relativeWorkspacePath(workspace, absolutePath) : null
      if (!relativePath) {
        rejected = true
        continue
      }
      if (!selected.some((item) => item.relativePath.toLowerCase() === relativePath.toLowerCase())) {
        selected.push({ name: file.name, relativePath })
      }
    }
    if (selected.length > 0) {
      setAttachments((current) => {
        const next = [...current]
        for (const item of selected) {
          if (!next.some((existing) => existing.relativePath.toLowerCase() === item.relativePath.toLowerCase())) next.push(item)
        }
        return next
      })
    }
    setComposerNotice(rejected ? 'Можно прикрепить только файлы из выбранного проекта.' : null)
  }, [api, workspace])

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
  const canStart = connected && prompt.trim().length > 0 && !busy
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
      <div ref={scrollRef} className="chat__scroll" onScroll={handleTimelineScroll}>
        {empty ? (
          <HomeScreen
            workspace={workspace}
            identityName={identityName}
            onOpenChat={onChatOpened}
            onPickSuggestion={setPrompt}
            revision={chatRevision}
          />
        ) : (
          <>
            {conversationLog && projectedConversationEvents[0]?.sequence && projectedConversationEvents[0].sequence > 1 ? (
              <div className="chat__history-controls">
                <button type="button" onClick={loadOlderHistory} disabled={loadingOlderHistory}>
                  {loadingOlderHistory ? 'Загружаю историю…' : 'Загрузить более старую историю'}
                </button>
              </div>
            ) : null}
            <ol className="chat__stream">
              {renderedTimelineStart > 0 ? (
                <li className="chat__window-spacer" aria-hidden="true" style={{ height: `${renderedTimelineStart * TIMELINE_ITEM_HEIGHT_ESTIMATE_PX}px` }} />
              ) : null}
              {renderedTimelineItems}
              {renderedTimelineStart + renderedTimelineItems.length < timelineItems.length ? (
                <li className="chat__window-spacer" aria-hidden="true" style={{ height: `${(timelineItems.length - renderedTimelineStart - renderedTimelineItems.length) * TIMELINE_ITEM_HEIGHT_ESTIMATE_PX}px` }} />
              ) : null}

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
          </>
        )}
      </div>

      <div className="composer">
        <div className="composer__inner">
          <div className="composer__box">
            <div className="composer__prompt-row">
              <ComposerIcon name="link" className="composer__prompt-icon" />
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
                {running ? <span aria-hidden="true">■</span> : <ComposerIcon name="send" />}
              </button>
            </div>
            <div className="composer__input-actions">
              <input
                ref={fileInputRef}
                className="visually-hidden"
                type="file"
                multiple
                tabIndex={-1}
                aria-hidden="true"
                onChange={(event) => {
                  selectFiles(event.target.files)
                  event.target.value = ''
                }}
              />
              <button
                type="button"
                className="composer__action"
                disabled={!connected || busy || !workspace}
                title={!workspace ? 'Сначала выбери проект' : 'Прикрепить файлы из проекта'}
                onClick={() => fileInputRef.current?.click()}
              >
                <ComposerIcon name="paperclip" />
                <span>Файлы</span>
              </button>
              <button
                type="button"
                className={`composer__action${toolsMenuOpen ? ' composer__action--active' : ''}`}
                disabled={!connected || busy}
                aria-expanded={toolsMenuOpen}
                title="Открыть режим доступа к инструментам"
                onClick={() => setToolsMenuOpen((value) => !value)}
              >
                <ComposerIcon name="layers" />
                <span>Инструменты</span>
              </button>
              <button
                type="button"
                className={`composer__action${webSearchEnabled ? ' composer__action--active' : ''}`}
                disabled={!connected || busy}
                aria-pressed={webSearchEnabled}
                title="Попросить Core использовать веб-поиск для этого запроса"
                onClick={() => setWebSearchEnabled((value) => !value)}
              >
                <ComposerIcon name="globe" />
                <span>Веб-поиск</span>
              </button>
              <span className="composer__shortcut">
                <kbd>Enter</kbd> — отправить · <kbd>Shift + Enter</kbd> — новая строка
              </span>
            </div>
          </div>

          <div className="composer__controls" aria-label="Параметры задачи">
            <div className="composer__control-group composer__control-group--project" aria-label="Проект чата">
              <button
                type="button"
                className="composer__project-action"
                onClick={() => void pickWorkspace()}
                disabled={busy || !onWorkspaceChange}
                aria-label="Выбрать / создать проект"
                title="Выбрать / создать проект"
              >
                <ComposerIcon name="folder" />
              </button>
              <span className="composer__control-label">Проект</span>
              <select
                aria-label="Проект"
                value={workspace ?? ''}
                onChange={(event) => void changeWorkspace(event.target.value)}
                disabled={busy}
              >
                <option value="">Без проекта</option>
                {projects.map((project) => (
                  <option key={project.path} value={project.path}>
                    {basename(project.path)}{project.available ? '' : ' · недоступен'}
                  </option>
                ))}
              </select>
            </div>
            <span className="composer__divider" aria-hidden="true" />
            <div className="composer__control-group">
              <ComposerIcon name="lock" className="composer__control-icon" />
              <span className="composer__control-label">Режим доступа</span>
              <PermissionModePicker
                connection={connection}
                workspace={workspace}
                open={toolsMenuOpen}
                onOpenChange={setToolsMenuOpen}
              />
            </div>
            <span className="composer__divider" aria-hidden="true" />
            <div className="composer__control-group">
              <ComposerIcon name="link" className="composer__control-icon" />
              <span className="composer__control-label">Провайдер</span>
              <ChatProviderPicker
                connection={connection}
                value={providerMode}
                onChange={setProviderMode}
                disabled={busy}
              />
            </div>
            <span className="composer__divider" aria-hidden="true" />
            <div className="composer__control-group composer__control-group--model">
              <ComposerIcon name="box" className="composer__control-icon" />
              <span className="composer__control-label">Модель</span>
              <ModelPicker connection={connection} events={events} provider={providerMode} use="agent" />
            </div>
            <div className="composer__usage">
              <ContextUsage events={taskEvents} />
            </div>
          </div>

          {workspace !== null ? (
            <div className="composer__repository">
              <RepositoryBar workspace={workspace} refreshKey={finished ? entries.length : 0} />
            </div>
          ) : null}

          {attachments.length > 0 ? (
            <div className="composer__attachments" aria-label="Прикреплённые файлы">
              {attachments.map((attachment) => (
                <span className="composer__attachment" key={attachment.relativePath}>
                  <span title={attachment.relativePath}>{attachment.name}</span>
                  <button
                    type="button"
                    aria-label={`Удалить файл ${attachment.name}`}
                    onClick={() => setAttachments((current) => current.filter((item) => item.relativePath !== attachment.relativePath))}
                  >
                    ×
                  </button>
                </span>
              ))}
            </div>
          ) : null}
          {webSearchEnabled ? <p className="composer__mode-hint" role="status">Веб-поиск включён для следующего запроса.</p> : null}
          {composerNotice ? <p className="composer__mode-hint" role="status">{composerNotice}</p> : null}


          {!connected ? (
            <p className="shell__reason">Core недоступен: запуск и управление задачей приостановлены.</p>
          ) : null}

          {commandError ? <p role="alert" className="shell__reason">{commandError}</p> : null}
        </div>
      </div>
    </section>
  )
}

function composeTaskPrompt(prompt: string, attachments: readonly ComposerAttachment[], webSearchEnabled: boolean): string {
  const sections = [prompt]
  if (attachments.length > 0) {
    sections.push(`Прикреплённые файлы из текущего проекта:\n${attachments.map((attachment) => `- ${attachment.relativePath}`).join('\n')}\nИспользуй их как дополнительный контекст задачи.`)
  }
  if (webSearchEnabled) {
    sections.push('Для этой задачи используй веб-поиск и актуальные внешние источники, если они доступны; укажи использованные источники в ответе.')
  }
  return sections.join('\n\n')
}

function relativeWorkspacePath(workspace: string, filePath: string): string | null {
  const normalize = (value: string): string => value.replaceAll('\\', '/').replace(/\/+$/, '')
  const root = normalize(workspace)
  const candidate = normalize(filePath)
  const prefix = `${root}/`
  if (candidate.toLowerCase() === root.toLowerCase() || !candidate.toLowerCase().startsWith(prefix.toLowerCase())) return null
  const relative = candidate.slice(prefix.length)
  return relative.length > 0 && !relative.split('/').includes('..') ? relative : null
}

type ComposerIconName = 'box' | 'folder' | 'globe' | 'layers' | 'link' | 'lock' | 'paperclip' | 'send'

function ComposerIcon({ name, className = '' }: { readonly name: ComposerIconName; readonly className?: string }): React.JSX.Element {
  const common = {
    className: `composer__icon${className.length > 0 ? ` ${className}` : ''}`,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 1.8,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
    'aria-hidden': true
  }

  switch (name) {
    case 'box':
      return <svg {...common}><path d="m12 3 8 4.5v9L12 21l-8-4.5v-9L12 3Z" /><path d="m4.5 7.5 7.5 4 7.5-4M12 11.5V21" /></svg>
    case 'folder':
      return <svg {...common}><path d="M3.5 6.5h6l2 2h9v9a2 2 0 0 1-2 2h-15v-13Z" /><path d="M3.5 10.5h17" /></svg>
    case 'globe':
      return <svg {...common}><circle cx="12" cy="12" r="8.5" /><path d="M3.7 12h16.6M12 3.5c2.2 2.3 3.2 5.1 3.2 8.5s-1 6.2-3.2 8.5c-2.2-2.3-3.2-5.1-3.2-8.5s1-6.2 3.2-8.5Z" /></svg>
    case 'layers':
      return <svg {...common}><path d="m12 3 8.5 4.5L12 12 3.5 7.5 12 3Z" /><path d="m4.5 12 7.5 4 7.5-4M4.5 16.5l7.5 4 7.5-4" /></svg>
    case 'link':
      return <svg {...common}><path d="M9 17H7a5 5 0 0 1 0-10h2M15 7h2a5 5 0 0 1 0 10h-2M8 12h8" /></svg>
    case 'lock':
      return <svg {...common}><rect x="5" y="10" width="14" height="10" rx="2" /><path d="M8 10V7a4 4 0 0 1 8 0v3M12 14v2" /></svg>
    case 'paperclip':
      return <svg {...common}><path d="m20.5 11.5-7.2 7.2a5 5 0 0 1-7.1-7.1l7.6-7.6a3.5 3.5 0 0 1 5 5l-7.6 7.6a2 2 0 1 1-2.8-2.8l7-7" /></svg>
    case 'send':
      return <svg {...common}><path d="m21 3-7.2 18-3.1-7.7L3 10.2 21 3Z" /><path d="m21 3-10.3 10.3" /></svg>
  }
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
    <TranscriptMessage
      key={messageId}
      kind={entry.kind}
      id={messageId}
      text={entry.text}
      failed={entry.kind === 'result' && entry.failed}
      entryTimes={entryTimes}
      copied={copiedMessageId === messageId}
      onCopy={onCopy}
    />
  )
}

interface TranscriptMessageProps {
  readonly kind: 'agent' | 'result'
  readonly id: string
  readonly text: string
  readonly failed: boolean
  readonly entryTimes: React.MutableRefObject<Map<string, number>>
  readonly copied: boolean
  readonly onCopy: (id: string) => void
}

const TranscriptMessage = memo(function TranscriptMessage({
  kind,
  id,
  text,
  failed,
  entryTimes,
  copied,
  onCopy
}: TranscriptMessageProps): React.JSX.Element {
  return (
    <li
      className={`message message--agent${kind === 'result' && failed ? ' message--error' : ''}`}
    >
      <div className="message__bubble"><MarkdownMessage text={text} /></div>
      <MessageActions
        id={id}
        text={text}
        atMs={messageTime(entryTimes, id)}
        copied={copied}
        onCopy={onCopy}
      />
    </li>
  )
}, (previous, next) =>
  previous.kind === next.kind
  && previous.id === next.id
  && previous.text === next.text
  && previous.failed === next.failed
  && previous.entryTimes === next.entryTimes
  && previous.copied === next.copied
  && previous.onCopy === next.onCopy
)

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

function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter((part) => part.length > 0)
  return parts.at(-1) ?? path
}
