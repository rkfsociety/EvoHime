// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { formatTrace, TracePanel } from '../src/renderer/src/TracePanel'

afterEach(() => cleanup())

describe('trace panel', () => {
  it('shows recent redacted events and closes from the header', async () => {
    const onClose = vi.fn()
    render(
      <TracePanel
        state={null}
        workspace="G:/github/EvoHime"
        onClose={onClose}
        events={[{
          sequenceId: 12,
          taskId: 'task-1',
          eventType: 'tool.output',
          payload: '{"output":"готово"}'
        }]}
      />
    )

    expect(screen.getByRole('complementary', { name: 'Трейс' })).toBeTruthy()
    expect(screen.getByText('tool.output')).toBeTruthy()
    expect(screen.getByText('task: task-1')).toBeTruthy()
    expect(screen.getByText((text) => text.includes('"output": "готово"'))).toBeTruthy()

    await userEvent.click(screen.getByRole('complementary', { name: 'Трейс' }).querySelector('.trace-panel__close')!)
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('closes on Escape', async () => {
    const onClose = vi.fn()
    render(<TracePanel events={[]} state={null} workspace={null} onClose={onClose} />)

    await userEvent.keyboard('{Escape}')
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('formats diagnostics and every event for sharing', () => {
    const trace = formatTrace(null, 'G:/github/EvoHime', [{
      sequenceId: 4,
      taskId: 'task-1',
      eventType: 'task.failed',
      payload: '{"error":"boom"}'
    }])

    expect(trace).toContain('workspace: G:/github/EvoHime')
    expect(trace).toContain('[4] task.failed task=task-1')
    expect(trace).toContain('"error": "boom"')
  })
})
