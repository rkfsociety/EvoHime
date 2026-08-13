import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'
import { latestRecoveryNotice } from './recovery-state'
import './RecoveryBanner.css'

interface RecoveryBannerProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly onOpenTask: () => void
}

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

export function RecoveryBanner({ connection, events, onOpenTask }: RecoveryBannerProps): React.JSX.Element | null {
  const api = useShellApi()
  const notice = latestRecoveryNotice(events)
  if (!notice) return null

  const retry = async (): Promise<void> => {
    if (!api || !CONNECTED_STATES.includes(connection)) return
    await api.invoke('shell.requestResync', {})
  }

  return (
    <section className={`recovery-banner recovery-banner--${notice.state.toLowerCase()}`} role="status" aria-label={`Состояние восстановления: ${notice.state}`}>
      <div>
        <strong>{notice.state}</strong>
        <p>{notice.reason}</p>
        <small>Операция: {notice.correlationId}{notice.phase ? ` · Фаза: ${notice.phase}` : ''}</small>
      </div>
      <div className="recovery-banner__actions">
        {notice.state === 'WAITING_APPROVAL' ? (
          <button type="button" onClick={onOpenTask}>Открыть подтверждение</button>
        ) : null}
        {notice.state === 'BLOCKED' || notice.state === 'FAILED' ? (
          <button type="button" onClick={() => void retry()} disabled={!CONNECTED_STATES.includes(connection)}>Перезапросить состояние</button>
        ) : null}
        {notice.state === 'FAILED' ? (
          <button type="button" onClick={onOpenTask}>Открыть детали</button>
        ) : null}
      </div>
    </section>
  )
}
