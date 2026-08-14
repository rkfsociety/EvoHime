import type { UpdateStatus } from '@shared/update'
import {
  activeUpdateStep,
  completedUpdateSteps,
  shortCommit,
  UPDATE_STEP_DESCRIPTIONS,
  updateProgress
} from '@shared/update'

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
  const activeStep = activeUpdateStep(status)
  const completed = completedUpdateSteps(status)
  const percent = progress === null ? null : Math.round(progress * 100)

  return (
    <div className="update-gate" role="dialog" aria-modal="true" aria-label="Обновление EvoHime">
      <div className="update-gate__panel">
        <p className="update-gate__eyebrow">Локальная сборка обновления</p>
        <h2 className="update-gate__title">Обновляю Еву</h2>
        <p className="update-gate__message">
          {status.message}
        </p>

        <div className="update-gate__meta">
          <span>
            Ветка <strong>{status.branch}</strong>
          </span>
          <span className="update-gate__commits">
            {shortCommit(status.installedCommit)} → {shortCommit(status.remoteCommit)}
          </span>
        </div>

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

        <div className="update-gate__progress-label">
          <strong>{percent === null ? 'Подготавливаю сборку' : `${percent}% готово`}</strong>
          <span>{completed} из {status.steps.length} этапов завершено</span>
        </div>

        {activeStep ? (
          <section className="update-gate__current" aria-label="Текущий этап">
            <span className="update-gate__current-marker" aria-hidden="true">◉</span>
            <div>
              <strong>{activeStep.label}</strong>
              <span>{UPDATE_STEP_DESCRIPTIONS[activeStep.id]}</span>
            </div>
          </section>
        ) : null}

        <ul className="update-steps">
          {status.steps.map((step) => (
            <li key={step.id} className="update-steps__item" data-state={step.state}>
              <span className="update-steps__marker" aria-hidden="true">
                {STEP_MARKERS[step.state] ?? '○'}
              </span>
              <span className="update-steps__copy">
                <strong>{step.label}</strong>
                <small>{UPDATE_STEP_DESCRIPTIONS[step.id]}</small>
              </span>
            </li>
          ))}
        </ul>

        <div className="update-gate__detail" aria-live="polite">
          <span>Последняя операция</span>
          <code>{status.detail || 'Ожидаю запуска этапа…'}</code>
        </div>

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
