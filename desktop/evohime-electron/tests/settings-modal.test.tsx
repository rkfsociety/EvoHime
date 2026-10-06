// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { SettingsModal } from '../src/renderer/src/SettingsModal'
import { DEFAULT_APPEARANCE } from '../src/renderer/src/appearance'
import { setAppLocale } from '../src/renderer/src/i18n'

afterEach(() => {
  cleanup()
  setAppLocale('ru')
})

describe('settings modal', () => {
  it('does not show the redundant workspace tab', async () => {
    render(<SettingsModal workspace={'C:\\work\\repo'} onClose={vi.fn()} appearance={DEFAULT_APPEARANCE} onAppearanceChange={vi.fn()} />)

    expect(screen.getByRole('dialog', { name: 'Настройки' })).toBeTruthy()
    expect(screen.getByText('Доступ к моделям')).toBeTruthy()
    expect(screen.queryByRole('tab', { name: 'Рабочая область' })).toBeNull()
  })

  it('closes from the close button and Escape', async () => {
    const onClose = vi.fn()
    render(<SettingsModal workspace={null} onClose={onClose} appearance={DEFAULT_APPEARANCE} onAppearanceChange={vi.fn()} />)

    await userEvent.click(screen.getByRole('button', { name: 'Закрыть настройки' }))
    expect(onClose).toHaveBeenCalledTimes(1)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    expect(onClose).toHaveBeenCalledTimes(2)
  })

  it('shows live appearance controls and applies reset through the owner', async () => {
    const onAppearanceChange = vi.fn()
    render(<SettingsModal workspace={null} onClose={vi.fn()} initialTab="appearance" appearance={{ ...DEFAULT_APPEARANCE, theme: 'light' }} onAppearanceChange={onAppearanceChange} />)

    expect(screen.getByRole('combobox', { name: 'Тема' })).toHaveProperty('value', 'light')
    await userEvent.click(screen.getByRole('button', { name: 'Сбросить' }))
    expect(onAppearanceChange).toHaveBeenCalledWith(DEFAULT_APPEARANCE)
  })

  it('stores the selected interface language in appearance settings', async () => {
    const onAppearanceChange = vi.fn()
    render(<SettingsModal workspace={null} onClose={vi.fn()} initialTab="appearance" appearance={DEFAULT_APPEARANCE} onAppearanceChange={onAppearanceChange} />)

    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Язык' }), 'en')

    expect(onAppearanceChange).toHaveBeenCalledWith({ ...DEFAULT_APPEARANCE, locale: 'en' })
  })
})
