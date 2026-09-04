import type { UpdateStatus } from '@shared/update'
import {
  activeUpdateStep,
  completedUpdateSteps,
  shortCommit,
  UPDATE_STEP_DESCRIPTIONS,
  updateProgress
} from '@shared/update'

import './UpdateSurface.css'

/**
 * Launch gate of the source updater.
 *
 * A launch-time update is a first-class screen. It names the current stage,
 * shows the last operation and keeps the regular shell hidden until the
 * transaction is complete.
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
  if (!status.blocking) return null

  const progress = updateProgress(status)
  const activeStep = activeUpdateStep(status)
  const completed = completedUpdateSteps(status)
  const percent = progress === null ? null : Math.round(progress * 100)

  const applying = status.phase === 'applying'

  return (
    <main className="update-gate" aria-label="Обновление EvoHime">
      <div className="update-gate__orb update-gate__orb--one" aria-hidden="true" />
      <div className="update-gate__orb update-gate__orb--two" aria-hidden="true" />
      <div className="update-gate__panel">
        <header className="update-gate__header">
          <div className="update-gate__brand-mark" aria-hidden="true">E</div>
          <div>
            <p className="update-gate__brand">EvoHime</p>
            <p className="update-gate__eyebrow">Обновление приложения</p>
          </div>
          <span className="update-gate__live">
            <span className="update-gate__live-dot" aria-hidden="true" />
            {applying ? 'Завершаю' : 'Выполняю'}
          </span>
        </header>

        <div className="update-gate__intro">
          <p className="update-gate__kicker">Подожди немного</p>
          <h1 className="update-gate__title">{applying ? 'Завершаю обновление' : 'Обновляю Еву'}</h1>
          <p className="update-gate__message">{status.message}</p>
          <p className="update-gate__promise">Обычный интерфейс откроется после полного завершения обновления.</p>
        </div>

        <div className="update-gate__meta">
          <span>Ветка <strong>{status.branch}</strong></span>
          <span className="update-gate__commits">
            {shortCommit(status.installedCommit)} → {shortCommit(status.remoteCommit)}
          </span>
        </div>
        <p className="update-gate__components">
          Набор: <strong>{status.selectedComponents?.join(', ') || 'не выбран'}</strong>
          {status.totalBytes ? ` · ${status.downloadedBytes ?? 0} / ${status.totalBytes} байт` : ''}
        </p>

        <div
          className={`update-progress${progress === null ? ' update-progress--indeterminate' : ''}`}
          role="progressbar"
          aria-label="Прогресс обновления"
          {...(progress === null
            ? {}
            : { 'aria-valuenow': Math.round(progress * 100), 'aria-valuemin': 0, 'aria-valuemax': 100 })}
        >
          <div className="update-progress__value" style={progress === null ? undefined : { width: `${progress * 100}%` }} />
        </div>

        <div className="update-gate__progress-label">
          <strong>{percent === null ? 'Подготавливаю обновление' : `${percent}% готово`}</strong>
          <span>{completed} из {status.steps.length} этапов</span>
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
          <span>Текущая операция</span>
          <code>{status.detail || 'Ожидаю запуска этапа…'}</code>
        </div>
      </div>
    </main>
  )
}
