import { useEffect, useMemo, useState } from 'react'

import type { UpdaterUiStatus } from '@shared/updater'

import './UpdaterSurface.css'

const fallback: UpdaterUiStatus = {
  phase: 'checking',
  heading: 'Проверяю модули',
  badge: 'Проверка',
  message: 'Сверяю версии и целостность компонентов EvoHime…',
  detail: '',
  percent: null,
  modules: [],
  canApply: false
}

export function UpdaterApp(): React.JSX.Element {
  const [status, setStatus] = useState<UpdaterUiStatus>(fallback)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let active = true
    void window.evohimeUpdater.getStatus().then((next) => {
      if (active) setStatus(next)
    })
    return window.evohimeUpdater.subscribe((next) => setStatus(next))
  }, [])

  useEffect(() => {
    if (status.phase === 'failed' || status.phase === 'ready') setBusy(false)
  }, [status.phase])

  const progressStyle = useMemo(() => {
    if (status.percent === null) return undefined
    return { width: `${status.percent}%` }
  }, [status.percent])

  async function apply(): Promise<void> {
    setBusy(true)
    await window.evohimeUpdater.apply()
  }

  async function launch(): Promise<void> {
    setBusy(true)
    await window.evohimeUpdater.launch()
  }

  const applying = status.phase === 'applying'
  const checking = status.phase === 'checking'
  const failed = status.phase === 'failed'

  return (
    <main className="updater-shell">
      <div className="updater-glow updater-glow--top" aria-hidden="true" />
      <div className="updater-glow updater-glow--bottom" aria-hidden="true" />

      <header className="updater-titlebar">
        <div className="updater-titlebar__drag">
          <span className="updater-logo" aria-hidden="true">E</span>
          <span className="updater-titlebar__name">EvoHime</span>
          <span className="updater-titlebar__divider">/</span>
          <span className="updater-titlebar__section">Обновление</span>
        </div>
        <div className="updater-window-actions">
          <button type="button" aria-label="Свернуть" onClick={() => void window.evohimeUpdater.minimize()}>—</button>
          <button type="button" aria-label="Закрыть" onClick={() => void window.evohimeUpdater.close()}>×</button>
        </div>
      </header>

      <section className="updater-layout">
        <aside className="updater-rail">
          <div>
            <p className="updater-overline">НАДЁЖНЫЙ ЗАПУСК</p>
            <h1>Ева сама<br />проверяет себя.</h1>
            <p className="updater-rail__copy">Компоненты проходят проверку перед запуском, чтобы рабочая оболочка открылась в целостном состоянии.</p>
          </div>
          <div className="updater-orbit" aria-hidden="true">
            <span className="updater-orbit__ring updater-orbit__ring--one" />
            <span className="updater-orbit__ring updater-orbit__ring--two" />
            <span className="updater-orbit__core">✓</span>
          </div>
          <p className="updater-rail__footnote">Проверка выполняется<br />локально и безопасно.</p>
        </aside>

        <div className="updater-content">
          <div className="updater-content__heading">
            <div>
              <p className="updater-overline">ПРОВЕРКА ПЕРЕД ЗАПУСКОМ</p>
              <h2>{status.heading}</h2>
            </div>
            <span className={`updater-badge updater-badge--${status.phase}`}>
              <span className="updater-badge__dot" aria-hidden="true" />
              {status.badge}
            </span>
          </div>

          <p className="updater-message" aria-live="polite">{status.message}</p>

          <div className={`updater-progress${status.percent === null ? ' updater-progress--indeterminate' : ''}`} role="progressbar" aria-label="Прогресс обновления" {...(status.percent === null ? {} : { 'aria-valuenow': status.percent, 'aria-valuemin': 0, 'aria-valuemax': 100 })}>
            <div className="updater-progress__value" style={progressStyle} />
          </div>

          <div className="updater-module-list">
            {status.modules.map((module) => (
              <article className="updater-module" key={module.id}>
                <span className={`updater-module__icon updater-module__icon--${module.available ? 'update' : 'ready'}`} aria-hidden="true">{module.available ? '↻' : '✓'}</span>
                <div className="updater-module__copy">
                  <strong>{module.label}</strong>
                  <span>{module.summary}</span>
                </div>
                <div className="updater-module__version">
                  <span>{module.installed}</span>
                  {module.available ? <><b>→</b><strong>{module.available}</strong></> : <em>актуально</em>}
                </div>
              </article>
            ))}
            {status.modules.length === 0 ? <div className="updater-module updater-module--empty">Получаю список компонентов…</div> : null}
          </div>

          {status.detail ? <p className="updater-detail">{status.detail}</p> : null}

          <footer className="updater-actions">
            <button className="updater-button updater-button--quiet" type="button" disabled={busy || applying} onClick={() => void window.evohimeUpdater.close()}>Закрыть</button>
            {status.canApply ? <button className="updater-button updater-button--secondary" type="button" disabled={busy} onClick={() => void apply()}>Обновить сейчас</button> : null}
            <button className="updater-button updater-button--primary" type="button" disabled={busy || checking || applying} onClick={() => void launch()}>{failed ? 'Запустить текущую версию' : 'Запустить EvoHime'}</button>
          </footer>
        </div>
      </section>
    </main>
  )
}
