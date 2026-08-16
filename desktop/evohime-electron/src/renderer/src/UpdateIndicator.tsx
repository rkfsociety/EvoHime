import { useEffect, useRef, useState } from 'react'

import type { UpdateStatus } from '@shared/update'
import { shortCommit, updateProgress } from '@shared/update'

import { useShellApi } from './shell-api'
import './UpdateSurface.css'

const VISIBLE_PHASES = ['checking', 'available', 'preparing', 'ready', 'applying', 'failed'] as const

const STEP_MARKERS: Record<string, string> = {
  pending: '○',
  active: '◉',
  done: '✓',
  failed: '✕',
  skipped: '—'
}

interface UpdateIndicatorProps {
  readonly status: UpdateStatus | null
}

/** Compact, always-available entry point for the non-blocking installer update. */
export function UpdateIndicator({ status }: UpdateIndicatorProps): React.JSX.Element | null {
  const api = useShellApi()
  const [open, setOpen] = useState(false)
  const panelRef = useRef<HTMLDivElement>(null)

  const visible = status !== null && (VISIBLE_PHASES as readonly string[]).includes(status.phase)

  useEffect(() => {
    if (!open) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false)
    }
    const onPointerDown = (event: PointerEvent) => {
      if (panelRef.current && !panelRef.current.contains(event.target as Node)) setOpen(false)
    }
    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('pointerdown', onPointerDown)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('pointerdown', onPointerDown)
    }
  }, [open])

  if (!visible || !status) return null

  const running = status.phase === 'checking' || status.phase === 'preparing' || status.phase === 'applying'
  const failed = status.phase === 'failed'
  const progress = updateProgress(status)

  return (
    <div className="update-indicator" ref={panelRef}>
      <button
        type="button"
        className={`update-indicator__button${failed ? ' update-indicator__button--failed' : ''}`}
        aria-label="Показать статус фонового обновления"
        aria-expanded={open}
        title={status.message}
        onClick={() => setOpen((value) => !value)}
      >
        <span className={`update-indicator__spinner${running ? ' update-indicator__spinner--running' : ''}`} aria-hidden="true">
          {failed ? '⚠' : '⟳'}
        </span>
        <span className="update-indicator__dot" aria-hidden="true" />
      </button>

      {open ? (
        <section className="update-popover" role="dialog" aria-label="Статус фонового обновления">
          <div className="update-popover__header">
            <div>
              <h2>Фоновое обновление</h2>
              <p>{status.error ?? status.message}</p>
            </div>
            <button type="button" className="update-popover__close" aria-label="Закрыть" onClick={() => setOpen(false)}>×</button>
          </div>

          <div className="update-popover__meta">
            <span>Ветка <strong>{status.branch}</strong></span>
            <span className="update-popover__commits">
              {shortCommit(status.installedCommit)} → {shortCommit(status.remoteCommit)}
            </span>
          </div>

          <div
            className={`update-progress${progress === null ? ' update-progress--indeterminate' : ''}`}
            role="progressbar"
            aria-label="Прогресс обновления"
            {...(progress === null ? {} : { 'aria-valuenow': Math.round(progress * 100), 'aria-valuemin': 0, 'aria-valuemax': 100 })}
          >
            <div className="update-progress__value" style={progress === null ? undefined : { width: `${progress * 100}%` }} />
          </div>

          <ul className="update-steps">
            {status.steps.map((step) => (
              <li key={step.id} className="update-steps__item" data-state={step.state}>
                <span className="update-steps__marker" aria-hidden="true">{STEP_MARKERS[step.state] ?? '○'}</span>
                {step.label}
              </li>
            ))}
          </ul>

          {status.detail ? <p className="update-popover__detail" title={status.detail}>{status.detail}</p> : null}

          <div className="update-popover__actions">
            {status.phase === 'available' || status.phase === 'failed' ? (
              <button type="button" onClick={() => void api?.invoke('update.prepare', {})}>
                {failed ? 'Повторить' : 'Обновить'}
              </button>
            ) : null}
            {status.phase === 'ready' ? (
              <button type="button" onClick={() => void api?.invoke('update.restart', {})}>Обновить</button>
            ) : null}
          </div>
        </section>
      ) : null}
    </div>
  )
}
