// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type {
  CommandOutcome,
  EvoHimeApiV1,
  RendererCommand,
  WorkspaceSelection
} from '../src/shared/api'
import { WorkspacePicker } from '../src/renderer/src/WorkspacePicker'

/**
 * UI tests for the workspace slice: what the user sees for every state the
 * main process can report, and that every action goes through the bridge
 * instead of touching the filesystem (plan 0, stage 3).
 */

const calls: Array<{ command: string; payload: unknown }> = []
let respond: (command: RendererCommand) => unknown

function installApi(): void {
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      return respond(command)
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', {
    value: Object.freeze({ v1: api }),
    configurable: true
  })
}

function selection(overrides: Partial<WorkspaceSelection> = {}): WorkspaceSelection {
  return {
    selected: 'C:\\work\\repo',
    options: [{ path: 'C:\\work\\repo', available: true, lastUsedMs: 1 }],
    ...overrides
  }
}

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

beforeEach(() => {
  calls.length = 0
  respond = () => ok(selection())
  installApi()
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('workspace picker', () => {
  it('shows the persisted selection restored at startup', async () => {
    render(<WorkspacePicker connection="connected" />)

    // The remembered path is both shown as the current workspace and marked as
    // the selected row.
    const row = await screen.findByRole('button', { name: 'C:\\work\\repo' })
    expect(row.getAttribute('aria-current')).toBe('true')
    expect(screen.getAllByText('C:\\work\\repo')).toHaveLength(2)
    expect(calls[0]).toEqual({ command: 'workspace.list', payload: {} })
  })

  it('asks the user to pick a folder when nothing is remembered', async () => {
    respond = () => ok(selection({ selected: null, options: [] }))
    render(<WorkspacePicker connection="connected" />)

    expect(await screen.findByText(/Папка не выбрана/)).toBeTruthy()
  })

  it('opens the native dialog through the bridge, never the filesystem', async () => {
    respond = (command) =>
      command === 'workspace.pick'
        ? ok({ cancelled: false, selection: selection({ selected: 'C:\\work\\picked' }) })
        : ok(selection({ selected: null, options: [] }))

    render(<WorkspacePicker connection="connected" />)
    await screen.findByText(/Папка не выбрана/)
    await userEvent.click(screen.getByRole('button', { name: 'Выбрать папку…' }))

    await waitFor(() => expect(screen.getByText('C:\\work\\picked')).toBeTruthy())
    expect(calls.map((call) => call.command)).toEqual(['workspace.list', 'workspace.pick'])
  })

  it('reports a remembered folder that no longer exists', async () => {
    respond = () =>
      ok(
        selection({
          options: [{ path: 'C:\\work\\repo', available: false, lastUsedMs: 1 }]
        })
      )
    render(<WorkspacePicker connection="connected" />)

    expect(await screen.findByRole('alert')).toBeTruthy()
    expect(screen.getByText(/Папка недоступна/)).toBeTruthy()
    expect(screen.getByText('недоступна')).toBeTruthy()
  })

  it('explains that Core is unavailable without hiding the picker', async () => {
    render(<WorkspacePicker connection="reconnecting" />)

    expect(await screen.findByText(/Core недоступен/)).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Выбрать папку…' })).toBeTruthy()
  })

  it('surfaces a rejected selection instead of pretending it worked', async () => {
    respond = (command) =>
      command === 'workspace.select'
        ? ({
            ok: false,
            code: 'workspace-unavailable',
            message: 'Эта папка не выбрана ранее — выбери её заново.'
          } as CommandOutcome<'workspace.select'>)
        : ok(
            selection({
              selected: null,
              options: [{ path: 'C:\\work\\repo', available: true, lastUsedMs: 1 }]
            })
          )

    render(<WorkspacePicker connection="connected" />)
    await userEvent.click(await screen.findByRole('button', { name: 'C:\\work\\repo' }))

    expect(await screen.findByRole('alert')).toBeTruthy()
    expect(screen.getByText(/не выбрана ранее/)).toBeTruthy()
  })

  it('forgets a workspace through the bridge', async () => {
    respond = (command) =>
      command === 'workspace.forget'
        ? ok(selection({ selected: null, options: [] }))
        : ok(selection())

    render(<WorkspacePicker connection="connected" />)
    await userEvent.click(await screen.findByRole('button', { name: 'Забыть C:\\work\\repo' }))

    await waitFor(() => expect(screen.getByText(/Папка не выбрана/)).toBeTruthy())
    expect(calls.at(-1)).toEqual({
      command: 'workspace.forget',
      payload: { path: 'C:\\work\\repo' }
    })
  })

  it('shows a recovery state when the preload bridge is missing', async () => {
    Object.defineProperty(window, 'evohime', { value: undefined, configurable: true })
    render(<WorkspacePicker connection="connected" />)

    expect(await screen.findByRole('alert')).toBeTruthy()
    expect(screen.getByText(/Мост оболочки недоступен/)).toBeTruthy()
  })
})
