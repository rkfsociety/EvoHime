import { useEffect, useState } from 'react'

import type { ChatRecord, CoreEvent, ShellDiagnostic, ShellState } from '@shared/api'
import type { UpdateStatus } from '@shared/update'

import { useShellApi } from './shell-api'
import { filterEventsForChat } from './trace-filter'

export { filterEventsForChat } from './trace-filter'

export interface TraceDiagnostics {
  readonly errorCode: string
  readonly source: string
  readonly operation: string
  readonly pathForm?: string
  readonly pathScope?: string
  readonly pathBoundaryReason?: string
}

interface Props {
  readonly chatId: string | null
  /** Reloads the persisted task ids after a prompt or task changes the chat. */
  readonly chatRevision?: number
  readonly events: readonly CoreEvent[]
  readonly shellDiagnostics?: readonly ShellDiagnostic[]
  readonly state: ShellState | null
  readonly update?: UpdateStatus | null
  readonly workspace: string | null
  readonly onClose: () => void
}

export function TracePanel({ chatId, chatRevision = 0, events, shellDiagnostics = [], state, update = null, workspace, onClose }: Props): React.JSX.Element {
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
    const outcome = await api.invoke('trace.export', { content: formatTrace(state, workspace, traceEvents, update, shellDiagnostics) })
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
        <OllamaFallbackNotice events={traceEvents} shellDiagnostics={shellDiagnostics} />
        <ShellDiagnosticsView diagnostics={shellDiagnostics} />
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
      {diagnostics.pathForm ? <div><dt>Форма пути</dt><dd><code>{diagnostics.pathForm}</code></dd></div> : null}
      {diagnostics.pathScope ? <div><dt>Область пути</dt><dd><code>{diagnostics.pathScope}</code></dd></div> : null}
      {diagnostics.pathBoundaryReason ? <div><dt>Причина границы</dt><dd><code>{diagnostics.pathBoundaryReason}</code></dd></div> : null}
    </dl>
  )
}

function OllamaFallbackNotice({ events, shellDiagnostics }: { readonly events: readonly CoreEvent[]; readonly shellDiagnostics: readonly ShellDiagnostic[] }): React.JSX.Element | null {
  const ollamaFailure = events.some((event) => {
    if (event.eventType !== 'task.failed') return false
    const diagnostics = parseTraceDiagnostics(formatTraceEventPayload(event.eventType, event.payload))
    return diagnostics?.operation === 'ollama.download'
  })
  if (!ollamaFailure) return null
  const observed = shellDiagnostics.some((diagnostic) => diagnostic.event === 'shell.ollama_download_fallback')
  return <p className="trace-panel__reason" role="status">Ollama download fallback: {observed ? 'подтверждён' : 'не подтверждён в shell-событиях'}</p>
}

function ShellDiagnosticsView({ diagnostics }: { readonly diagnostics: readonly ShellDiagnostic[] }): React.JSX.Element | null {
  if (diagnostics.length === 0) return null
  return (
    <section className="trace-panel__shell-diagnostics" aria-label="События оболочки">
      <h3>События оболочки</h3>
      <ul>
        {diagnostics.map((diagnostic) => (
          <li key={diagnostic.event}>
            <code>{diagnostic.event}</code>
            <span>error_code={diagnostic.errorCode} · source={diagnostic.source} · operation={diagnostic.operation}</span>
          </li>
        ))}
      </ul>
    </section>
  )
}

const TRACE_URL_PATTERN = /\b(?:https?|wss?|file|ftp):\/\/[^\s"'<>]+/gi
const TRACE_SENSITIVE_FIELD_PATTERN = /^(?:prompt|secret|token|password|api[_-]?key|authorization|credential)$/i
const TRACE_SENSITIVE_ASSIGNMENT_PATTERN = /(["']?(?:prompt|secret|token|password|api[_-]?key|authorization|credential)["']?\s*[:=]\s*)(?:"[^"]*"|'[^']*'|[^\s,}]+)/gi

function formatPayload(payload: string): string {
  if (!payload) return 'без payload'
  try {
    return JSON.stringify(redactTraceValue(JSON.parse(payload)), null, 2) ?? '[REDACTED]'
  } catch {
    return '[REDACTED]'
  }
}

function redactTraceValue(value: unknown, key = ''): unknown {
  if (TRACE_SENSITIVE_FIELD_PATTERN.test(key)) return '[REDACTED]'
  if (typeof value === 'string') return redactTraceText(value)
  if (Array.isArray(value)) return value.map((item) => redactTraceValue(item))
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).map(([entryKey, entryValue]) => [entryKey, redactTraceValue(entryValue, entryKey)])
    )
  }
  return value
}

