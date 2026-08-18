import { useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'
import { latestRecoveryNotice } from './recovery-state'
import './RecoveryBanner.css'

interface RecoveryBannerProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly onOpenTask: () => void
  readonly showOpenTask?: boolean
}

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

export function RecoveryBanner({ connection, events, onOpenTask, showOpenTask = true }: RecoveryBannerProps): React.JSX.Element | null {
  const api = useShellApi()
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState<string | null>(null)
  const [detailsOf, setDetailsOf] = useState<string | null>(null)
  // Уведомление живёт в ленте событий и само не исчезает, поэтому закрытие
  // помнится по correlationId: иначе кнопки выглядят так, будто ничего не делают.
  const [dismissedId, setDismissedId] = useState<string | null>(null)

  const notice = latestRecoveryNotice(events)
  if (!notice || notice.correlationId === dismissedId) return null

  const connected = CONNECTED_STATES.includes(connection)
  const detailsOpen = detailsOf === notice.correlationId

  const retry = async (): Promise<void> => {
    if (!api) {
      setStatus('Мост оболочки недоступен: перезапусти приложение.')
      return
    }
    if (!connected) {
      setStatus('Core не подключён: запрос состояния невозможен.')
      return
    }
    setBusy(true)
    setStatus('Запрашиваю состояние у Core…')
    const outcome = await api.invoke('shell.requestResync', {})
    setBusy(false)
    if (!outcome.ok) {
      setStatus(`Core отклонил запрос: ${outcome.message}`)
      return
    }
    if (!outcome.value.accepted) {
      setStatus('Очередь Core переполнена, повтори чуть позже.')
      return
    }
    setStatus('Состояние запрошено — жду события от Core.')
  }

  const openDetails = (): void => {
    setDetailsOf(detailsOpen ? null : notice.correlationId)
    onOpenTask()
  }

  return (
    <section className={`recovery-banner recovery-banner--${notice.state.toLowerCase()}`} role="status" aria-label={`Состояние восстановления: ${notice.state}`}>
      <div className="recovery-banner__body">
        <strong>{notice.state}</strong>
        <p>{notice.reason}</p>
        <small>Операция: {notice.correlationId}{notice.phase ? ` · Фаза: ${notice.phase}` : ''}</small>
        {status ? <p className="recovery-banner__status" role="status">{status}</p> : null}
        {detailsOpen ? (
          <dl className="recovery-banner__details">
            <dt>Событие</dt>
            <dd>{notice.eventType} · seq {notice.sequenceId}</dd>
            <dt>Задача</dt>
            <dd>{notice.taskId || '—'}</dd>
            {Object.entries(notice.details).map(([key, value]) => (
              <div key={key} className="recovery-banner__detail">
                <dt>{key}</dt>
                <dd>{formatValue(value)}</dd>
              </div>
            ))}
          </dl>
        ) : null}
      </div>
      <div className="recovery-banner__actions">
        {notice.state === 'WAITING_APPROVAL' && showOpenTask ? (
          <button type="button" onClick={onOpenTask}>Открыть подтверждение</button>
        ) : null}
        {notice.state === 'BLOCKED' || notice.state === 'FAILED' ? (
          <button type="button" onClick={() => void retry()} disabled={busy}>Перезапросить состояние</button>
        ) : null}
        {notice.state === 'FAILED' ? (
          <button type="button" aria-expanded={detailsOpen} onClick={openDetails}>
            {detailsOpen ? 'Скрыть детали' : 'Открыть детали'}
          </button>
        ) : null}
        {notice.state === 'BLOCKED' || notice.state === 'FAILED' ? (
          <button
            type="button"
            className="recovery-banner__dismiss"
            aria-label="Скрыть уведомление"
            onClick={() => setDismissedId(notice.correlationId)}
          >
            ✕
          </button>
        ) : null}
      </div>
    </section>
  )
}

/** Payload значения уже отредактированы Core, показываем их как есть. */
function formatValue(value: unknown): string {
  if (typeof value === 'string') return value
  return JSON.stringify(value) ?? String(value)
}
