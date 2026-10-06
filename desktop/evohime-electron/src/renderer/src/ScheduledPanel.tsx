import { translate } from './i18n'
import { useCallback, useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'

interface ScheduledPanelProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly workspace: string | null
}

interface ScheduleProjection {
  readonly schedule_id: string
  readonly definition_id: string
  readonly revision: number
  readonly hour: number
  readonly minute: number
  readonly timezone_minutes: number
  readonly enabled: boolean
  readonly last_slot?: string | null
  readonly workspace_path?: string | null
}

interface ScheduleListProjection {
  readonly schedules?: readonly ScheduleProjection[]
  readonly error_code?: string
}

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

export function ScheduledPanel({ connection, events, workspace }: ScheduledPanelProps): React.JSX.Element {
  const api = useShellApi()
  const [message, setMessage] = useState<string | null>(null)
  const [requested, setRequested] = useState(false)
  const connected = CONNECTED_STATES.includes(connection)
  const ownerScope = workspace ?? 'user'

  const scheduleEvent = useMemo(
    () => events.find((event) => event.eventType === 'automation.schedules'),
    [events]
  )
  const projection = useMemo(() => parseProjection(scheduleEvent), [scheduleEvent])
  const schedules = projection?.schedules ?? []

  const refresh = useCallback(() => {
    if (!api || !connected) return
    setRequested(true)
    void api.invoke('automation.listSchedules', { ownerScope, limit: 64 }).then((outcome) => {
      if (!outcome.ok) setMessage(outcome.message)
    })
  }, [api, connected, ownerScope])

  useEffect(() => {
    refresh()
  }, [refresh])

  const setEnabled = useCallback(async (schedule: ScheduleProjection) => {
    if (!api || !connected) return
    const outcome = await api.invoke('automation.setScheduleEnabled', {
      scheduleId: schedule.schedule_id,
      enabled: !schedule.enabled
    })
    setMessage(outcome.ok ? (schedule.enabled ? 'Расписание приостановлено.' : 'Расписание включено.') : outcome.message)
    if (outcome.ok) refresh()
  }, [api, connected, refresh])

  return (
    <section className="panel scheduled-panel" aria-label={translate("Запланированные задачи")}>
      <header className="scheduled-panel__header">
        <div>
          <p className="panel__eyebrow">Pulse / Automation</p>
          <h2>{translate("Запланировано")}</h2>
          <p>{translate("Сохранённые расписания запускаются Core с контролем состояния и повторов.")}</p>
        </div>
        <div className="scheduled-panel__header-actions">
          <span className={`status-pill status-pill--${connection}`}>{translate(connection)}</span>
          <button type="button" className="button" onClick={refresh} disabled={!connected}>
            {translate("Обновить")}</button>
        </div>
      </header>

      <div className="scheduled-panel__scope">
        <span className="scheduled-panel__scope-dot" aria-hidden="true" />
        <span>{translate("Область расписаний")}</span>
        <code title={ownerScope}>{translate(workspace ? basename(workspace) : translate("пользовательская"))}</code>
      </div>

      {!connected ? <p className="empty-state">{translate("Нет подключения к Core — расписания будут доступны после подключения.")}</p> : null}
      {connected && projection?.error_code ? <p role="alert" className="shell__reason">Core: {translate(projection.error_code)}</p> : null}
      {connected && requested && schedules.length === 0 && !projection?.error_code ? (
        <div className="scheduled-panel__empty">
          <span className="scheduled-panel__empty-icon" aria-hidden="true">◷</span>
          <h3>{translate("Расписаний пока нет")}</h3>
          <p>{translate("Создай расписание из составной задачи — здесь появится его состояние.")}</p>
        </div>
      ) : null}

      {schedules.length > 0 ? (
        <div className="scheduled-list">
          {schedules.map((schedule) => (
            <article className={`scheduled-card${schedule.enabled ? '' : ' scheduled-card--muted'}`} key={schedule.schedule_id}>
              <div className="scheduled-card__icon" aria-hidden="true">◷</div>
              <div className="scheduled-card__body">
                <div className="scheduled-card__title-row">
                  <h3>{translate(schedule.definition_id)}</h3>
                  <span className={`scheduled-card__state${schedule.enabled ? ' scheduled-card__state--active' : ''}`}>
                    {translate(schedule.enabled ? translate("активно") : translate("приостановлено"))}
                  </span>
                </div>
                <p>{translate(formatTime(schedule.hour, schedule.minute))} · {translate(formatTimezone(schedule.timezone_minutes))} {translate("· ревизия")}{translate(schedule.revision)}</p>
                <small>{translate(schedule.last_slot ? `Последний слот: ${schedule.last_slot}` : translate("Ещё не запускалось"))}</small>
              </div>
              <button type="button" className="button" onClick={() => void setEnabled(schedule)}>
                {translate(schedule.enabled ? translate("Пауза") : translate("Включить"))}
              </button>
            </article>
          ))}
        </div>
      ) : null}

      {message ? <p className="scheduled-panel__message" role="status">{translate(message)}</p> : null}
    </section>
  )
}

function parseProjection(event: CoreEvent | undefined): ScheduleListProjection | null {
  if (!event) return null
  try {
    return JSON.parse(event.payload) as ScheduleListProjection
  } catch {
    return null
  }
}

function formatTime(hour: number, minute: number): string {
  return `${String(hour).padStart(2, '0')}:${String(minute).padStart(2, '0')}`
}

function formatTimezone(minutes: number): string {
  const sign = minutes >= 0 ? '+' : '-'
  const absolute = Math.abs(minutes)
  const hours = Math.floor(absolute / 60)
  const rest = absolute % 60
  return `UTC${sign}${String(hours).padStart(2, '0')}:${String(rest).padStart(2, '0')}`
}

function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter((part) => part.length > 0)
  return parts.at(-1) ?? path
}
