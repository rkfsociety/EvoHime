import { useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'
import { GoalPanel } from './GoalPanel'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly workspace: string | null
}

const LABELS: Record<string, string> = {
  'approval.required': 'Ожидают разрешения',
  'task.failed': 'Ошибки задач',
  'runtime.schedule_failed': 'Ошибки Pulse',
  'runtime.schedule_dead_letter': 'Dead-letter Pulse',
  'child.failed': 'Ошибки дочерних задач',
  'memory.pending': 'Кандидаты памяти',
  'workspace.index_status': 'Индекс workspace'
}

const IMPORTANT_EVENTS = new Set(Object.keys(LABELS))

/** Сколько событий показываем внутри раскрытой группы, не заливая экран. */
const GROUP_PREVIEW_LIMIT = 6

export function OverviewPanel({ connection, events, workspace }: Props): React.JSX.Element {
  const api = useShellApi()
  const [expanded, setExpanded] = useState<string | null>(null)
  const [details, setDetails] = useState<string | null>(null)
  const [copied, setCopied] = useState<string | null>(null)
  const attention = events.filter((event) => IMPORTANT_EVENTS.has(event.eventType))
  const currentAttention = attention.filter((event) => isCurrentSignal(event, events))
  const errors = events.filter((event) => event.eventType.includes('failed') || event.eventType.includes('error'))
  const currentErrors = errors.filter((event) => isCurrentSignal(event, events))
  const groups = [...IMPORTANT_EVENTS].map((eventType) => ({
    eventType,
    label: LABELS[eventType],
    items: events.filter((event) => event.eventType === eventType)
  })).filter((group) => group.items.length > 0)

  return (
    <section className="panel overview-panel" aria-label="Обзор состояния">
      <GoalPanel connection={connection} events={events} workspace={workspace} />
      <div className="panel__header overview-panel__heading">
        <div>
          <h2>Обзор</h2>
          <p>{workspace ? `Проект: ${projectName(workspace)}` : 'Проект не выбран'}</p>
        </div>
        <span className={`status-pill status-pill--${connection}`}>{connectionLabel(connection)}</span>
      </div>

      <div className="overview-grid">
        <article className={`overview-card ${connection === 'connected' ? 'overview-card--ok' : 'overview-card--warning'}`}>
          <span>Core</span>
          <strong>{connectionLabel(connection)}</strong>
          <small>состояние подключения</small>
        </article>
        <article className={`overview-card ${currentErrors.length > 0 ? 'overview-card--danger' : 'overview-card--ok'}`}>
          <span>Ошибки</span>
          <strong>{currentErrors.length}</strong>
          <small>текущих задач · {errors.length} в журнале</small>
        </article>
        <article className={`overview-card ${currentAttention.length > 0 ? 'overview-card--warning' : 'overview-card--ok'}`}>
          <span>Внимание</span>
          <strong>{currentAttention.length}</strong>
          <small>текущих сигналов · {attention.length} в журнале</small>
        </article>
        <article className="overview-card">
          <span>Лента</span>
          <strong>{events.length}</strong>
          <small>событий в памяти оболочки</small>
        </article>
      </div>

      <section className="overview-section" aria-label="Что требует внимания">
        <div className="overview-section__heading">
          <h3>Что требует внимания</h3>
          <span>
            {currentAttention.length === 0
              ? `${attention.length > 0 ? 'Текущих проблем нет · ' : ''}${attention.length} записей журнала`
              : `${currentAttention.length} текущих сигналов · ${attention.length} записей журнала`}
          </span>
        </div>
        {groups.length > 0 ? (
          <ul className="overview-list">
            {groups.map((group) => {
              const open = expanded === group.eventType
              const preview = group.items.slice(0, GROUP_PREVIEW_LIMIT)
              const hidden = group.items.length - preview.length
              return (
                <li key={group.eventType} className="overview-group">
                  <button
                    type="button"
                    className="overview-group__toggle"
                    aria-expanded={open}
                    onClick={() => setExpanded(open ? null : group.eventType)}
                  >
                    <span className="overview-group__chevron" aria-hidden="true">{open ? '▾' : '▸'}</span>
                    <span>{group.label}</span>
                    <code className="overview-group__type">{group.eventType}</code>
                    <strong>{group.items.length}</strong>
                  </button>
                  {open ? (
                    <ol className="overview-group__events">
                      {preview.map((event) => (
                        <li key={`${event.sequenceId}-${event.eventType}`} className={isCurrentSignal(event, events) ? 'overview-event--current' : 'overview-event--history'}>
                          <span className="overview-group__sequence">#{event.sequenceId}</span>
                          <span className="overview-group__detail">{summarize(event.payload)}</span>
                          {event.taskId ? <small className="overview-group__task">task: {event.taskId}</small> : null}
                          <span className="overview-group__state">{isCurrentSignal(event, events) ? 'текущее' : 'история'}</span>
                          <span className="overview-group__actions">
                            <button type="button" onClick={() => setDetails(details === eventKey(event) ? null : eventKey(event))}>
                              {details === eventKey(event) ? 'Скрыть' : 'Подробнее'}
                            </button>
                            <button type="button" onClick={() => {
                              if (!api) return
                              void api.writeClipboardText(formatEvent(event, isCurrentSignal(event, events))).then((ok) => {
                                if (!ok) return
                                setCopied(eventKey(event))
                                window.setTimeout(() => setCopied(null), 1400)
                              })
                            }}>
                              {copied === eventKey(event) ? 'Скопировано' : 'Копировать'}
                            </button>
                          </span>
                          {details === eventKey(event) ? <pre className="overview-group__payload">{formatPayload(event.payload)}</pre> : null}
                        </li>
                      ))}
                      {hidden > 0 ? (
                        <li className="overview-group__more">Ещё {hidden} — остальные смотри в трейсе чата.</li>
                      ) : null}
                    </ol>
                  ) : null}
                </li>
              )
            })}
          </ul>
        ) : (
          <p className="empty-state">Ошибок, ожидающих решений, и проблем расписаний не обнаружено.</p>
        )}
      </section>

      <section className="overview-section" aria-label="Последние события">
        <div className="overview-section__heading">
          <h3>Последние события</h3>
          <span>новые сверху · журнал, не список активных ошибок</span>
        </div>
        {events.length > 0 ? (
          <ol className="overview-events">
            {events.slice(0, 8).map((event) => (
              <li key={`${event.sequenceId}-${event.eventType}`}>
                <span className="overview-events__dot" aria-hidden="true" />
                <code>{event.eventType}</code>
                <span className="overview-events__detail">{summarize(event.payload)}</span>
                <span className="overview-events__sequence">#{event.sequenceId}</span>
              </li>
            ))}
          </ol>
        ) : (
          <p className="empty-state">События появятся после подключения Core.</p>
        )}
      </section>
    </section>
  )
}

