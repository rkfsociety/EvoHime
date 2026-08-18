import { useEffect, useRef, useState } from 'react'

import type { ConnectionState } from '@shared/api'

import { useShellApi } from './shell-api'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']
type PermissionMode = 'ask' | 'read_only' | 'full'

const MODES: readonly {
  value: PermissionMode
  label: string
  compactLabel: string
  description: string
}[] = [
  {
    value: 'ask',
    label: 'Запрашивать разрешение',
    compactLabel: 'Подтверждение',
    description: 'Всегда спрашивать перед изменениями и использованием инструментов'
  },
  {
    value: 'read_only',
    label: 'Только чтение',
    compactLabel: 'Только чтение',
    description: 'Разрешить чтение workspace и Git без записи и запуска команд'
  },
  {
    value: 'full',
    label: 'Полный доступ',
    compactLabel: 'Полный доступ',
    description: 'Разрешить доступ к инструментам без отдельных подтверждений'
  }
]

export interface PermissionModePickerProps {
  readonly connection: ConnectionState
}

export function PermissionModePicker({ connection }: PermissionModePickerProps): React.JSX.Element | null {
  const api = useShellApi()
  const connected = CONNECTED_STATES.includes(connection)
  const [mode, setMode] = useState<PermissionMode>('ask')
  const [open, setOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const root = useRef<HTMLDivElement | null>(null)
  const current = MODES.find((item) => item.value === mode) ?? MODES[0]!

  useEffect(() => {
    if (!open) return
    const onPointerDown = (event: MouseEvent): void => {
      if (!root.current?.contains(event.target as Node)) setOpen(false)
    }
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') setOpen(false)
    }
    document.addEventListener('mousedown', onPointerDown)
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('mousedown', onPointerDown)
      document.removeEventListener('keydown', onKeyDown)
    }
  }, [open])

  if (!connected) return null

  const select = async (next: PermissionMode): Promise<void> => {
    if (!api) return
    const outcome = await api.invoke('core.setPermissionMode', { mode: next })
    if (!outcome.ok) {
      setError(outcome.message)
      return
    }
    setError(null)
    setMode(next)
    setOpen(false)
  }

  return (
    <div className="permission-picker" ref={root}>
      <button
        type="button"
        className={`permission-picker__button permission-picker__button--${mode}`}
        aria-label="Режим доступа"
        title={current.label}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <span aria-hidden="true">◉</span>
        <span>{current.compactLabel}</span>
        <span className="permission-picker__chevron" aria-hidden="true">▾</span>
      </button>

      {open ? (
        <div className="permission-picker__menu" role="menu" aria-label="Режим доступа агента">
          <div className="permission-picker__title">Как подтверждать действия?</div>
          {MODES.map((item) => (
            <button
              key={item.value}
              type="button"
              role="menuitemradio"
              aria-checked={item.value === mode}
              className={`permission-picker__option${item.value === mode ? ' permission-picker__option--selected' : ''}`}
              onClick={() => void select(item.value)}
            >
              <span className="permission-picker__option-icon" aria-hidden="true">{item.value === mode ? '✓' : '○'}</span>
              <span>
                <strong>{item.label}</strong>
                <small>{item.description}</small>
              </span>
            </button>
          ))}
          {error ? <p className="permission-picker__error" role="alert">{error}</p> : null}
        </div>
      ) : null}
    </div>
  )
}
