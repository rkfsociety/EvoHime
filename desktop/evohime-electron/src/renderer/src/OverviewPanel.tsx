import { useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

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
  const [expanded, setExpanded] = useState<string | null>(null)
  const attention = events.filter((event) => IMPORTANT_EVENTS.has(event.eventType))
  const errors = events.filter((event) => event.eventType.includes('failed') || event.eventType.includes('error'))
  const groups = [...IMPORTANT_EVENTS].map((eventType) => ({
    eventType,
    label: LABELS[eventType],
    items: events.filter((event) => event.eventType === eventType)
  })).filter((group) => group.items.length > 0)

  return (
    <section className="panel overview-panel" aria-label="Обзор состояния">
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
        <article className={`overview-card ${errors.length > 0 ? 'overview-card--danger' : 'overview-card--ok'}`}>
          <span>Ошибки</span>
          <strong>{errors.length}</strong>
          <small>среди последних {events.length} событий</small>
        </article>
        <article className={`overview-card ${attention.length > 0 ? 'overview-card--warning' : 'overview-card--ok'}`}>
          <span>Внимание</span>
          <strong>{attention.length}</strong>
          <small>событий требуют проверки</small>
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
            {attention.length === 0
              ? 'Всё спокойно'
              : `${attention.length} сигналов · нажми группу, чтобы увидеть события`}
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
                        <li key={`${event.sequenceId}-${event.eventType}`}>
                          <span className="overview-group__sequence">#{event.sequenceId}</span>
                          <span className="overview-group__detail">{summarize(event.payload)}</span>
                          {event.taskId ? <small className="overview-group__task">task: {event.taskId}</small> : null}
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
          <span>новые сверху</span>
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
