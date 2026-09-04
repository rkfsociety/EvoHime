// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { ChatSummary, CommandOutcome, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { ProjectSidebar } from '../src/renderer/src/ProjectSidebar'

const calls: { command: string; payload: unknown }[] = []
let chats: ChatSummary[] = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function option(path: string) {
  return { path, available: true, lastUsedMs: 0 }
}

beforeEach(() => {
  calls.length = 0
  chats = [{ id: 'chat-1', workspacePath: 'C:\\work\\repo', title: 'Изучи проект', updatedMs: 2, messageCount: 1 }]
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      if (command === 'workspace.list') return ok({ selected: 'C:\\work\\repo', options: [option('C:\\work\\repo'), option('C:\\work\\other')] })
      if (command === 'chat.list') return ok(chats)
      if (command === 'chat.create') {
        const created = { id: 'chat-2', workspacePath: 'C:\\work\\repo', title: 'Новый чат', createdMs: 3, updatedMs: 3, taskIds: [], messages: [] }
        chats = [{ ...created, messageCount: 0 }, ...chats]
        return ok(created)
      }
      return ok({ accepted: true })
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {}, writeClipboardText: async () => true, openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

function renderSidebar(overrides: Partial<Parameters<typeof ProjectSidebar>[0]> = {}) {
  const props = {
    connection: 'connected' as const, workspace: 'C:\\work\\repo' as string | null, chatId: null as string | null,
    onWorkspaceChange: vi.fn(), onChatChange: vi.fn(), onScheduled: vi.fn(), onPlugins: vi.fn(), revision: 0, ...overrides
  }
  return { props, ...render(<ProjectSidebar {...props} />) }
}

describe('project sidebar', () => {
  it('opens an empty standalone chat without creating it yet', async () => {
    const { props } = renderSidebar({ workspace: null })
    await userEvent.click(await screen.findByRole('button', { name: /Новый чат/ }))
    expect(calls.some(({ command }) => command === 'chat.create')).toBe(false)
    expect(props.onChatChange).toHaveBeenCalledWith(null)
  })

  it('keeps one new-chat action and leaves project selection to the composer', async () => {
    const { props } = renderSidebar()
    expect(screen.getAllByRole('button', { name: /Новый чат/ })).toHaveLength(1)
    expect(screen.getByRole('button', { name: 'Запланировано' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Плагины' })).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Чаты' })).toBeTruthy()
    expect(screen.queryByRole('combobox', { name: 'Рабочая папка' })).toBeNull()
    await waitFor(() => expect(screen.getByRole('button', { name: 'Изучи проект' })).toBeTruthy())
    await userEvent.click(screen.getByRole('button', { name: 'Запланировано' }))
    await userEvent.click(screen.getByRole('button', { name: 'Плагины' }))
    expect(props.onScheduled).toHaveBeenCalledOnce()
    expect(props.onPlugins).toHaveBeenCalledOnce()
  })

  it('opens a project-scoped empty chat without creating it yet', async () => {
    const { props } = renderSidebar()
    await userEvent.click(await screen.findByRole('button', { name: /Новый чат/ }))
    expect(calls.some(({ command }) => command === 'chat.create')).toBe(false)
    expect(props.onChatChange).toHaveBeenCalledWith(null)
  })
})