function redactTraceText(value: string): string {
  return value
    .replace(TRACE_URL_PATTERN, '[URL]')
    .replace(TRACE_SENSITIVE_ASSIGNMENT_PATTERN, '$1[REDACTED]')
}

function formatTraceEventPayload(eventType: string, payload: string): string {
  if (eventType !== 'task.failed') return formatPayload(payload)
  const diagnostics = parseTraceDiagnostics(payload) ?? {
    errorCode: 'task_failed',
    source: 'core',
    operation: 'task.execute'
  }
  const result: Record<string, unknown> = {
    redacted: true,
    conversation_projection: true,
    terminal: true,
    error_code: diagnostics.errorCode,
    source: diagnostics.source,
    operation: diagnostics.operation
  }
  if (diagnostics.pathForm) result.path_form = diagnostics.pathForm
  if (diagnostics.pathScope) result.path_scope = diagnostics.pathScope
  if (diagnostics.pathBoundaryReason) result.path_boundary_reason = diagnostics.pathBoundaryReason
  return JSON.stringify(result, null, 2)
}

export function parseTraceDiagnostics(payload: string): TraceDiagnostics | null {
  try {
    const value: unknown = JSON.parse(payload)
    if (!value || typeof value !== 'object' || Array.isArray(value)) return null
    const record = value as Record<string, unknown>
    const errorCode = safeTraceToken(record.error_code)
    const source = safeTraceToken(record.source ?? record.error_source)
    const operation = safeTraceToken(record.operation ?? record.operation_name ?? record.tool_name)
    if (!errorCode || !source || !operation) return null
    const pathForm = safeTraceToken(record.path_form)
    const pathScope = safeTraceToken(record.path_scope)
    const pathBoundaryReason = safeTraceToken(record.path_boundary_reason)
    return {
      errorCode,
      source,
      operation,
      ...(pathForm ? { pathForm } : {}),
      ...(pathScope ? { pathScope } : {}),
      ...(pathBoundaryReason ? { pathBoundaryReason } : {})
    }
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
  update: UpdateStatus | null = null,
  shellDiagnostics: readonly ShellDiagnostic[] = []
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

  appendTraceSummary(lines, events)

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

  const ollamaFailure = diagnostics.some(({ value }) => value.operation === 'ollama.download')
  if (ollamaFailure) {
    const observed = shellDiagnostics.some((diagnostic) => diagnostic.event === 'shell.ollama_download_fallback')
    lines.push('ollama_fallback:')
    lines.push(`- event=shell.ollama_download_fallback observed=${observed ? 'yes' : 'no'}`)
    lines.push('')
  }

  if (shellDiagnostics.length > 0) {
    lines.push('shell_diagnostics:')
    for (const diagnostic of shellDiagnostics) {
      lines.push(`- event=${diagnostic.event} error_code=${diagnostic.errorCode} source=${diagnostic.source} operation=${diagnostic.operation}`)
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

interface TraceToolSummary {
  readonly started: number
  readonly outputs: number
  readonly telemetry: number
  readonly ok: number
  readonly failed: number
}

function appendTraceSummary(lines: string[], events: readonly CoreEvent[]): void {
  const counts = new Map<string, number>()
  const tools = new Map<string, TraceToolSummary>()
  const routingStatuses = new Map<string, number>()
  const toolFailures: string[] = []
  const sequences = events.map((event) => event.sequenceId).filter(Number.isFinite)
  let completed = 0
  let failed = 0
  let stopped = 0

  for (const event of events) {
    counts.set(event.eventType, (counts.get(event.eventType) ?? 0) + 1)
    if (event.eventType === 'task.completed') completed += 1
    if (event.eventType === 'task.failed') failed += 1
    if (event.eventType === 'task.stopped') stopped += 1

    const payload = parseTraceObject(formatTraceEventPayload(event.eventType, event.payload))
    const tool = safeTraceToken(payload?.tool_name) ?? 'unknown'
    if (event.eventType === 'tool.started' || event.eventType === 'tool.output' || event.eventType === 'tool.telemetry') {
      const current = tools.get(tool) ?? { started: 0, outputs: 0, telemetry: 0, ok: 0, failed: 0 }
      const next = { ...current }
      if (event.eventType === 'tool.started') next.started += 1
      if (event.eventType === 'tool.output') next.outputs += 1
      if (event.eventType === 'tool.telemetry') {
        next.telemetry += 1
        if (payload?.ok === true) next.ok += 1
        if (payload?.ok === false) next.failed += 1
        if (payload?.ok === false && (payload?.path_form || payload?.path_scope || payload?.path_boundary_reason)) {
          const iteration = typeof payload.iteration === 'number' ? payload.iteration : 'unknown'
          const failureKind = safeTraceToken(payload.failure_kind) ?? 'unknown'
          const pathForm = safeTraceToken(payload.path_form) ?? 'unknown'
          const pathScope = safeTraceToken(payload.path_scope) ?? 'unknown'
          const boundaryReason = safeTraceToken(payload.path_boundary_reason) ?? 'unknown'
          toolFailures.push(`- tool=${tool} iteration=${iteration} failure_kind=${failureKind} path_form=${pathForm} path_scope=${pathScope} path_boundary_reason=${boundaryReason}`)
        }
      }
      tools.set(tool, next)
    }
    if (event.eventType === 'routing.terminal') {
      const status = safeTraceToken(payload?.terminal_status) ?? 'unknown'
      routingStatuses.set(status, (routingStatuses.get(status) ?? 0) + 1)
    }
  }

  const uniqueSequences = new Set(sequences)
  const firstSequence = sequences.length > 0 ? Math.min(...sequences) : 0
  const lastSequence = sequences.length > 0 ? Math.max(...sequences) : 0
  const contiguous = sequences.length > 0 && uniqueSequences.size === lastSequence - firstSequence + 1

  lines.push('summary:')
  lines.push(`- sequence_range=${firstSequence}..${lastSequence} unique=${uniqueSequences.size} contiguous=${contiguous ? 'yes' : 'no'}`)
  lines.push(`- task_outcome=completed:${completed} failed:${failed} stopped:${stopped}`)
  lines.push('event_counts:')
  for (const [eventType, count] of [...counts.entries()].sort(([left], [right]) => left.localeCompare(right))) {
    lines.push(`- ${eventType}=${count}`)
  }

  if (tools.size > 0) {
    lines.push('tool_summary:')
    for (const [tool, summary] of [...tools.entries()].sort(([left], [right]) => left.localeCompare(right))) {
      const pending = Math.max(summary.started - summary.outputs, 0)
      lines.push(`- tool=${tool} started=${summary.started} outputs=${summary.outputs} telemetry=${summary.telemetry} ok=${summary.ok} failed=${summary.failed} pending=${pending}`)
    }
  }

  if (toolFailures.length > 0) {
    lines.push('tool_failures:')
    lines.push(...toolFailures)
  }

  if (routingStatuses.size > 0) {
    lines.push('routing_statuses:')
    for (const [status, count] of [...routingStatuses.entries()].sort(([left], [right]) => left.localeCompare(right))) {
      lines.push(`- ${status}=${count}`)
    }
  }
  lines.push('')
}

function parseTraceObject(payload: string): Record<string, unknown> | null {
  try {
    const value: unknown = JSON.parse(payload)
    if (!value || typeof value !== 'object' || Array.isArray(value)) return null
    return value as Record<string, unknown>
  } catch {
    return null
  }
}
