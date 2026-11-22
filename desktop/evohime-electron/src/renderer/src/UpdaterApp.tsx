import { translate } from './i18n'
import { useEffect, useId, useMemo, useState } from 'react'

import type { UpdaterUiStatus } from '@shared/updater'
import { isDownloadPhaseMessage, isInstallPhaseMessage } from '@shared/updater'

import updaterEvaHeroUrl from './assets/updater-eva-hero.jpg'
import './UpdaterSurface.css'

const fallback: UpdaterUiStatus = {
  phase: 'checking',
  heading: 'Проверяю обновления',
  badge: 'Проверка',
  message: 'Сверяю версии и целостность компонентов EvoHime…',
  detail: '',
  percent: null,
  modules: [],
  canApply: false
}

const SPINNER_RADIUS = 16
const SPINNER_CIRCUMFERENCE = 2 * Math.PI * SPINNER_RADIUS

type UpdaterStepId = 'check' | 'download' | 'install'
type UpdaterStepState = 'done' | 'active' | 'pending' | 'error'

interface UpdaterStep {
  readonly id: UpdaterStepId
  readonly label: string
  readonly state: UpdaterStepState
}

function resolveSteps(status: UpdaterUiStatus): readonly UpdaterStep[] {
  const { phase, message, percent, modules } = status
  const hasAvailableModules = modules.some((module) => module.available !== null)

  let check: UpdaterStepState = 'pending'
  let download: UpdaterStepState = 'pending'
  let install: UpdaterStepState = 'pending'

  if (phase === 'checking') {
    check = 'active'
  } else if (phase === 'available') {
    check = 'done'
    download = 'active'
  } else if (phase === 'applying') {
    check = 'done'
    const installing = isInstallPhaseMessage(message) || (!isDownloadPhaseMessage(message) && percent !== null && percent >= 80)
    if (installing) {
      download = 'done'
      install = 'active'
    } else {
      download = 'active'
    }
  } else if (phase === 'recovering') {
    check = 'done'
    download = 'done'
    install = 'active'
  } else if (phase === 'ready') {
    check = 'done'
    download = 'done'
    install = 'done'
  } else {
    if (!hasAvailableModules && percent === null && !isDownloadPhaseMessage(message) && !isInstallPhaseMessage(message)) {
      check = 'error'
    } else if (isInstallPhaseMessage(message) || (percent !== null && percent >= 80)) {
      check = 'done'
      download = 'done'
      install = 'error'
    } else {
      check = 'done'
      download = 'error'
    }
  }

  return [
    { id: 'check', label: 'Проверка', state: check },
    { id: 'download', label: 'Загрузка', state: download },
    { id: 'install', label: 'Установка', state: install }
  ]
}

function StepIcon({ id }: { readonly id: UpdaterStepId }): React.JSX.Element {
  if (id === 'check') {
    return (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="11" cy="11" r="6" />
        <path d="m15.5 15.5 4 4" />
      </svg>
    )
  }
  if (id === 'download') {
    return (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
        <path d="M12 4v10" />
        <path d="m8 10 4 4 4-4" />
        <path d="M5 17v1a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-1" />
      </svg>
    )
  }
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round">
      <path d="m12 3 8 4.5-8 4.5-8-4.5Z" />
      <path d="m4 12 8 4.5 8-4.5" />
      <path d="m4 16.5 8 4.5 8-4.5" />
    </svg>
  )
}

function StatusSpinner({ percent }: { readonly percent: number | null }): React.JSX.Element {
  const clamped = percent === null ? null : Math.max(0, Math.min(100, percent))
  const dashOffset = clamped === null ? SPINNER_CIRCUMFERENCE * 0.72 : SPINNER_CIRCUMFERENCE * (1 - clamped / 100)

  return (
    <svg
      className={`updater-status-spinner${clamped === null ? ' updater-status-spinner--indeterminate' : ''}`}
      viewBox="0 0 40 40"
      aria-hidden="true"
    >
      <circle className="updater-status-spinner__track" cx="20" cy="20" r={SPINNER_RADIUS} />
      <circle
        className="updater-status-spinner__arc"
        cx="20"
        cy="20"
        r={SPINNER_RADIUS}
        strokeDasharray={SPINNER_CIRCUMFERENCE}
        strokeDashoffset={dashOffset}
      />
    </svg>
  )
}

function UpdaterLotusLogo(): React.JSX.Element {
  const gradientId = useId()
  const centerId = `${gradientId}-center`
  const wingId = `${gradientId}-wing`

  return (
    <svg viewBox="0 0 48 48" fill="none">
      <defs>
        <linearGradient id={centerId} x1="24" y1="7" x2="24" y2="39" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#ffffff" />
          <stop offset="55%" stopColor="#ddd6fe" />
          <stop offset="100%" stopColor="#93c5fd" />
        </linearGradient>
        <linearGradient id={wingId} x1="8" y1="16" x2="40" y2="38" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#e9d5ff" stopOpacity="0.92" />
          <stop offset="100%" stopColor="#a7f3d0" stopOpacity="0.72" />
        </linearGradient>
      </defs>
      <path d="M24 7C28.5 14 30 22 24 34C18 22 19.5 14 24 7Z" fill={`url(#${centerId})`} />
      <path d="M12 16C18.5 18 22 24.5 23.5 34.5C15.5 31.5 11.5 24.5 12 16Z" fill={`url(#${wingId})`} />
      <path d="M36 16C29.5 18 26 24.5 24.5 34.5C32.5 31.5 36.5 24.5 36 16Z" fill={`url(#${wingId})`} />
      <path d="M7 25.5C13.5 25.5 18.5 29.5 23 35.5C15 36.5 9.5 32.5 7 25.5Z" fill="#ddd6fe" fillOpacity="0.68" />
      <path d="M41 25.5C34.5 25.5 29.5 29.5 25 35.5C33 36.5 38.5 32.5 41 25.5Z" fill="#93c5fd" fillOpacity="0.68" />
    </svg>
  )
}

