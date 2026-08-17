import { useState } from 'react'

import type { UpdateStatus } from '@shared/update'
import { updateProgress } from '@shared/update'

import { useShellApi } from './shell-api'
import './UpdateSurface.css'

const VISIBLE_PHASES = ['checking', 'available', 'preparing', 'ready', 'applying', 'failed'] as const

interface UpdateIndicatorProps {
  readonly status: UpdateStatus | null
}

/** Compact, always-available entry point for the non-blocking installer update. */
export function UpdateIndicator({ status }: UpdateIndicatorProps): React.JSX.Element | null {
  const api = useShellApi()
  const [confirmOpen, setConfirmOpen] = useState(false)

  const visible = status !== null && (VISIBLE_PHASES as readonly string[]).includes(status.phase)

  if (!visible || !status) return null

  const running = status.phase === 'checking' || status.phase === 'preparing' || status.phase === 'applying'
  const failed = status.phase === 'failed'
  const progress = status.downloadProgress ?? updateProgress(status)
  const percent = progress === null ? null : Math.round(progress * 100)
  const ready = status.phase === 'ready'

  return (
    <div className="update-indicator">
      <button
        type="button"
        className={`update-indicator__button${failed ? ' update-indicator__button--failed' : ''}${ready ? ' update-indicator__button--ready' : ''}`}
        aria-label={ready ? 'Подтвердить установку обновления' : 'Прогресс скачивания обновления'}
        aria-expanded={ready ? confirmOpen : undefined}
        title={status.message}
        onClick={() => {
          if (ready) setConfirmOpen(true)
        }}
      >
        <span
          className={`update-indicator__circle${running && percent === null ? ' update-indicator__circle--indeterminate' : ''}`}
          style={percent === null ? undefined : { '--update-percent': `${percent}%` } as React.CSSProperties}
          aria-hidden="true"
        >
          <span>{failed ? '!' : percent === null ? '…' : `${percent}%`}</span>
        </span>
      </button>

      {confirmOpen && ready ? (
        <section className="update-confirm" role="dialog" aria-label="Подтверждение обновления">
          <div className="update-popover__header">
            <div>
              <h2>Обновление готово</h2>
              <p>Установщик скачан и проверен. Перезапустить Еву сейчас?</p>
            </div>
          </div>
          <div className="update-confirm__actions">
            <button type="button" onClick={() => setConfirmOpen(false)}>Позже</button>
            <button type="button" onClick={() => void api?.invoke('update.restart', {})}>Перезапустить и обновить</button>
          </div>
        </section>
      ) : null}
    </div>
  )
}
