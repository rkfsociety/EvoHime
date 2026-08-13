// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { DeveloperTools } from '../src/renderer/src/DeveloperTools'

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

describe('developer tools', () => {
  it('lists files and opens a read-only preview through Core', async () => {
    const view = render(<DeveloperTools connection="connected" events={[]} />)
    await userEvent.click(await screen.findByRole('button', { name: 'Обновить файлы' }))
    expect(calls.at(-1)?.command).toBe('core.listWorkspace')

    view.rerender(<DeveloperTools connection="connected" events={[
      event('workspace.list', {
        path: '.',
        truncated: false,
        entries: [{ name: 'README.md', relative_path: 'README.md', directory: false, bytes: 12 }]
      })
    ]} />)
    await userEvent.click(await screen.findByRole('button', { name: /README\.md/ }))
    expect(calls.at(-1)).toMatchObject({
      command: 'core.readWorkspaceFile',
      payload: { workspacePath: 'C:\\work', relativePath: 'README.md' }
    })

    view.rerender(<DeveloperTools connection="connected" events={[
      event('workspace.file', { path: 'README.md', content: 'hello from Core' })
    ]} />)
    expect(await screen.findByText('hello from Core')).toBeTruthy()
  })

  it('requests Git status and displays the bounded Core response', async () => {
    const view = render(<DeveloperTools connection="connected" events={[]} />)
    await userEvent.click(await screen.findByRole('button', { name: 'Git status' }))
    expect(calls.at(-1)?.command).toBe('core.gitStatus')
    view.rerender(<DeveloperTools connection="connected" events={[
      event('git.status', { output: 'M README.md', truncated: false })
    ]} />)
    await waitFor(() => expect(screen.getByText('M README.md')).toBeTruthy())
  })
})
