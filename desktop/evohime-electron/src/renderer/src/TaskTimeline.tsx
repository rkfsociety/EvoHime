import { formatTime, translate, useAppLocale } from './i18n'
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'

import { HIVE_SYSTEM_CHAT_ID, type ChatIsolationMode, type ChatMessage, type ChatProviderMode, type ChatRecord, type ChatSessionSettings, type ConnectionState, type CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'
import { ModelPicker } from './ModelPicker'
import { HomeScreen } from './HomeScreen'
import { ActivityLine } from './ActivityLine'
import { buildTranscript } from './transcript'
import { MarkdownMessage } from './MarkdownMessage'
import { RecoveryBanner } from './RecoveryBanner'
import { PermissionModePicker } from './PermissionModePicker'
import { RoutingStatus } from './RoutingStatus'
import { ChatProviderPicker } from './ChatProviderPicker'
import { useProviderState } from './provider-state'
import { PinnedGoalBar } from './PinnedGoalBar'
import { HumanChatQuestions } from './HumanWorkItemsPanel'
import { UiIcon } from './UiIcon'
import { collectResolvedApprovalIds } from './approval-state'
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
  type OptimisticConversationMessage,
  type ConversationProjectionState
} from './conversation-projection'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']
const MAX_RENDERED_ITEMS = 80
const TIMELINE_WINDOW_OVERSCAN = 16
const TIMELINE_ITEM_HEIGHT_ESTIMATE_PX = 72
const TIMELINE_BOTTOM_THRESHOLD_PX = 48
const MAX_COMPOSER_HEIGHT_PX = 200
interface ComposerAttachment {
  readonly name: string
  readonly relativePath: string
}

type TimelineItemDescriptor =
  | { readonly key: string; readonly kind: 'user'; readonly message: ChatMessage; readonly delivery: OptimisticConversationMessage | null }
  | {
      readonly key: string
      readonly kind: 'transcript'
      readonly entry: ReturnType<typeof buildTranscript>['entries'][number]
      readonly keySuffix: string
      readonly activeActivity: boolean
    }

interface TaskEventIndexCache {
  readonly scope: string
  readonly events: readonly CoreEvent[]
  readonly byTask: ReadonlyMap<string, readonly CoreEvent[]>
}

function updateTaskEventIndex(
  events: readonly CoreEvent[],
  scope: string,
  previous: TaskEventIndexCache | null
): TaskEventIndexCache {
  if (previous?.scope === scope) {
    if (previous.events === events) return previous

    const appendOlderStart = previous.events.length
    if (
      previous.events.length > 0
      && events.length > appendOlderStart
      && events[0] === previous.events[0]
      && events[appendOlderStart - 1] === previous.events[appendOlderStart - 1]
    ) {
      const byTask = new Map(previous.byTask)
      const addedByTask = new Map<string, CoreEvent[]>()
      for (const event of events.slice(appendOlderStart)) {
        const added = addedByTask.get(event.taskId)
        if (added) added.push(event)
        else addedByTask.set(event.taskId, [event])
      }
      for (const [taskId, added] of addedByTask) {
        byTask.set(taskId, [...(byTask.get(taskId) ?? []), ...added])
      }
      return { scope, events, byTask }
    }

    const previousHead = previous.events[0]
    const newEventsEnd = previousHead ? events.findIndex((event) => event === previousHead) : -1
    if (newEventsEnd > 0) {
      const retainedPreviousCount = Math.min(previous.events.length, events.length - newEventsEnd)
      const retainedPreviousTail = previous.events[retainedPreviousCount - 1]
      if (retainedPreviousTail && events.at(-1) === retainedPreviousTail) {
        const byTask = new Map(previous.byTask)
        const droppedByTask = new Map<string, Set<CoreEvent>>()
        for (const event of previous.events.slice(retainedPreviousCount)) {
          const dropped = droppedByTask.get(event.taskId)
          if (dropped) dropped.add(event)
          else droppedByTask.set(event.taskId, new Set([event]))
        }
        for (const [taskId, dropped] of droppedByTask) {
          const retained = (byTask.get(taskId) ?? []).filter((event) => !dropped.has(event))
          if (retained.length > 0) byTask.set(taskId, retained)
          else byTask.delete(taskId)
        }

        const addedByTask = new Map<string, CoreEvent[]>()
        for (let index = newEventsEnd - 1; index >= 0; index -= 1) {
          const event = events[index]
          if (!event) continue
          const added = addedByTask.get(event.taskId)
          if (added) added.unshift(event)
          else addedByTask.set(event.taskId, [event])
        }
        for (const [taskId, added] of addedByTask) {
          byTask.set(taskId, [...added, ...(byTask.get(taskId) ?? [])])
        }
        return { scope, events, byTask }
      }
    }
  }

  const byTask = new Map<string, CoreEvent[]>()
  for (const event of events) {
    const grouped = byTask.get(event.taskId)
    if (grouped) grouped.push(event)
    else byTask.set(event.taskId, [event])
  }
  return { scope, events, byTask }
}

