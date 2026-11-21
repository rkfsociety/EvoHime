import { translate } from './i18n'
import { useEffect, useMemo, useState } from 'react'

import type { UpdaterUiStatus } from '@shared/updater'

import './UpdaterSurface.css'
import { EvaIcon } from './EvaIcon'

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

  return (
    <main className="updater-shell">
      <header className="updater-titlebar">
        <div className="updater-titlebar__drag">
          <span className="updater-brand-mark" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="none">
              <defs>
                <linearGradient id="updater-brand-petal" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="#ffffff" />
                  <stop offset="100%" stopColor="#d9ccff" />
                </linearGradient>
              </defs>
              <path
                d="M12 13C10.2 10.6 7.4 8.2 6.8 5.2C6.4 3.5 8 2.6 9.5 3.6C11.1 4.7 11.9 8.2 12 13Z"
                fill="url(#updater-brand-petal)"
              />
              <path
                d="M12 13C13.8 10.6 16.6 8.2 17.2 5.2C17.6 3.5 16 2.6 14.5 3.6C12.9 4.7 12.1 8.2 12 13Z"
                fill="url(#updater-brand-petal)"
              />
              <path
                d="M12 13C10.1 15.1 9.4 18.4 10.8 20.3C11.4 21.1 12.6 21.1 13.2 20.3C14.6 18.4 13.9 15.1 12 13Z"
                fill="url(#updater-brand-petal)"
              />
            </svg>
          </span>
          <span className="updater-brand-copy">
            <strong>EvoHime</strong>
            <span>AI COMPANION FOR A BRIGHTER YOU</span>
          </span>
        </div>
        <div className="updater-window-actions">
          <button type="button" aria-label={translate("Свернуть")} onClick={() => void window.evohimeUpdater.minimize()}>—</button>
          <button type="button" aria-label={translate("Закрыть")} onClick={() => void window.evohimeUpdater.close()}>×</button>
        </div>
      </header>

      <section className={`updater-content updater-content--${status.phase}`} aria-labelledby="updater-heading">
        <div className="updater-hero" aria-hidden="true">
          <EvaIcon className="updater-hero-art" />
          <div className="updater-hero-glow updater-hero-glow--left" />
          <div className="updater-hero-glow updater-hero-glow--right" />
        </div>

        <div className="updater-status-orbit" aria-hidden="true">
          <div className="updater-status-orbit__ring" />
          <div className="updater-status-icon" data-status-phase={status.phase}>
            {ready ? '✓' : attention ? '!' : <StatusSpinner percent={progressPercent} />}
          </div>
        </div>

        <div className="updater-panel">
          <h1 id="updater-heading">{translate(status.heading)}</h1>
          <p className="updater-message" aria-live="polite">{translate(status.message)}</p>

          <div className="updater-progress-block">
            <div className="updater-progress-row">
              <div className={`updater-progress${progressPercent === null ? ' updater-progress--indeterminate' : ''}`} role="progressbar" aria-label={translate("Прогресс обновления")} {...(progressPercent === null ? {} : { 'aria-valuenow': progressPercent, 'aria-valuemin': 0, 'aria-valuemax': 100 })}>
                <div className="updater-progress__value" style={progressStyle} />
              </div>
              <span className="updater-progress__label">{translate(progressLabel)}</span>
            </div>
          </div>

          {failed && status.detail.length > 0 ? <p className="updater-detail" role="status">{translate(status.detail)}</p> : null}

          <div className="updater-features" aria-hidden="true">
            <div className="updater-feature"><span>♢</span><p>{translate("Безопасное")}<br />{translate("обновление")}</p></div>
            <div className="updater-feature"><span>ϟ</span><p>{translate("Новые")}<br />{translate("возможности")}</p></div>
            <div className="updater-feature"><span>♡</span><p>{translate("Спасибо,")}<br />{translate("что ты с нами")}</p></div>
          </div>

          <p className="updater-footer">EVOHIME <span>•</span> {translate("РАЗВИВАЕМСЯ ВМЕСТЕ")}<span>•</span> {translate("С КАЖДЫМ ОБНОВЛЕНИЕМ")}</p>
        </div>
      </section>
    </main>
  )
}
