import type { ConnectionState, CoreEvent } from '@shared/api'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

/** Read-only projection of Core-owned memory/child/schedule events. */
export function OperationsPanel({ connection, events }: Props): React.JSX.Element {
  const count = (name: string) => events.filter((event) => event.eventType === name).length
  const childEvents = events.filter((event) => event.eventType.startsWith('child.'))
  const pulseFailed = count('runtime.schedule_failed') + count('runtime.schedule_dead_letter')

  return (
    <section className="panel operations-panel" aria-label="Память и автоматизация">
      <div className="panel__header">
        <div>
          <h2>Память и автоматизация</h2>
          <p>Только состояние, подтверждённое Core; локальные события не подменяются успехом.</p>
        </div>
        <span className={`status-pill status-pill--${connection}`}>{connection}</span>
      </div>
      <div className="operations-grid">
        <article className="operations-card">
          <h3>Memory v1</h3>
          <strong>{count('memory.candidate.created')}</strong>
          <span>новых кандидатов</span>
          <small>Подтверждение нужно до сохранения важной записи</small>
        </article>
        <article className="operations-card">
          <h3>Child jobs</h3>
          <strong>{childEvents.length}</strong>
          <span>событий timeline</span>
          <small>{count('child.report.accepted')} принятых отчётов · {count('child.failed')} failed</small>
        </article>
        <article className={`operations-card ${pulseFailed ? 'operations-card--warning' : ''}`}>
          <h3>Pulse</h3>
          <strong>{pulseFailed ? 'Внимание' : 'OK'}</strong>
          <span>{pulseFailed ? 'есть ошибки расписаний' : 'ошибок не обнаружено'}</span>
          <small>{count('runtime.schedule_completed')} completed · {count('runtime.schedule_requeued')} requeued · {count('runtime.schedule_dead_letter')} dead-letter</small>
        </article>
      </div>
      {childEvents.length > 0 ? (
        <ol className="operations-timeline" aria-label="Последние child события">
          {childEvents.slice(0, 8).map((event) => (
            <li key={`${event.sequenceId}-${event.eventType}`}>
              <code>{event.eventType}</code>
              <span>{event.payload || 'без дополнительной информации'}</span>
            </li>
          ))}
        </ol>
      ) : <p className="empty-state">Child timeline появится после запуска bounded read-only задачи.</p>}
    </section>
  )
}
