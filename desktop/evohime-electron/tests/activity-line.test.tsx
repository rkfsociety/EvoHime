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
  it('names the running tool in Russian', () => {
    render(<ActivityLine calls={[{ tool: 'filesystem.read', output: null, running: true }]} running />)

    expect(screen.getByText('Читаю файл')).toBeTruthy()
    expect(screen.queryByText('filesystem.read')).toBeNull()
  })

  it('sums a finished stretch into one line and opens details on click', async () => {
    render(
      <ActivityLine
        running={false}
        calls={[
          { tool: 'filesystem.list', output: 'src', running: false },
          { tool: 'filesystem.read', output: '# EvoHime', running: false }
        ]}
      />
    )

    expect(screen.getByText('2 действия · смотрю содержимое папки, читаю файл')).toBeTruthy()
    // Output stays hidden until asked for.
    expect(screen.queryByText('# EvoHime')).toBeNull()

    await userEvent.click(screen.getByRole('button'))

    expect(screen.getByText('# EvoHime')).toBeTruthy()
  })

  it('falls back to the identifier of a tool it does not know', () => {
    render(<ActivityLine calls={[{ tool: 'custom.tool', output: null, running: true }]} running />)

    expect(screen.getByText('custom.tool')).toBeTruthy()
  })

  it('counts with the right Russian plural', () => {
    const call = { tool: 'filesystem.read', output: 'x', running: false } as const
    const { rerender } = render(<ActivityLine calls={[call]} running={false} />)
    expect(screen.getByText(/^1 действие/)).toBeTruthy()

    rerender(<ActivityLine calls={[call, call, call, call, call]} running={false} />)
    expect(screen.getByText(/^5 действий/)).toBeTruthy()
  })
})
