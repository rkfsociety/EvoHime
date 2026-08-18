// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { SettingsModal } from '../src/renderer/src/SettingsModal'

afterEach(() => cleanup())

describe('settings modal', () => {
  it('switches between settings tabs and shows the current workspace', async () => {
    render(<SettingsModal workspace={'C:\\work\\repo'} onClose={vi.fn()} />)

    expect(screen.getByRole('dialog', { name: 'Настройки' })).toBeTruthy()
    expect(screen.getByText('Доступ к моделям')).toBeTruthy()

    await userEvent.click(screen.getByRole('tab', { name: 'Рабочая область' }))

    expect(screen.getByRole('heading', { name: 'Рабочая область' })).toBeTruthy()
    expect(screen.getByText('C:\\work\\repo')).toBeTruthy()
    expect(screen.getByRole('tab', { name: 'Рабочая область' }).getAttribute('aria-selected')).toBe('true')
  })

  it('closes from the close button and Escape', async () => {
    const onClose = vi.fn()
    render(<SettingsModal workspace={null} onClose={onClose} />)

    await userEvent.click(screen.getByRole('button', { name: 'Закрыть настройки' }))
    expect(onClose).toHaveBeenCalledTimes(1)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    expect(onClose).toHaveBeenCalledTimes(2)
  })
})
