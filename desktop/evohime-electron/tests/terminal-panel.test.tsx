// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { TerminalPanel } from '../src/renderer/src/TerminalPanel'

const calls: Array<{ command: string; payload: unknown }> = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function event(eventType: string, payload: Record<string, unknown>): CoreEvent {
  return { sequenceId: 0, taskId: '', eventType, payload: JSON.stringify(payload) }
}

beforeEach(() => {
  calls.length = 0
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      return ok(command === 'workspace.list' ? { selected: 'C:\\work', options: [] } : { accepted: true })
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

describe('terminal panel', () => {
  it('sends bounded commands only through Core and renders output', async () => {
    const view = render(<TerminalPanel connection="connected" events={[]} />)
    await userEvent.click(await screen.findByRole('button', { name: 'Выполнить' }))
    expect(calls.at(-1)?.command).toBe('core.terminalExecute')
    expect(calls.at(-1)?.payload).toMatchObject({ program: 'git', args: ['status', '--short'], timeoutMs: 30_000 })
    view.rerender(<TerminalPanel connection="connected" events={[event('terminal.result', { ok: true, output: 'M README.md', truncated: false })]} />)
    expect(await screen.findByText('M README.md')).toBeTruthy()
  })

  it('shows Core approval and resubmits the same task with approval id', async () => {
    const view = render(<TerminalPanel connection="connected" events={[event('approval.required', {
      task_id: 'task-1', approval_id: 'approval-1', tool_name: 'shell.execute', scope: 'workspace',
      preview: { kind: 'command', summary: 'Запустить команду', command: 'git status --short', cwd: 'workspace' }
    })]} />)
    expect(await screen.findByText(/Terminal требует approval/)).toBeTruthy()
    expect(screen.getByText('Команда: git status --short')).toBeTruthy()
    await userEvent.click(screen.getByRole('button', { name: 'Разрешить выполнение' }))
    expect(calls.at(-1)).toMatchObject({
      command: 'core.terminalExecute',
      payload: { taskId: 'task-1', approvalId: 'approval-1' }
    })
    void view
  })
})