/** Поля payload, которые чаще всего и объясняют, что случилось. */
const SUMMARY_FIELDS = ['message', 'error', 'reason', 'detail', 'details', 'summary', 'title', 'status', 'state', 'kind']

/** Одна строка про событие: понятное поле payload либо усечённый payload целиком. */
export function summarize(payload: string, limit = 140): string {
  if (!payload) return 'без payload'
  let parsed: unknown
  try {
    parsed = JSON.parse(payload)
  } catch {
    return truncate(payload.replace(/\s+/g, ' ').trim(), limit)
  }
  if (parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed)) {
    const record = parsed as Record<string, unknown>
    for (const field of SUMMARY_FIELDS) {
      const value = record[field]
      if (typeof value === 'string' && value.trim() !== '') return truncate(value.trim(), limit)
      if (typeof value === 'number' || typeof value === 'boolean') return truncate(`${field}: ${value}`, limit)
    }
  }
  return truncate(JSON.stringify(parsed) ?? String(parsed), limit)
}

function eventKey(event: CoreEvent): string {
  return `${event.sequenceId}-${event.eventType}`
}

/** A failed task is current only while it is the latest terminal state for that task. */
function isCurrentSignal(event: CoreEvent, events: readonly CoreEvent[]): boolean {
  if (event.eventType === 'task.failed') {
    const latestTerminal = events.find((candidate) =>
      candidate.taskId === event.taskId &&
      (candidate.eventType === 'task.completed' || candidate.eventType === 'task.failed' || candidate.eventType === 'task.stopped')
    )
    return latestTerminal === event
  }
  if (event.eventType === 'approval.required') {
    return !events.some((candidate) =>
      candidate.taskId === event.taskId &&
      candidate.sequenceId > event.sequenceId &&
      (candidate.eventType === 'task.completed' || candidate.eventType === 'task.failed' || candidate.eventType === 'task.stopped')
    )
  }
  // For operational signals there is no universal acknowledgement event.
  // Keep the latest record visible as current and older records as history.
  return events.find((candidate) => candidate.eventType === event.eventType && candidate.taskId === event.taskId) === event
}

function formatPayload(payload: string): string {
  try {
    const parsed: unknown = JSON.parse(payload)
    return JSON.stringify(parsed, null, 2) ?? payload
  } catch {
    return payload || 'без payload'
  }
}

function formatEvent(event: CoreEvent, current: boolean): string {
  return [
    `EvoHime diagnostic event`,
    `sequence: ${event.sequenceId}`,
    `type: ${event.eventType}`,
    `task: ${event.taskId || '—'}`,
    `state: ${current ? 'текущее' : 'история'}`,
    'payload:',
    formatPayload(event.payload)
  ].join('\n')
}

function truncate(text: string, limit: number): string {
  return text.length > limit ? `${text.slice(0, limit - 1)}…` : text
}

function projectName(workspace: string): string {
  return workspace.split(/[\\/]/).filter(Boolean).at(-1) ?? workspace
}

function connectionLabel(connection: ConnectionState): string {
  return {
    starting: 'Запуск', connecting: 'Подключение', connected: 'Подключено',
    reconnecting: 'Переподключение', replaying: 'Восстановление', resyncing: 'Синхронизация',
    'state-gap': 'Пробел состояния', 'version-mismatch': 'Несовместимая версия',
    degraded: 'Ограниченный режим', fatal: 'Критическая ошибка'
  }[connection]
}
