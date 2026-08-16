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

export function OverviewPanel({ connection, events, workspace }: Props): React.JSX.Element {
  const attention = events.filter((event) => IMPORTANT_EVENTS.has(event.eventType))
  const errors = events.filter((event) => event.eventType.includes('failed') || event.eventType.includes('error'))
  const counts = [...IMPORTANT_EVENTS].map((eventType) => ({
    eventType,
    label: LABELS[eventType],
    count: events.filter((event) => event.eventType === eventType).length
  })).filter((item) => item.count > 0)

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
          <span>{attention.length === 0 ? 'Всё спокойно' : `${attention.length} сигналов`}</span>
        </div>
        {counts.length > 0 ? (
          <ul className="overview-list">
            {counts.map((item) => (
              <li key={item.eventType}>
                <span>{item.label}</span>
                <strong>{item.count}</strong>
              </li>
            ))}
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
