// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { ChatSummary, CommandOutcome, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { ProjectSidebar } from '../src/renderer/src/ProjectSidebar'

/**
 * The sidebar is how a user reaches their work: projects, the chats inside the
 * open project, and a way to start a new one. Chats are scoped to a project,
 * so switching projects must not leave the previous conversation open.
 */

const calls: { command: string; payload: unknown }[] = []
let chats: ChatSummary[] = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function option(path: string, available = true) {
  return { path, available, lastUsedMs: 0 }
}

beforeEach(() => {
  calls.length = 0
  chats = [
    { id: 'chat-1', workspacePath: 'C:\\work\\repo', title: 'Изучи проект', updatedMs: 2, messageCount: 1 }
  ]
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      if (command === 'workspace.list') {
        return ok({ selected: 'C:\\work\\repo', options: [option('C:\\work\\repo'), option('C:\\work\\other')] })
      }
      if (command === 'workspace.select') {
        return ok({ selected: 'C:\\work\\other', options: [option('C:\\work\\repo'), option('C:\\work\\other')] })
      }
      if (command === 'chat.list') return ok(chats)
      if (command === 'chat.create') {
        const created = {
          id: 'chat-2',
          workspacePath: 'C:\\work\\repo',
          title: 'Новый чат',
          createdMs: 3,
          updatedMs: 3,
          taskIds: [],
          messages: []
        }
        chats = [{ ...created, messageCount: 0 }, ...chats]
        return ok(created)
      }
      return ok({ accepted: true })
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

function renderSidebar(overrides: Partial<Parameters<typeof ProjectSidebar>[0]> = {}) {
  const props = {
    connection: 'connected' as const,
    workspace: 'C:\\work\\repo' as string | null,
    chatId: null as string | null,
    onWorkspaceChange: vi.fn(),
    onChatChange: vi.fn(),
    revision: 0,
    ...overrides
  }
  return { props, ...render(<ProjectSidebar {...props} />) }
}

describe('project sidebar', () => {
  it('lists projects and the chats of the open one', async () => {
    renderSidebar()

    expect(await screen.findByRole('button', { name: 'repo' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'other' })).toBeTruthy()
    // Chats belong to the open project only.
    await waitFor(() => expect(screen.getByRole('button', { name: 'Изучи проект' })).toBeTruthy())
    expect(calls).toContainEqual({
      command: 'chat.list',
      payload: { workspacePath: 'C:\\work\\repo' }
    })
  })

  it('creates a chat and opens it', async () => {
    const { props } = renderSidebar()

    await userEvent.click(await screen.findByRole('button', { name: '+ Новый чат' }))

    expect(calls).toContainEqual({
      command: 'chat.create',
      payload: { workspacePath: 'C:\\work\\repo' }
    })
    await waitFor(() => expect(props.onChatChange).toHaveBeenCalledWith('chat-2'))
  })

  it('closes the open chat when the project changes', async () => {
    const { props } = renderSidebar({ chatId: 'chat-1' })

    await userEvent.click(await screen.findByRole('button', { name: 'other' }))

    await waitFor(() => expect(props.onWorkspaceChange).toHaveBeenCalledWith('C:\\work\\other'))
    // A chat of the previous project must not stay open under the new one.
    expect(props.onChatChange).toHaveBeenCalledWith(null)
  })

  it('marks a project whose folder is gone', async () => {
    renderSidebar()
    cleanup()
    Object.defineProperty(window, 'evohime', {
      value: Object.freeze({
        v1: {
          apiVersion: 1,
          invoke: (async (command: RendererCommand) => {
            if (command === 'workspace.list') {
              return ok({ selected: 'C:\\work\\repo', options: [option('C:\\work\\repo', false)] })
            }
            return ok([])
          }) as EvoHimeApiV1['invoke'],
          subscribe: () => () => {},
          writeClipboardText: async () => true,
          openExternal: async () => true
        }
      }),
      configurable: true
    })

    render(
      <ProjectSidebar
        connection="connected"
        workspace="C:\work\repo"
        chatId={null}
        onWorkspaceChange={vi.fn()}
        onChatChange={vi.fn()}
        revision={0}
      />
    )

    expect(await screen.findByText('недоступна')).toBeTruthy()
    expect(screen.getByText(/Папка недоступна/)).toBeTruthy()
  })
})
