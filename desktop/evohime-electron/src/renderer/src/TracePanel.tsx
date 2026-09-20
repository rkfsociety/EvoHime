import { useEffect, useState } from 'react'

import type { ChatRecord, CoreEvent, ShellState } from '@shared/api'
import type { UpdateStatus } from '@shared/update'

import { useShellApi } from './shell-api'
import { filterEventsForChat } from './trace-filter'

export { filterEventsForChat } from './trace-filter'

export interface TraceDiagnostics {
  readonly errorCode: string
  readonly source: string
  readonly operation: string
}

interface Props {
  readonly chatId: string | null
  /** Reloads the persisted task ids after a prompt or task changes the chat. */
  readonly chatRevision?: number
  readonly events: readonly CoreEvent[]
  readonly state: ShellState | null
  readonly update?: UpdateStatus | null
  readonly workspace: string | null
  readonly onClose: () => void
}

export function TracePanel({ chatId, chatRevision = 0, events, state, update = null, workspace, onClose }: Props): React.JSX.Element {
  const api = useShellApi()
  const [chat, setChat] = useState<ChatRecord | null>(null)
  const [saveStatus, setSaveStatus] = useState<string | null>(null)
  const traceEvents = filterEventsForChat(events, chat)

  useEffect(() => {
    if (!api || chatId === null) {
      setChat(null)
      return
    }
    setChat(null)
    let active = true
    void api.invoke('chat.open', { chatId }).then((outcome) => {
      if (active && outcome.ok) setChat(outcome.value)
    })
    return () => {
      active = false
    }
  }, [api, chatId, chatRevision])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onClose])

  const save = async () => {
    if (!api) return
    setSaveStatus('Сохраняю…')
    const outcome = await api.invoke('trace.export', { content: formatTrace(state, workspace, traceEvents, update) })
    if (!outcome.ok) {
      setSaveStatus(outcome.message)
      return
    }
    setSaveStatus(outcome.value.cancelled ? null : 'Трейс сохранён в Markdown-файл.')
  }

  return (
    <aside className="trace-panel" aria-label="Трейс текущего чата" aria-live="polite">
        <header className="trace-panel__header">
          <div>
            <h2>Трейс</h2>
            <p>{traceEvents.length} событий текущего чата · новые сверху</p>
          </div>
          <div className="trace-panel__actions">
            <button type="button" onClick={() => void save()}>Сохранить .md</button>
            <button type="button" className="trace-panel__close" aria-label="Закрыть трейс" onClick={onClose}>×</button>
          </div>
        </header>
        <dl className="trace-panel__summary">
          <div><dt>Подключение</dt><dd>{state?.connection ?? 'неизвестно'}</dd></div>
          <div><dt>Core модуль</dt><dd>{update?.installedModules?.core ?? '—'}</dd></div>
          <div><dt>Core runtime</dt><dd>{state?.coreVersion ?? '—'}</dd></div>
          <div><dt>Протокол</dt><dd>{state?.protocol ? `v${state.protocol.major}.${state.protocol.minor}` : '—'}</dd></div>
          <div><dt>Последний sequence</dt><dd>{state?.lastSequence ?? 0}</dd></div>
          <div><dt>Workspace</dt><dd title={workspace ?? undefined}>{workspace ?? 'не выбран'}</dd></div>
        </dl>
        {saveStatus ? <p className="trace-panel__reason" role="status">{saveStatus}</p> : null}
        {state?.reason ? <p className="trace-panel__reason">Причина: {safeTraceReason(state.reason)}</p> : null}
        {chatId === null ? (
          <p className="trace-panel__empty">Выбери чат, чтобы открыть его трейс.</p>
        ) : traceEvents.length === 0 ? (
          <p className="trace-panel__empty">События появятся после запуска задачи в этом чате.</p>
        ) : (
          <ol className="trace-panel__events">
            {traceEvents.map((event) => (
              <TraceEventItem key={`${event.sequenceId}-${event.eventType}`} event={event} />
            ))}
          </ol>
        )}
    </aside>
  )
}

function TraceEventItem({ event }: { readonly event: CoreEvent }): React.JSX.Element {
  const payload = formatTraceEventPayload(event.eventType, event.payload)
  const diagnostics = parseTraceDiagnostics(payload)
  return (
    <li className="trace-event">
      <div className="trace-event__meta">
        <code>{event.eventType}</code>
        <span>#{event.sequenceId}</span>
      </div>
      {event.taskId ? <small className="trace-event__task">task: {event.taskId}</small> : null}
      {diagnostics ? <TraceDiagnosticsView diagnostics={diagnostics} /> : null}
      <pre>{payload}</pre>
    </li>
  )
}

