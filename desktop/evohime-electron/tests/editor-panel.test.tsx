// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { EditorPanel } from '../src/renderer/src/EditorPanel'

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

describe('editor panel', () => {
  it('prepares and applies a Core-approved bounded build', async () => {
    const user = userEvent.setup()
    const view = render(<EditorPanel connection="connected" events={[]} />)
    await user.type(screen.getByPlaceholderText('src/example.txt'), 'src/example.txt')
    await user.type(screen.getByLabelText('Новое содержимое'), 'hello')
    await user.click(screen.getByRole('button', { name: 'Подготовить diff' }))
    expect(calls.at(-1)?.command).toBe('core.prepareBuild')

    view.rerender(<EditorPanel connection="connected" events={[event('build.prepared', {
      intent_hash: 'intent-1', changes: [{ relative_path: 'src/example.txt', operation: 'write' }]
    })]} />)
    expect(await screen.findByText(/intent_hash/)).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Одобрить и применить' }))
    expect(calls.at(-1)?.command).toBe('core.applyApprovedBuild')
  })
})