function buildCoreEventKey(event: CoreEvent): string | null {
  if (!Number.isSafeInteger(event.sequenceId) || event.sequenceId <= 0) return null
  return `${event.coreInstanceId ?? 'legacy'}:${event.sessionEpoch ?? 0}:${event.sequenceId}:${event.taskId}:${event.eventType}`
}

export interface TaskTimelineProps {
  readonly connection: ConnectionState
  /** True once Core has been connected at least once in this shell session. */
  readonly everConnected?: boolean
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
  readonly resolvedApprovalIds?: readonly string[]
  readonly onApprovalResolved?: (approvalId: string) => void
}

export function TaskTimeline({
  connection,
  everConnected = true,
  events,
  workspace,
  chatId,
  onChatTouched,
  onChatOpened,
  identityName,
  chatRevision,
  resolvedApprovalIds = [],
  onApprovalResolved
}: TaskTimelineProps): React.JSX.Element {
  const api = useShellApi()
  const [chat, setChat] = useState<ChatRecord | null>(null)
  const [prompt, setPrompt] = useState('')
  const [attachments, setAttachments] = useState<readonly ComposerAttachment[]>([])
  const [webSearchEnabled, setWebSearchEnabled] = useState(false)
  const [goalMode, setGoalMode] = useState(false)
  const [toolsMenuOpen, setToolsMenuOpen] = useState(false)
  const [composerNotice, setComposerNotice] = useState<string | null>(null)
  const [taskId, setTaskId] = useState<string | null>(null)
  const [startingTaskId, setStartingTaskId] = useState<string | null>(null)
  const [stopRequested, setStopRequested] = useState(false)
  const [sentPrompt, setSentPrompt] = useState<string | null>(null)
  const [sentPromptAtMs, setSentPromptAtMs] = useState<number | null>(null)
  const [commandError, setCommandError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const { summary: providerSummary, apply: applyProviderSummary } = useProviderState()
  const [providerMode, setProviderMode] = useState<ChatProviderMode>(() => {
    const stored = window.localStorage.getItem('evohime.chat-provider-mode')
    return stored === 'codex_cli' || stored === 'openai_compatible' || stored === 'openai_responses' || stored === 'literouter' || stored === 'ollama'
      ? stored
      : 'literouter'
  })
  const [chatModel, setChatModel] = useState(() => {
    const storedProvider = window.localStorage.getItem('evohime.chat-provider-mode')
    const initialProvider = storedProvider === 'codex_cli' || storedProvider === 'openai_compatible' || storedProvider === 'openai_responses' || storedProvider === 'literouter' || storedProvider === 'ollama'
      ? storedProvider
      : 'literouter'
    return window.localStorage.getItem(`evohime.chat-model.${initialProvider}`) ?? ''
  })
  const [chatIsolationMode, setChatIsolationMode] = useState<ChatIsolationMode>('shared_workspace')
  const [copiedMessageId, setCopiedMessageId] = useState<string | null>(null)
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
  const transcriptCacheRef = useRef<{
    chatId: string | null
    tasks: Map<string, { signature: string; transcript: ReturnType<typeof buildTranscript> }>
  }>({ chatId, tasks: new Map() })
  const taskEventIndexRef = useRef<TaskEventIndexCache | null>(null)
  const cancelRequestedTaskId = useRef<string | null>(null)
  const eventCursorRef = useRef<{ firstKey: string | null; length: number }>({
    firstKey: null,
    length: 0
  })
  const subscriptionCursorRef = useRef<string | null>(null)
  const previousConnectionRef = useRef<ConnectionState>(connection)
  const conversationLogRef = useRef<ConversationProjectionState | null>(null)

  useEffect(() => {
    // Only apply persisted session fields when a real chat record is loaded.
    // Clearing chatModel while `chat` is transiently null (for example during
    // first-message chat creation or chat reload) caused ModelPicker to
    // overwrite the user's chosen model with the first model in the catalog.
    if (!chat) return
    if (chat.session?.provider) {
      setProviderMode(chat.session.provider)
    }
    if (chat.session?.model) {
      setChatModel(chat.session.model)
    }
    if (chat.session?.isolationMode) {
      setChatIsolationMode(chat.session.isolationMode)
    }
  }, [chat?.id, chat?.session?.isolationMode, chat?.session?.model, chat?.session?.provider])

  const persistChatSession = useCallback(
    (patch: Partial<ChatSessionSettings>) => {
      const nextSession: ChatSessionSettings = {
        ...(providerMode !== 'codex_cli' ? { provider: providerMode } : {}),
        ...(chatModel.length > 0 ? { model: chatModel } : {}),
        ...(chatIsolationMode ? { isolationMode: chatIsolationMode } : {}),
        ...patch
      }
      if (!api || !chatId) return
      void api.invoke('chat.saveSessionSettings', { chatId, session: nextSession }).then((outcome) => {
        if (outcome.ok && outcome.value) setChat(outcome.value)
      })
    },
    [api, chatId, chatIsolationMode, chatModel, providerMode]
  )

  const handleChatProviderChange = useCallback(
    (next: ChatProviderMode) => {
      setProviderMode(next)
      window.localStorage.setItem('evohime.chat-provider-mode', next)
      const remembered =
        window.localStorage.getItem(`evohime.chat-model.${next}`) ??
        (next !== 'codex_cli' ? (providerSummary?.profiles?.[next]?.model ?? '') : '')
      setChatModel(remembered)
      if (next !== 'codex_cli' && api) {
        void api.invoke('provider.select', { provider: next }).then((outcome) => {
          if (outcome.ok) applyProviderSummary(outcome.value.summary)
        })
      }
      if (next !== 'codex_cli') {
        persistChatSession({ provider: next, model: remembered })
      }
    },
    [api, applyProviderSummary, persistChatSession, providerSummary?.profiles]
  )

  const handleChatModelChange = useCallback(
    (nextModel: string) => {
      setChatModel(nextModel)
      if (nextModel.length > 0) {
        window.localStorage.setItem(`evohime.chat-model.${providerMode}`, nextModel)
        if (api) {
          if (providerMode === 'codex_cli') {
            void api.invoke('codex.selectModel', { model: nextModel })
          } else {
            void api.invoke('core.selectModel', { model: nextModel })
          }
        }
      }
      persistChatSession({ model: nextModel })
    },
    [api, persistChatSession, providerMode]
  )

  const handleChatIsolationChange = useCallback(
    (nextIsolation: ChatIsolationMode) => {
      setChatIsolationMode(nextIsolation)
      persistChatSession({ isolationMode: nextIsolation })
    },
    [persistChatSession]
  )

  const setConversationProjection = useCallback((updater: (current: ConversationProjectionState | null) => ConversationProjectionState | null) => {
    setConversationLog((current) => {
      const next = updater(current)
      conversationLogRef.current = next
      return next
    })
  }, [])

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
    eventCursorRef.current.firstKey = null
    eventCursorRef.current.length = 0
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
    eventCursorRef.current.firstKey = null
    eventCursorRef.current.length = 0
    subscriptionCursorRef.current = null
  }, [connection])

  useEffect(() => {
    if (chatId === null) return

    const newEvents: CoreEvent[] = []
    const eventCursor = eventCursorRef.current
    const firstKey = events[0] ? buildCoreEventKey(events[0]) : null
    if (eventCursor.firstKey === null) {
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
        if (key !== null && key === eventCursor.firstKey) {
          break
        }
        newEvents.push(event)
      }
      // The app retains only a bounded live window. If the old boundary fell
      // out of it, process that bounded window and rely on Core projection
      // sequence/idempotency checks for replay safety.
    }
    eventCursor.firstKey = firstKey
    eventCursor.length = events.length

    const pageEnvelopes: Array<{ event: CoreEvent; page: NonNullable<CoreEvent['conversationEventLog']> }> = []
    for (const event of newEvents) {
      const page = event.conversationEventLog
      if (page == null || page.conversationId !== chatId) continue
      pageEnvelopes.push({ event, page })
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

  const taskEventScope = JSON.stringify([
    chatId,
    projectedConversationEvents.length > 0,
    [...new Set([...(chat?.taskIds ?? []), ...(taskId ? [taskId] : [])])].sort()
  ])
  const taskEventsByTask = useMemo(() => {
    const nextIndex = updateTaskEventIndex(taskEvents, taskEventScope, taskEventIndexRef.current)
    taskEventIndexRef.current = nextIndex
    return nextIndex.byTask
  }, [taskEventScope, taskEvents])

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

  if (transcriptCacheRef.current.chatId !== chatId) {
    transcriptCacheRef.current = { chatId, tasks: new Map() }
  }

  const transcriptForTask = useCallback((id: string, taskEventsForMessage: readonly CoreEvent[]) => {
    const signature = `${taskEventsForMessage.length}:${taskEventsForMessage[0]?.sequenceId ?? ''}:${taskEventsForMessage.at(-1)?.sequenceId ?? ''}`
    const cached = transcriptCacheRef.current.tasks.get(id)
    if (cached?.signature === signature) return cached.transcript
    const transcript = buildTranscript(taskEventsForMessage)
    transcriptCacheRef.current.tasks.set(id, { signature, transcript })
    return transcript
  }, [])

  const { entries, approval, finished } = useMemo(
    () => activeTaskId === null
      ? buildTranscript(activeTaskEvents)
      : transcriptForTask(activeTaskId, activeTaskEvents.filter((event) => event.taskId === activeTaskId)),
    [activeTaskEvents, activeTaskId, transcriptForTask]
  )
  const resolvedApprovals = useMemo(
    () => collectResolvedApprovalIds(activeTaskEvents, resolvedApprovalIds),
    [activeTaskEvents, resolvedApprovalIds]
  )
  const visibleApproval = approval && !resolvedApprovals.has(approval.approvalId) ? approval : null

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
    const retainedTasks = new Set(messages.map((message) => message.taskId))
    for (const cachedTaskId of transcriptCacheRef.current.tasks.keys()) {
      if (!retainedTasks.has(cachedTaskId)) transcriptCacheRef.current.tasks.delete(cachedTaskId)
    }
    return messages.map((message) => {
      const taskEventsForMessage = taskEventsByTask.get(message.taskId) ?? []
      return {
        message,
        delivery: conversationLog?.optimistic.find(
          (item) => item.clientMessageId === message.clientMessageId
        ) ?? null,
        transcript: transcriptForTask(message.taskId, taskEventsForMessage)
      }
    })
  }, [chatId, chat?.messages, conversationLog?.optimistic, projectedConversationEvents, sentPrompt, sentPromptAtMs, taskId, taskEvents, taskEventsByTask, transcriptForTask])

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
      preferredRouteHint: conversationLog.conversationId === HIVE_SYSTEM_CHAT_ID ? 'hive' : (providerMode === 'codex_cli' ? 'codex_cli' : 'cloud'),
      executionKind: conversationLog.conversationId === HIVE_SYSTEM_CHAT_ID ? 'dialogue' : (providerMode === 'codex_cli' ? 'coding' : 'dialogue'),
      ...(providerMode !== 'codex_cli' ? { provider: providerMode } : {}),
      ...(chatModel.length > 0 ? { model: chatModel } : {}),
      ...(chatIsolationMode === 'isolated_worktree' ? { isolationMode: chatIsolationMode } : {})
    })
    if (!outcome.ok) {
      setConversationProjection((current) => current ? markOptimisticFailed(current, clientMessageId) : current)
      setCommandError(outcome.message)
    }
  }, [api, chatIsolationMode, chatModel, conversationLog, providerMode, setConversationProjection, workspace])

  const running = activeTaskId !== null && (startingTaskId === activeTaskId || !finished)

  const timelineItems = useMemo(() => {
    if (conversation.length > 0) {
      return conversation.flatMap(({ message, transcript, delivery }, messageIndex): TimelineItemDescriptor[] => {
        const messageId = `user-${message.taskId}-${message.atMs}`
        const isLastConversationTurn = messageIndex === conversation.length - 1
        return [
          { key: messageId, kind: 'user', message, delivery },
          ...transcript.entries.map((entry, index) => {
            const keySuffix = `${message.taskId}-${index}`
            const activeActivity =
              entry.kind === 'activity'
              && isLastConversationTurn
              && index === transcript.entries.length - 1
              && !transcript.finished
              && running
              && !visibleApproval
            return {
              key: `${entry.kind}-${entry.id}-${keySuffix}`,
              kind: 'transcript' as const,
              entry,
              keySuffix,
              activeActivity
            }
          })
        ]
      })
    }
    return entries.map((entry, index) => {
      const keySuffix = String(index)
      const activeActivity =
        entry.kind === 'activity'
        && index === entries.length - 1
        && !finished
        && running
        && !visibleApproval
      return {
        key: `${entry.kind}-${entry.id}-${keySuffix}`,
        kind: 'transcript' as const,
        entry,
        keySuffix,
        activeActivity
      }
    })
  }, [conversation, entries, finished, running, visibleApproval])

  const timelineItemKeys = useMemo(
    () => timelineItems.map((item) => item.key),
    [timelineItems]
  )
  const maxTimelineWindowStart = Math.max(0, timelineItems.length - MAX_RENDERED_ITEMS)
  const renderedTimelineStart = Math.min(timelineWindowStart, maxTimelineWindowStart)
  const renderedTimelineDescriptors = timelineItems.slice(renderedTimelineStart, renderedTimelineStart + MAX_RENDERED_ITEMS)
  const renderedTimelineItems = useMemo(
    () => renderedTimelineDescriptors.map((item) => {
      if (item.kind === 'transcript') {
        return renderTranscriptEntry(
          item.entry,
          item.keySuffix,
          entryTimes,
          copiedMessageId,
          setCopiedMessageId,
          item.activeActivity
        )
      }
      const messageId = item.key
      const { message, delivery } = item
      return (
        <li key={messageId} className="message message--user">
          <div className="message__bubble">{message.prompt}</div>
          {delivery ? (
            <small className="message__delivery" role="status">
              {translate(delivery.status === 'sending' ? translate("Отправляется…") : null)}
              {translate(delivery.status === 'retry' ? translate("Повторная отправка…") : null)}
              {delivery.status === 'failed' ? (
                <button type="button" onClick={() => void retryMessage(delivery.clientMessageId)}>
                  {translate("Повторить отправку")}</button>
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
        </li>
      )
    }),
    [copiedMessageId, entryTimes, renderedTimelineDescriptors, retryMessage]
  )

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

  const launchTask = useCallback(async (text: string, goalId?: string, chatIdOverride?: string): Promise<boolean> => {
    if (!api || text.trim().length === 0) return false
    const nextTaskId = makeTaskId()
    const clientMessageId = globalThis.crypto.randomUUID()
    cancelRequestedTaskId.current = null
    setTaskId(nextTaskId)
    setStartingTaskId(nextTaskId)
    setStopRequested(false)
    setBusy(true)
    setCommandError(null)

    // Typing is the intent to start a conversation, so the first prompt of a
    // session creates the chat instead of demanding it be made first.
    let targetChatId = chatIdOverride ?? chatId
    if (targetChatId === null) {
      const created = await api.invoke('chat.create', { workspacePath: workspace })
      if (!created.ok) {
        setTaskId(null)
        setStartingTaskId(null)
        setBusy(false)
        setCommandError(created.message)
        return false
      }
      targetChatId = created.value.id
      let initialChat = created.value
      const initialSession: ChatSessionSettings = {
        ...(providerMode !== 'codex_cli' ? { provider: providerMode } : {}),
        ...(chatModel.length > 0 ? { model: chatModel } : {}),
        ...(chatIsolationMode ? { isolationMode: chatIsolationMode } : {})
      }
      const savedSession = await api.invoke('chat.saveSessionSettings', {
        chatId: targetChatId,
        session: initialSession
      })
      if (savedSession.ok && savedSession.value) initialChat = savedSession.value
      setChat(initialChat)
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
      preferredRouteHint: targetChatId === HIVE_SYSTEM_CHAT_ID ? 'hive' : (providerMode === 'codex_cli' ? 'codex_cli' : 'cloud'),
      executionKind: targetChatId === HIVE_SYSTEM_CHAT_ID ? 'dialogue' : (providerMode === 'codex_cli' ? 'coding' : 'dialogue'),
      ...(providerMode !== 'codex_cli' ? { provider: providerMode } : {}),
      ...(chatModel.length > 0 ? { model: chatModel } : {}),
      ...(chatIsolationMode === 'isolated_worktree' ? { isolationMode: chatIsolationMode } : {}),
      ...(goalId ? { goalId } : {})
    })
    setBusy(false)
    if (!outcome.ok) {
      setTaskId(null)
      setStartingTaskId(null)
      setStopRequested(false)
      cancelRequestedTaskId.current = null
      setConversationProjection((current) => current ? markOptimisticFailed(current, clientMessageId) : current)
      setCommandError(outcome.message)
      return false
    }
    setStartingTaskId(null)
    setTaskId(nextTaskId)
    setSentPrompt(text)
    setSentPromptAtMs(Date.now())
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
    return true
  }, [api, chatId, onChatOpened, onChatTouched, providerMode, setConversationProjection, workspace])

  const start = useCallback(async () => {
    if (!api || (prompt.trim().length === 0 && (attachments.length === 0 || goalMode))) return
    const rawPrompt = prompt.trim()
    const parsedGoal = parseGoalSlashCommand(rawPrompt)
    if (parsedGoal.isGoalCommand) {
      if (parsedGoal.objective.length === 0) {
        setComposerNotice('Укажи цель после /goal, например: /goal починить тесты')
        return
      }
      if (!workspace) {
        setComposerNotice('Сначала выбери проект, чтобы поставить цель.')
        return
      }
    }
    const effectivePrompt = parsedGoal.isGoalCommand ? parsedGoal.objective : rawPrompt
    const text = composeTaskPrompt(effectivePrompt, attachments, webSearchEnabled)
    let goalId: string | undefined
    let targetChatId = chatId ?? undefined
    if ((parsedGoal.isGoalCommand || goalMode) && workspace) {
      setBusy(true)
      setCommandError(null)
      if (!targetChatId) {
        const createdChat = await api.invoke('chat.create', { workspacePath: workspace })
        if (!createdChat.ok) {
          setBusy(false)
          setCommandError(createdChat.message)
          return
        }
        targetChatId = createdChat.value.id
        setChat(createdChat.value)
        onChatOpened(targetChatId)
      }
      const createdGoalId = `goal-${makeTaskId()}`
      const created = await api.invoke('core.createGoal', {
        goalId: createdGoalId,
        workspacePath: workspace,
        chatId: targetChatId,
        objective: effectivePrompt,
        successCriteria: [{ id: 'criterion-1', kind: 'manual', statement: 'Цель полностью выполнена и проверена.' }],
        continuationBudget: 12,
        idempotencyKey: `goal:create:${createdGoalId}`
      })
      if (!created.ok) {
        setBusy(false)
        setCommandError(created.message)
        return
      }
      goalId = createdGoalId
    }
    const launched = await launchTask(text, goalId, targetChatId)
    if (!launched) return
    setPrompt('')
    setAttachments([])
    setWebSearchEnabled(false)
    if (goalMode) setGoalMode(false)
  }, [api, attachments, chatId, goalMode, launchTask, onChatOpened, prompt, webSearchEnabled, workspace])

  const selectFiles = useCallback((files: readonly File[]) => {
    if (files.length === 0) return
    if (!workspace) {
      setComposerNotice('Сначала выбери проект, чтобы прикрепить файл.')
      return
    }
    const selected: ComposerAttachment[] = []
    let rejected = false
    for (const file of files) {
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
      try {
        const outcome = await api.invoke('core.resolveApproval', {
          approvalId: approval.approvalId,
          granted,
          idempotencyKey: `approval:${approval.approvalId}:${granted ? 'grant' : cancel ? 'cancel' : 'reject'}`,
          ...(granted ? {} : { rejectionReason: cancel ? 'cancelled_by_user' : 'rejected_by_user' }),
          cancel
        })
        if (!outcome.ok) setCommandError(outcome.message)
        else onApprovalResolved?.(approval.approvalId)
      } catch {
        setCommandError('Не удалось отправить решение в Core; запрос остаётся ожидающим.')
      } finally {
        setBusy(false)
      }
    },
    [api, approval, onApprovalResolved]
  )

  const connected = CONNECTED_STATES.includes(connection)
  const canStart = connected && !busy && (prompt.trim().length > 0 || attachments.length > 0 && !goalMode)
  // Запрос разрешения может прийти раньше любой другой записи ленты.
  const empty =
    entries.length === 0 && sentPrompt === null && visibleApproval === null && conversation.length === 0

  return (
    <section className={`chat${empty ? ' chat--empty' : ''}`} aria-label={translate("Ход задачи")}>
      <RecoveryBanner
        connection={connection}
        events={taskEvents}
        onOpenTask={() => {}}
        showOpenTask={false}
      />
      <RoutingStatus events={taskEvents} connection={connection} everConnected={everConnected} />
      {conversationLog?.historyReady && conversationLog.sync.state === 'gap' ? (
        <p role="alert" className="shell__reason">{translate("История неполна, восстанавливаю пропущенные события…")}</p>
      ) : null}
      {conversationLog?.sync.state === 'conflict' ? (
        <p role="alert" className="shell__reason">{translate("Обнаружен конфликт последовательности истории.")}</p>
      ) : null}
      {conversationLog?.sync.state === 'cursor-expired' ? (
        <p role="alert" className="shell__reason">{translate("Старая часть истории свёрнута; загружаю доступный диапазон…")}</p>
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
                  {translate(loadingOlderHistory ? translate("Загружаю историю…") : translate("Загрузить более старую историю"))}
                </button>
              </div>
            ) : null}
            <ol className="chat__stream">
              {translate(renderedTimelineStart > 0 ? (
                <li className="chat__window-spacer" aria-hidden="true" style={{ height: `${renderedTimelineStart * TIMELINE_ITEM_HEIGHT_ESTIMATE_PX}px` }} />
              ) : null)}
              {translate(renderedTimelineItems)}
              {translate(renderedTimelineStart + renderedTimelineItems.length < timelineItems.length ? (
                <li className="chat__window-spacer" aria-hidden="true" style={{ height: `${(timelineItems.length - renderedTimelineStart - renderedTimelineItems.length) * TIMELINE_ITEM_HEIGHT_ESTIMATE_PX}px` }} />
              ) : null)}

              {conversation.length > 0 && running && !visibleApproval && conversation.at(-1)?.transcript.entries.at(-1)?.kind !== 'activity' ? (
                <li className="message message--working" role="status" aria-label={translate("Агент формирует ответ")}>
                  <span className="working-indicator" aria-hidden="true">
                    <span />
                    <span />
                    <span />
                  </span>
                </li>
              ) : null}

              {visibleApproval ? (
                <li className="approval task-timeline__approval" role="alert">
                  <strong>{translate("Нужно разрешение:")} {translate(visibleApproval.toolName)}</strong>
                  <span>{translate(visibleApproval.permission)} · {translate(visibleApproval.scope)}</span>
                  <strong>{visibleApproval.preview.summary}</strong>
                  {visibleApproval.preview.command ? <code>{translate("Команда:")}{visibleApproval.preview.command}</code> : null}
                  {visibleApproval.preview.cwd ? <code>cwd: {visibleApproval.preview.cwd}</code> : null}
                  {visibleApproval.preview.path ? <code>{translate("Файл:")}{visibleApproval.preview.path}</code> : null}
                  {visibleApproval.preview.details ? <pre className="approval__details">{visibleApproval.preview.details}</pre> : null}
                  {visibleApproval.preview.truncated ? <small>{translate("Preview ограничен по размеру.")}</small> : null}
                  <div>
                    <button type="button" onClick={() => void resolveApproval(true)} disabled={busy}>{translate("Разрешить")}</button>
                    <button type="button" onClick={() => void resolveApproval(false)} disabled={busy}>{translate("Отклонить")}</button>
                    <button type="button" onClick={() => void resolveApproval(false, true)} disabled={busy}>{translate("Отменить")}</button>
                  </div>
                </li>
              ) : null}
            </ol>
          </>
        )}
      </div>

      <HumanChatQuestions connection={connection} />

      <PinnedGoalBar
        connection={connection}
        events={events}
        workspace={workspace}
        chatId={chatId}
        taskRunning={running}
        disabled={busy || running}
        onRunGoal={async (goalId, objective) => {
          await launchTask(objective, goalId)
        }}
      />

      <div className="composer">
        <div className="composer__inner">
          <div className="composer__box">
            <div className="composer__prompt-row">
              <input
                ref={fileInputRef}
                className="visually-hidden"
                type="file"
                multiple
                tabIndex={-1}
                aria-hidden="true"
                onChange={(event) => {
                  selectFiles(Array.from(event.target.files ?? []))
                  event.target.value = ''
                }}
              />
              <button
                type="button"
                className="composer__action composer__action--icon"
                aria-label={translate("Прикрепить файлы")}
                disabled={!connected || busy || !workspace}
                title={!workspace ? translate("Сначала выбери проект") : translate("Прикрепить файлы из проекта")}
                onClick={() => fileInputRef.current?.click()}
              >
                <ComposerIcon name="plus" />
              </button>
              <label htmlFor="task-prompt" className="visually-hidden">{translate("Задача")}</label>
              <textarea
                id="task-prompt"
                ref={promptRef}
                value={prompt}
                onChange={(event) => setPrompt(event.target.value)}
                onPaste={(event) => {
                  const files = Array.from(event.clipboardData.files)
                  if (files.length === 0) return
                  event.preventDefault()
                  selectFiles(files)
                }}
                onKeyDown={(event) => {
                  if (event.key === 'Enter' && !event.shiftKey) {
                    event.preventDefault()
                    if (canStart) void start()
                  }
                }}
                placeholder={goalMode
                  ? translate("Опиши цель — Ева закрепит её и будет выполнять до конца…")
                  : translate("Опиши задачу для агента или /goal <цель>…")}
                disabled={!connected || busy}
                rows={1}
              />
              <button
                type="button"
                className={`composer__action composer__action--icon${goalMode ? ' composer__action--active' : ''}`}
                disabled={!connected || busy || !workspace}
                aria-label={translate("Режим цели")}
                aria-pressed={goalMode}
                title={!workspace ? translate("Сначала выбери проект") : translate("Поставить цель: закрепить над композером и выполнять до полного завершения")}
                onClick={() => setGoalMode((value) => !value)}
              >
                <ComposerIcon name="target" />
              </button>
              <button
                type="button"
                className={`composer__action composer__action--icon${webSearchEnabled ? ' composer__action--active' : ''}`}
                disabled={!connected || busy}
                aria-label={translate("Веб-поиск")}
                aria-pressed={webSearchEnabled}
                title={translate("Попросить Core использовать веб-поиск для этого запроса")}
                onClick={() => setWebSearchEnabled((value) => !value)}
              >
                <ComposerIcon name="globe" />
              </button>
              <span
                className="composer__send-wrap"
                title={running
                  ? (stopRequested ? translate("Остановка задачи") : translate("Остановить задачу"))
                  : `Enter ${translate("— отправить ·")} Shift + Enter ${translate("— новая строка ·")} Ctrl + V ${translate("— вставить файлы")}`}
              >
                <button
                  type="button"
                  className={`composer__send${running ? ' composer__send--stop' : ''}`}
                  aria-label={running ? (stopRequested ? translate("Остановка задачи") : translate("Остановить задачу")) : translate("Запустить задачу")}
                  onClick={() => {
                    if (running) void stop()
                    else if (canStart) void start()
                  }}
                  disabled={running ? stopRequested || !connected : !canStart}
                >
                  {running ? <UiIcon name="stop" size={15} className="composer__icon" /> : <ComposerIcon name="send" />}
                </button>
              </span>
            </div>
          </div>

          {attachments.length > 0 ? (
            <div className="composer__attachments" aria-label={translate("Прикреплённые файлы")}>
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
          {webSearchEnabled ? <p className="composer__mode-hint" role="status">{translate("Веб-поиск включён для следующего запроса.")}</p> : null}
          {goalMode ? <p className="composer__mode-hint" role="status">{translate("Режим цели: задача закрепится над композером и будет выполняться до конца.")}</p> : null}
          {composerNotice ? <p className="composer__mode-hint" role="status">{translate(composerNotice)}</p> : null}


          <div className="composer__footer">
            <div className="composer__footer-start">
              <div className="composer__scope" title={workspace ?? translate("Без проекта")}>
                <ComposerIcon name="folder" className="composer__control-icon" />
                <strong>{translate(workspace ? basename(workspace) : translate("Без проекта"))}</strong>
              </div>
              <div className="composer__control-group">
                <ComposerIcon name="link" className="composer__control-icon" />
                <ChatProviderPicker
                  perChat
                  connection={connection}
                  value={providerMode}
                  onChange={handleChatProviderChange}
                  disabled={busy}
                />
              </div>
              <div className="composer__control-group composer__control-group--model">
                <ComposerIcon name="box" className="composer__control-icon" />
                <ModelPicker
                  connection={connection}
                  events={events}
                  provider={providerMode}
                  selectedModel={chatModel}
                  onSelectChatModel={handleChatModelChange}
                  use="agent"
                />
              </div>
              <div className="composer__control-group">
                <ComposerIcon name="lock" className="composer__control-icon" />
                <PermissionModePicker
                  connection={connection}
                  workspace={workspace}
                  open={toolsMenuOpen}
                  onOpenChange={setToolsMenuOpen}
                />
              </div>
              {workspace !== null && chatId !== HIVE_SYSTEM_CHAT_ID ? (
                <div className="composer__control-group">
                  <ComposerIcon name="git-branch" className="composer__control-icon" />
                  <label className="composer__isolation">
                    <span className="sr-only">{translate("Изоляция файлов")}</span>
                    <select
                      aria-label={translate("Изоляция файлов")}
                      value={chatIsolationMode}
                      disabled={busy || !connected}
                      onChange={(event) => handleChatIsolationChange(event.target.value as ChatIsolationMode)}
                    >
                      <option value="shared_workspace">{translate("Общая папка")}</option>
                      <option value="isolated_worktree">{translate("Изолированный worktree")}</option>
                    </select>
                    <span className="composer__select-chevron" aria-hidden="true"><UiIcon name="chevron-down" size={12} /></span>
                  </label>
                </div>
              ) : null}
              {!connected ? <p className="shell__reason">{translate("Core недоступен: запуск и управление задачей приостановлены.")}</p> : null}
            </div>
          </div>

          {commandError ? <p role="alert" className="shell__reason">{translate(commandError)}</p> : null}
        </div>
      </div>
    </section>
  )
}

export function parseGoalSlashCommand(
  rawPrompt: string
): { readonly isGoalCommand: true; readonly objective: string } | { readonly isGoalCommand: false } {
  const match = rawPrompt.trim().match(/^\/goal(?:\s+([\s\S]*))?$/i)
  if (!match) return { isGoalCommand: false }
  return { isGoalCommand: true, objective: (match[1] ?? '').trim() }
}

function composeTaskPrompt(prompt: string, attachments: readonly ComposerAttachment[], webSearchEnabled: boolean): string {
  const sections = prompt.length > 0 ? [prompt] : []
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

type ComposerIconName = 'box' | 'folder' | 'git-branch' | 'globe' | 'link' | 'lock' | 'plus' | 'send' | 'target'

function ComposerIcon({ name, className = '' }: { readonly name: ComposerIconName; readonly className?: string }): React.JSX.Element {
  return <UiIcon name={name} size={16} className={`composer__icon${className.length > 0 ? ` ${className}` : ''}`} />
}

function renderTranscriptEntry(
  entry: ReturnType<typeof buildTranscript>['entries'][number],
  keySuffix: string,
  entryTimes: React.MutableRefObject<Map<string, number>>,
  copiedMessageId: string | null,
  onCopy: (id: string) => void,
  activeActivity = false
): React.JSX.Element {
  if (entry.kind === 'approval') {
    return (
      <li key={`${entry.kind}-${entry.id}-${keySuffix}`} className="message message--note" data-testid="approval-history-entry">
        <strong>{translate('Запрос подтверждения')}</strong>
        <span>{translate(entry.summary)} · {translate(entry.toolName)}</span>
      </li>
    )
  }
  if (entry.kind === 'activity') {
    return (
      <li key={`${entry.kind}-${entry.id}-${keySuffix}`} className="message message--activity">
        <ActivityLine calls={entry.calls} running={entry.running} active={activeActivity} />
      </li>
    )
  }
  if (entry.kind === 'stopped') {
    return (
      <li key={`${entry.kind}-${entry.id}-${keySuffix}`} className="message message--note">
        <span className="message__note">{translate("Задача остановлена")}</span>
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
  useAppLocale()
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
  if (existing !== undefined) {
    times.current.delete(id)
    times.current.set(id, existing)
    return existing
  }
  const now = Date.now()
  times.current.set(id, now)
  while (times.current.size > 256) {
    const oldest = times.current.keys().next().value
    if (oldest === undefined) break
    times.current.delete(oldest)
  }
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
        aria-label={copied ? translate("Сообщение скопировано") : translate("Скопировать сообщение")}
        title={copied ? translate("Скопировано") : translate("Скопировать")}
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
        <UiIcon name={copied ? 'check' : 'copy'} size={13} />
      </button>
      {atMs !== null ? <time dateTime={new Date(atMs).toISOString()}>{translate(formatMessageTime(atMs))}</time> : null}
    </div>
  )
}

function formatMessageTime(atMs: number): string {
  return formatTime(atMs)
}

function makeTaskId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  return `task-${Date.now()}-${Math.random().toString(16).slice(2)}`
}

function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter((part) => part.length > 0)
  return parts.at(-1) ?? path
}
