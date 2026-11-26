// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it } from 'vitest'

import { ActivityLine } from '../src/renderer/src/ActivityLine'

/**
 * The activity line is what the user watches while the agent works, so it must
 * read as Russian prose and must not push the answer off screen: many calls
 * collapse into one line whose details are opened on demand.
 */

afterEach(() => cleanup())

describe('activity line', () => {
  it('keeps the summary line stable and shows the running action underneath', () => {
    render(<ActivityLine calls={[{ tool: 'filesystem_read', output: null, running: true }]} running />)

    const summaryButton = screen.getByRole('button', { name: /1 действие · читаю файл/ })
    expect(summaryButton).toBeTruthy()
    expect(summaryButton.textContent).not.toContain('Выполняю:')
    expect(screen.getByText('Выполняю: Читаю файл')).toBeTruthy()
    expect(screen.queryByText('filesystem_read')).toBeNull()
  })

  it('keeps the latest action underneath between consecutive calls while the stretch is still active', () => {
    render(
      <ActivityLine
        running={false}
        active
        calls={[
          { tool: 'git_status', output: 'clean', running: false },
          { tool: 'git_changed_files', output: 'src/app.ts', running: false }
        ]}
      />
    )

    expect(
      screen.getByRole('button', { name: /2 действия · проверяю состояние репозитория, смотрю изменённые файлы/ })
    ).toBeTruthy()
    expect(screen.getByText('Последнее: Смотрю изменённые файлы')).toBeTruthy()
  })

  it('hides the subline while the full call list is expanded', async () => {
    render(
      <ActivityLine
        running
        active
        calls={[{ tool: 'git_log', output: null, running: true }]}
      />
    )

    expect(screen.getByText('Выполняю: Смотрю историю коммитов')).toBeTruthy()
    await userEvent.click(screen.getByRole('button', { name: /1 действие · смотрю историю коммитов/ }))
    expect(screen.queryByText('Выполняю: Смотрю историю коммитов')).toBeNull()
    expect(screen.getByRole('button', { name: /Смотрю историю коммитов/ })).toBeTruthy()
  })

  it('hides the call list initially, then expands the list and individual call outputs on demand', async () => {
    render(
      <ActivityLine
        running={false}
        calls={[
          { tool: 'filesystem_list', output: 'src-list-output', running: false },
          { tool: 'filesystem_read', output: '# EvoHime', running: false }
        ]}
      />
    )

    const summaryButton = screen.getByRole('button', {
      name: /2 действия · смотрю содержимое папки, читаю файл/
    })
    expect(summaryButton).toBeTruthy()
    expect(summaryButton.getAttribute('aria-expanded')).toBe('false')

    // Neither the list of calls nor their outputs are visible until expanded.
    expect(screen.queryByText('Смотрю содержимое папки')).toBeNull()
    expect(screen.queryByText('Читаю файл')).toBeNull()
    expect(screen.queryByText('src-list-output')).toBeNull()
    expect(screen.queryByText('# EvoHime')).toBeNull()

    await userEvent.click(summaryButton)

    expect(summaryButton.getAttribute('aria-expanded')).toBe('true')
    const listStep = screen.getByRole('button', { name: /Смотрю содержимое папки/ })
    const readStep = screen.getByRole('button', { name: /Читаю файл/ })
    expect(listStep.getAttribute('aria-expanded')).toBe('false')
    expect(readStep.getAttribute('aria-expanded')).toBe('false')
    expect(screen.queryByText('src-list-output')).toBeNull()
    expect(screen.queryByText('# EvoHime')).toBeNull()

    await userEvent.click(readStep)

    expect(readStep.getAttribute('aria-expanded')).toBe('true')
    expect(screen.getByText('# EvoHime')).toBeTruthy()
    expect(screen.queryByText('src-list-output')).toBeNull()
  })

  it('falls back to the identifier of a tool it does not know', () => {
    render(<ActivityLine calls={[{ tool: 'custom.tool', output: null, running: true }]} running />)

    expect(screen.getByText('Выполняю: custom.tool')).toBeTruthy()
  })

  it('counts with the right Russian plural', () => {
    const call = { tool: 'filesystem_read', output: 'x', running: false } as const
    const { rerender } = render(<ActivityLine calls={[call]} running={false} />)
    expect(screen.getByText(/^1 действие/)).toBeTruthy()

    rerender(<ActivityLine calls={[call, call, call, call, call]} running={false} />)
    expect(screen.getByText(/^5 действий/)).toBeTruthy()
  })
})
