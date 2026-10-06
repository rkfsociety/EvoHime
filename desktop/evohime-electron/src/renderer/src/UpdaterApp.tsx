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
  const progressPercent = status.phase === 'ready' ? 100 : status.percent
  const icon = failed ? '!' : '✓'
  const progressLabel = progressPercent === null ? 'Подготавливаю' : `${progressPercent}%`

  return (
    <main className="updater-shell">
      <header className="updater-titlebar">
        <div className="updater-titlebar__drag">
          <span className="updater-brand-mark" aria-hidden="true">
            <span className="updater-brand-mark__petal updater-brand-mark__petal--one" />
            <span className="updater-brand-mark__petal updater-brand-mark__petal--two" />
            <span className="updater-brand-mark__petal updater-brand-mark__petal--three" />
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
          <div className="updater-hero-copy updater-hero-copy--left">
            <span>{translate("ЛУЧШАЯ")}</span>
            <span>{translate("ВЕРСИЯ")}</span>
            <span>{translate("ТЕБЯ")}</span>
            <i />
            <span>{translate("ВМЕСТЕ")}</span>
            <span>{translate("С EVOHIME")}</span>
          </div>
          <div className="updater-hero-copy updater-hero-copy--right">
            <span>{translate("БОЛЬШЕ")}</span>
            <span>{translate("ЧЕМ ИИ")}</span>
            <span>{translate("ВМЕСТЕ С ТОБОЙ")}</span>
            <i />
          </div>
          <div className="updater-hero-signature">EvoHime <span>♡</span></div>
        </div>

        <div className="updater-status-orbit" aria-hidden="true">
          <div className="updater-status-orbit__ring updater-status-orbit__ring--one" />
          <div className="updater-status-orbit__ring updater-status-orbit__ring--two" />
          <div className="updater-status-icon">{translate(icon)}</div>
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