function TraceDiagnosticsView({ diagnostics }: { readonly diagnostics: TraceDiagnostics }): React.JSX.Element {
  return (
    <dl className="trace-event__diagnostics" aria-label="Диагностика ошибки">
      <div><dt>Код ошибки</dt><dd><code>{diagnostics.errorCode}</code></dd></div>
      <div><dt>Источник</dt><dd><code>{diagnostics.source}</code></dd></div>
      <div><dt>Операция</dt><dd><code>{diagnostics.operation}</code></dd></div>
    </dl>
  )
}

function formatPayload(payload: string): string {
  if (!payload) return 'без payload'
  try {
    return JSON.stringify(JSON.parse(payload), null, 2)
  } catch {
    return payload
  }
}

function formatTraceEventPayload(eventType: string, payload: string): string {
  if (eventType !== 'task.failed') return formatPayload(payload)
  const diagnostics = parseTraceDiagnostics(payload) ?? {
    errorCode: 'task_failed',
    source: 'core',
    operation: 'task.execute'
  }
  return JSON.stringify({
    redacted: true,
    conversation_projection: true,
    terminal: true,
    error_code: diagnostics.errorCode,
    source: diagnostics.source,
    operation: diagnostics.operation
  }, null, 2)
}

export function parseTraceDiagnostics(payload: string): TraceDiagnostics | null {
  try {
    const value: unknown = JSON.parse(payload)
    if (!value || typeof value !== 'object' || Array.isArray(value)) return null
    const record = value as Record<string, unknown>
    const errorCode = safeTraceToken(record.error_code)
    const source = safeTraceToken(record.source)
    const operation = safeTraceToken(record.operation)
    if (!errorCode || !source || !operation) return null
    return { errorCode, source, operation }
  } catch {
    return null
  }
}

function safeTraceToken(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const token = value.trim()
  if (
    !token ||
    [...token].length > 128 ||
    !/^[A-Za-z0-9_.:-]+$/.test(token) ||
    /secret|token|password|bearer|sk-/i.test(token)
  ) return null
  return token
}

function safeTraceReason(reason: string | null): string {
  if (!reason?.trim()) return 'none'
  const token = safeTraceToken(reason)
  if (token) return token
  const lower = reason.toLowerCase()
  if (lower.includes('err_blocked_by_client')) return 'client_blocked'
  if (lower.includes('timeout') || lower.includes('timed out')) return 'timeout'
  if (lower.includes('permission') || lower.includes('access denied')) return 'permission_denied'
  if (lower.includes('protocol') || lower.includes('frame') || lower.includes('replay')) return 'protocol_error'
  return 'unavailable'
}

export function formatTrace(
  state: ShellState | null,
  workspace: string | null,
  events: readonly CoreEvent[],
  update: UpdateStatus | null = null
): string {
  const lines = [
    'EvoHime trace',
    `captured_at: ${new Date().toISOString()}`,
    `connection: ${state?.connection ?? 'unknown'}`,
    `core_module_version: ${update?.installedModules?.core ?? 'unknown'}`,
    `core_runtime_version: ${state?.coreVersion ?? 'unknown'}`,
    `protocol: ${state?.protocol ? `${state.protocol.major}.${state.protocol.minor}` : 'unknown'}`,
    `last_sequence: ${state?.lastSequence ?? 0}`,
    `reconnect_attempts: ${state?.reconnectAttempts ?? 0}`,
    `workspace: ${workspace ?? 'none'}`,
    `reason: ${safeTraceReason(state?.reason ?? null)}`,
    `events: ${events.length}`,
    ''
  ]

  const diagnostics = events.flatMap((event) => {
    const value = parseTraceDiagnostics(formatTraceEventPayload(event.eventType, event.payload))
    return value ? [{ event, value }] : []
  })
  if (diagnostics.length > 0) {
    lines.push('diagnostics:')
    for (const { event, value } of diagnostics) {
      lines.push(`- sequence=${event.sequenceId} event=${event.eventType} error_code=${value.errorCode} source=${value.source} operation=${value.operation}`)
    }
    lines.push('')
  }

  for (const event of events) {
    lines.push(`[${event.sequenceId}] ${event.eventType}${event.taskId ? ` task=${event.taskId}` : ''}`)
    lines.push(formatTraceEventPayload(event.eventType, event.payload))
    lines.push('')
  }
  return lines.join('\n')
}
