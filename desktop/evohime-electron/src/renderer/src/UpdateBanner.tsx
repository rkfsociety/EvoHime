import type { UpdateStatus } from '@shared/update'
import { shortCommit, updateProgress } from '@shared/update'

import { useShellApi } from './shell-api'
import './UpdateSurface.css'

/**
 * Update notice for an already running shell.
 *
 * The background pass never interrupts work: it says an update exists, shows the
 * rebuild running, and offers the restart only once a package is staged.
 */

const VISIBLE_PHASES = ['available', 'preparing', 'ready', 'applying', 'failed'] as const

interface UpdateBannerProps {
  readonly status: UpdateStatus
}

export function UpdateBanner({ status }: UpdateBannerProps): React.JSX.Element | null {
  const api = useShellApi()
  if (status.blocking || !(VISIBLE_PHASES as readonly string[]).includes(status.phase)) {
    return null
  }

  const progress = updateProgress(status)
  const showProgress = status.phase === 'preparing'

  return (
    <section
      className={`update-banner${status.phase === 'failed' ? ' update-banner--failed' : ''}`}
      aria-label="Обновление EvoHime"
    >
      <span aria-hidden="true">{status.phase === 'failed' ? '⚠' : '⟳'}</span>
      <span className="update-banner__text">
        <span className="update-banner__message">
          {status.error ?? status.message}
          {status.remoteCommit && status.remoteCommit !== status.installedCommit ? (
            <span className="update-banner__commits">
              {' '}
              {shortCommit(status.installedCommit)} → {shortCommit(status.remoteCommit)}
            </span>
          ) : null}
        </span>
        {status.detail ? <span className="update-banner__detail">{status.detail}</span> : null}
        {showProgress ? (
          <span
            className={`update-progress${progress === null ? ' update-progress--indeterminate' : ''}`}
            role="progressbar"
            aria-label="Прогресс пересборки"
          >
            <span
              className="update-progress__value"
              style={progress === null ? undefined : { width: `${progress * 100}%` }}
            />
          </span>
        ) : null}
      </span>

      {status.phase === 'ready' ? (
        <button
          type="button"
          onClick={() => {
            void api?.invoke('update.restart', {})
          }}
        >
          Перезапустить
        </button>
      ) : null}

      {status.phase === 'available' || status.phase === 'failed' ? (
        <button
          type="button"
          onClick={() => {
            void api?.invoke('update.prepare', {})
          }}
        >
          {status.phase === 'failed' ? 'Повторить' : 'Обновить'}
        </button>
      ) : null}
    </section>
  )
}
