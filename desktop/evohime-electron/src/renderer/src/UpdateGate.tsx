import type { UpdateStatus } from '@shared/update'
import { updateProgress } from '@shared/update'

import { useShellApi } from './shell-api'
import './UpdateSurface.css'

/**
 * Launch gate of the source updater.
 *
 * A local rebuild takes minutes, so the launch never looks frozen: the gate
 * names the running step, shows the last build line and lets the user drop the
 * update and start the installed build instead.
 */

const STEP_MARKERS: Record<string, string> = {
  pending: '○',
  active: '◉',
  done: '✓',
  failed: '✕',
  skipped: '—'
}

interface UpdateGateProps {
  readonly status: UpdateStatus
}

export function UpdateGate({ status }: UpdateGateProps): React.JSX.Element | null {
  const api = useShellApi()
  if (!status.blocking) return null

  const progress = updateProgress(status)

  return (
    <div className="update-gate" role="dialog" aria-modal="true" aria-label="Обновление EvoHime">
      <div className="update-gate__panel">
        <h2 className="update-gate__title">Обновляю Еву</h2>
        <p className="update-gate__message">{status.message}</p>

        <div
          className={`update-progress${progress === null ? ' update-progress--indeterminate' : ''}`}
          role="progressbar"
          aria-label="Прогресс пересборки"
          {...(progress === null
            ? {}
            : { 'aria-valuenow': Math.round(progress * 100), 'aria-valuemin': 0, 'aria-valuemax': 100 })}
        >
          <div className="update-progress__value" style={progress === null ? undefined : { width: `${progress * 100}%` }} />
        </div>

        <ul className="update-steps">
          {status.steps.map((step) => (
            <li key={step.id} className="update-steps__item" data-state={step.state}>
              <span className="update-steps__marker" aria-hidden="true">
                {STEP_MARKERS[step.state] ?? '○'}
              </span>
              {step.label}
            </li>
          ))}
        </ul>

        <p className="update-gate__detail">{status.detail}</p>

        <div className="update-gate__actions">
          <button
            type="button"
            onClick={() => {
              void api?.invoke('update.skip', {})
            }}
          >
            Пропустить и запустить
          </button>
        </div>
      </div>
    </div>
  )
}
