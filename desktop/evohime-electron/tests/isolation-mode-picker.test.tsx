// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { IsolationModePicker } from '../src/renderer/src/IsolationModePicker'

describe('IsolationModePicker', () => {
  it('switches isolation mode from the icon dropdown menu', async () => {
    const onChange = vi.fn()
    render(<IsolationModePicker value="shared_workspace" onChange={onChange} />)

    const trigger = screen.getByRole('button', { name: 'Изоляция файлов' })
    expect(trigger.getAttribute('title')).toContain('Общая папка')

    await userEvent.click(trigger)
    const menu = screen.getByRole('menu', { name: 'Изоляция файлов' })
    expect(menu).toBeTruthy()

    await userEvent.click(screen.getByRole('menuitemradio', { name: /Изолированный worktree/ }))
    expect(onChange).toHaveBeenCalledWith('isolated_worktree')
    expect(screen.queryByRole('menu', { name: 'Изоляция файлов' })).toBeNull()
  })
})
