import { useEffect, useRef, useState } from 'react'

import type { ChatIsolationMode } from '@shared/api'
import { translate } from './i18n'
import { UiIcon } from './UiIcon'

export interface IsolationModePickerProps {
  readonly value: ChatIsolationMode
  readonly onChange: (mode: ChatIsolationMode) => void
  readonly disabled?: boolean
}

interface IsolationOption {
  readonly value: ChatIsolationMode
  readonly label: string
  readonly description: string
  readonly icon: 'folder' | 'git-branch'
}

const ISOLATION_OPTIONS: readonly IsolationOption[] = [
  {
    value: 'shared_workspace',
    label: 'Общая папка',
    description: 'Правки выполняются прямо в открытой папке проекта',
    icon: 'folder'
  },
  {
    value: 'isolated_worktree',
    label: 'Изолированный worktree',
    description: 'Отдельная рабочая копия без изменения текущей папки',
    icon: 'git-branch'
  }
]

export function IsolationModePicker({
  value,
  onChange,
  disabled = false
}: IsolationModePickerProps): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const root = useRef<HTMLDivElement | null>(null)
  const current = ISOLATION_OPTIONS.find((item) => item.value === value) ?? ISOLATION_OPTIONS[0]!

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

  return (
    <div className="permission-picker" ref={root}>
      <button
        type="button"
        className="permission-picker__button"
        aria-label={translate('Изоляция файлов')}
        aria-expanded={open}
        title={`${translate('Изоляция файлов')}: ${translate(current.label)}`}
        disabled={disabled}
        onClick={() => setOpen((prev) => !prev)}
      >
        <span className="composer__control-icon" aria-hidden="true">
          <UiIcon name={current.icon} size={14} />
        </span>
        <span>{translate(current.label)}</span>
        <span className="permission-picker__chevron" aria-hidden="true">
          <UiIcon name="chevron-down" size={12} />
        </span>
      </button>

      {open ? (
        <div className="permission-picker__menu" role="menu" aria-label={translate('Изоляция файлов')}>
          <div className="permission-picker__title">{translate('Режим работы с файлами')}</div>
          {ISOLATION_OPTIONS.map((item) => (
            <button
              key={item.value}
              type="button"
              role="menuitemradio"
              aria-checked={item.value === value}
              className={`permission-picker__option${item.value === value ? ' permission-picker__option--selected' : ''}`}
              onClick={() => {
                onChange(item.value)
                setOpen(false)
              }}
            >
              <span className="permission-picker__option-icon" aria-hidden="true">
                <UiIcon name={item.icon} size={14} />
              </span>
              <span>
                <strong>{translate(item.label)}</strong>
                <small>{translate(item.description)}</small>
              </span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  )
}