export function UpdaterApp(): React.JSX.Element {
  const [status, setStatus] = useState<UpdaterUiStatus>(fallback)

  useEffect(() => {
    let active = true
    const api = window.evohimeUpdater
    if (!api) {
      setStatus({
        ...fallback,
        phase: 'failed',
        heading: 'Интерфейс обновления не запустился',
        badge: 'Ошибка',
        message: 'Не удалось подключить окно updater. Закройте его и повторите запуск.'
      })
      return () => { active = false }
    }
    void api.getStatus().then((next) => {
      if (active) setStatus(next)
    })
    return api.subscribe((next) => setStatus(next))
  }, [])

  const progressStyle = useMemo(() => {
    const percent = status.phase === 'ready' ? 100 : status.percent
    return percent === null ? undefined : { width: `${percent}%` }
  }, [status.percent, status.phase])

  const failed = status.phase === 'failed'
  const attention = failed || status.phase === 'manual-recovery'
  const ready = status.phase === 'ready'
  const progressPercent = ready ? 100 : status.percent
  const progressLabel = progressPercent === null ? 'Подготавливаю' : `${progressPercent}%`
  const steps = useMemo(() => resolveSteps(status), [status])
  const updatableModules = useMemo(
    () => status.modules.filter((module) => module.available !== null),
    [status.modules]
  )
  const visibleModules = updatableModules.slice(0, 4)
  const extraModulesCount = updatableModules.length - visibleModules.length

  return (
    <main className="updater-shell">
      <header className="updater-titlebar">
        <div className="updater-titlebar__drag">
          <span className="updater-brand-mark" aria-hidden="true">
            <UpdaterLotusLogo />
          </span>
          <span className="updater-brand-copy">
            <strong>EvoHime</strong>
            <span>{translate('Центр обновлений')}</span>
          </span>
        </div>
        <div className="updater-window-actions">
          <button type="button" aria-label={translate("Свернуть")} onClick={() => void window.evohimeUpdater.minimize()}>—</button>
          <button type="button" aria-label={translate("Закрыть")} onClick={() => void window.evohimeUpdater.close()}>×</button>
        </div>
      </header>

      <section className={`updater-content updater-content--${status.phase}`} aria-labelledby="updater-heading">
        <div className="updater-hero" aria-hidden="true">
          <img
            className="updater-hero-art"
            src={updaterEvaHeroUrl}
            alt=""
            draggable={false}
            data-testid="updater-hero-art"
          />
        </div>

        <div className="updater-panel">
          <div className="updater-panel__head">
            <div className="updater-status-icon" data-status-phase={status.phase} aria-hidden="true">
              {ready ? '✓' : attention ? '!' : <StatusSpinner percent={progressPercent} />}
            </div>
            <div className="updater-panel__titles">
              <h1 id="updater-heading">{translate(status.heading)}</h1>
              <p className="updater-message" aria-live="polite">{translate(status.message)}</p>
            </div>
          </div>

          <div className="updater-progress-block">
            <div className="updater-progress-meta">
              <span className="updater-progress__badge">{translate(status.badge)}</span>
              <span className="updater-progress__label">{translate(progressLabel)}</span>
            </div>
            <div
              className={`updater-progress${progressPercent === null ? ' updater-progress--indeterminate' : ''}`}
              role="progressbar"
              aria-label={translate("Прогресс обновления")}
              {...(progressPercent === null ? {} : { 'aria-valuenow': progressPercent, 'aria-valuemin': 0, 'aria-valuemax': 100 })}
            >
              <div className="updater-progress__value" style={progressStyle} />
            </div>
          </div>

          {failed && status.detail.length > 0 ? <p className="updater-detail" role="status">{translate(status.detail)}</p> : null}

          {visibleModules.length > 0 ? (
            <div className="updater-modules" aria-label={translate("Обновляемые модули")}>
              {visibleModules.map((module) => (
                <span
                  key={module.id}
                  className="updater-module-chip"
                  title={`${module.label}: ${module.installed} → ${module.available}`}
                >
                  {translate(module.label)}
                </span>
              ))}
              {extraModulesCount > 0 ? (
                <span className="updater-module-chip updater-module-chip--more">+{extraModulesCount}</span>
              ) : null}
            </div>
          ) : null}

          <ol className="updater-steps" aria-label={translate("Этапы обновления")}>
            {steps.map((step) => (
              <li
                key={step.id}
                className={`updater-step updater-step--${step.state}`}
                data-step-id={step.id}
                data-step-state={step.state}
              >
                <span className="updater-step__icon" aria-hidden="true">
                  <StepIcon id={step.id} />
                </span>
                <span className="updater-step__label">{translate(step.label)}</span>
              </li>
            ))}
          </ol>
        </div>
      </section>
    </main>
  )
}
