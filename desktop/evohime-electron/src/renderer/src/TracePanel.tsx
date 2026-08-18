import { useEffect, useState } from 'react'

import type { ChatRecord, CoreEvent, ShellState } from '@shared/api'

import { useShellApi } from './shell-api'
import { filterEventsForChat } from './trace-filter'

export { filterEventsForChat } from './trace-filter'

interface Props {
  readonly chatId: string | null
  /** Reloads the persisted task ids after a prompt or task changes the chat. */
  readonly chatRevision?: number
  readonly events: readonly CoreEvent[]
  readonly state: ShellState | null
  readonly workspace: string | null
  readonly onClose: () => void
}

export function TracePanel({ chatId, chatRevision = 0, events, state, workspace, onClose }: Props): React.JSX.Element {
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
    const outcome = await api.invoke('trace.export', { content: formatTrace(state, workspace, traceEvents) })
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
          <div><dt>Core</dt><dd>{state?.coreVersion ?? '—'}</dd></div>
          <div><dt>Протокол</dt><dd>{state?.protocol ? `v${state.protocol.major}.${state.protocol.minor}` : '—'}</dd></div>
          <div><dt>Последний sequence</dt><dd>{state?.lastSequence ?? 0}</dd></div>
          <div><dt>Workspace</dt><dd title={workspace ?? undefined}>{workspace ?? 'не выбран'}</dd></div>
        </dl>
        {saveStatus ? <p className="trace-panel__reason" role="status">{saveStatus}</p> : null}
        {state?.reason ? <p className="trace-panel__reason">Причина: {state.reason}</p> : null}
        {chatId === null ? (
          <p className="trace-panel__empty">Выбери чат, чтобы открыть его трейс.</p>
        ) : traceEvents.length === 0 ? (
          <p className="trace-panel__empty">События появятся после запуска задачи в этом чате.</p>
        ) : (
          <ol className="trace-panel__events">
            {traceEvents.map((event) => (
              <li key={`${event.sequenceId}-${event.eventType}`} className="trace-event">
                <div className="trace-event__meta">
                  <code>{event.eventType}</code>
                  <span>#{event.sequenceId}</span>
                </div>
                {event.taskId ? <small className="trace-event__task">task: {event.taskId}</small> : null}
                <pre>{formatPayload(event.payload)}</pre>
              </li>
            ))}
          </ol>
        )}
    </aside>
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

export function formatTrace(state: ShellState | null, workspace: string | null, events: readonly CoreEvent[]): string {
  const lines = [
    'EvoHime trace',
    `captured_at: ${new Date().toISOString()}`,
    `connection: ${state?.connection ?? 'unknown'}`,
    `core_version: ${state?.coreVersion ?? 'unknown'}`,
    `protocol: ${state?.protocol ? `${state.protocol.major}.${state.protocol.minor}` : 'unknown'}`,
    `last_sequence: ${state?.lastSequence ?? 0}`,
    `reconnect_attempts: ${state?.reconnectAttempts ?? 0}`,
    `workspace: ${workspace ?? 'none'}`,
    `reason: ${state?.reason ?? 'none'}`,
    `events: ${events.length}`,
    ''
  ]

  for (const event of events) {
    lines.push(`[${event.sequenceId}] ${event.eventType}${event.taskId ? ` task=${event.taskId}` : ''}`)
    lines.push(formatPayload(event.payload))
    lines.push('')
  }
  return lines.join('\n')
}
