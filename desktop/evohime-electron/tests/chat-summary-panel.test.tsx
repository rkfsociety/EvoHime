// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { ChatRecord, CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { buildChatSummary } from '../src/renderer/src/chat-summary'
import { ChatSummaryPanel } from '../src/renderer/src/ChatSummaryPanel'

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function makeEvent(
  sequenceId: number,
  eventType: string,
  taskId: string,
  payload: Record<string, unknown>
): CoreEvent {
  return {
    sequenceId,
    eventType,
    taskId,
    timestamp: '2026-11-14T15:00:00Z',
    payload: JSON.stringify(payload)
  }
}

describe('chat summary', () => {
  const chat: ChatRecord = {
    id: 'chat-1',
    workspacePath: 'D:/github/EvoHime',
    title: 'Сборка сводки',
    createdMs: 0,
    updatedMs: 0,
    providerMode: 'cloud',
    workbenchPresentation: { activeTab: 'tasks', splitRatio: 0.5, collapsed: false },
    taskIds: ['task-1'],
    messages: [{ id: 'm1', taskId: 'task-1', prompt: 'Проверь проект', createdMs: 0 }]
  }

  let invokeMock: ReturnType<typeof vi.fn>

  beforeEach(() => {
    invokeMock = vi.fn(async (command: RendererCommand) => {
      if (command === 'chat.open') return ok(chat)
      if (command === 'repository.get') {
        return ok({ branch: 'main', added: 42, removed: 7 })
      }
      if (command === 'core.gitStatus') return ok({ accepted: true })
      if (command === 'core.gitDiff') return ok({ accepted: true })
      if (command === 'core.getContextLedger') return ok({ accepted: true })
      if (command === 'core.backgroundExecution') return ok({ accepted: true })
      if (command === 'core.summarizeContextNow') return ok({ accepted: true })
      return ok(null)
    })

    const api: EvoHimeApiV1 = {
      apiVersion: 1,
      invoke: invokeMock as unknown as EvoHimeApiV1['invoke'],
      subscribe: () => () => {},
      writeClipboardText: async () => true,
      openExternal: async () => true
    }
    Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
  })

  afterEach(() => cleanup())

  it('aggregates tasks, context usage, touched files and active shells for the chat', () => {
    const events: CoreEvent[] = [
      makeEvent(6, 'tool.started', 'task-1', { tool_name: 'shell_execute: npm test' }),
      makeEvent(5, 'tool.started', 'task-1', { tool_name: 'filesystem_patch', path: 'src/App.tsx' }),
      makeEvent(4, 'tool.output', 'task-1', { tool_name: 'shell_execute: git status', output: 'M src/App.tsx' }),
      makeEvent(3, 'tool.started', 'task-1', { tool_name: 'shell_execute: git status' }),
      makeEvent(2, 'model.context', 'task-1', { estimated_tokens: 25_000, context_limit_tokens: 100_000 }),
      makeEvent(1, 'task.started', 'task-1', { model: 'gemini-pro', provider: 'cloud' })
    ]

    const summary = buildChatSummary(events, chat)
    expect(summary.running).toBe(true)
    expect(summary.modelName).toBe('gemini-pro')
    expect(summary.contextPercent).toBe(25)
    expect(summary.activeShellCount).toBe(1)
    expect(summary.shells).toHaveLength(2)
    expect(summary.shells[0]?.command).toBe('npm test')
    expect(summary.shells[0]?.status).toBe('running')
    expect(summary.shells[1]?.status).toBe('completed')
    expect(summary.touchedFiles).toEqual([{ path: 'src/App.tsx', action: 'write', count: 1 }])
  })

  it('renders compact summary widget rows, git changes, context compression and shells', async () => {
    const user = userEvent.setup()
    const events: CoreEvent[] = [
      makeEvent(10, 'git.status', '', { output: ' M src/App.tsx\n?? src/New.tsx', truncated: false }),
      makeEvent(9, 'git.diff', '', { output: '+added line', structured: { path: 'src/App.tsx' }, truncated: false }),
      makeEvent(8, 'context.ledger', 'task-1', {
        entries: [{ id: 'e1', kind: 'pinned', summary: 'Архитектура панели', tokens: 500, pinned: true }]
      }),
      makeEvent(7, 'tool.started', 'task-1', { tool_name: 'shell_execute: pwsh -File check.ps1' }),
      makeEvent(6, 'model.context', 'task-1', { estimated_tokens: 40_000, context_limit_tokens: 100_000 }),
      makeEvent(5, 'task.started', 'task-1', { model: 'claude-sonnet' })
    ]

    render(
      <ChatSummaryPanel
        connection="connected"
        chatId="chat-1"
        events={events}
        workspace="D:/github/EvoHime"
      />
    )

    await waitFor(() => {
      expect(screen.getByText('+42')).toBeTruthy()
    })
    expect(screen.queryByRole('tablist')).toBeNull()
    expect(screen.queryByRole('button', { name: 'Закрыть сводку' })).toBeNull()
    expect(screen.getByText('40%')).toBeTruthy()

    await user.click(screen.getByRole('button', { name: /Изменения/i }))
    expect(screen.getByText('claude-sonnet')).toBeTruthy()

    await user.click(screen.getByRole('button', { name: /Файлы/i }))
    expect(screen.getByText('src/App.tsx')).toBeTruthy()
    expect(screen.getByText('src/New.tsx')).toBeTruthy()
    await user.click(screen.getAllByRole('button', { name: 'Diff' })[0]!)
    expect(invokeMock).toHaveBeenCalledWith('core.gitDiff', expect.objectContaining({ relativePath: 'src/App.tsx' }))

    await user.click(screen.getByRole('button', { name: /Контекст/i }))
    expect(screen.getByText('Архитектура панели')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Сжать контекст' }))
    expect(invokeMock).toHaveBeenCalledWith('core.summarizeContextNow', expect.objectContaining({ taskId: 'task-1' }))

    await user.click(screen.getByRole('button', { name: /Терминал/i }))
    expect(screen.getByText('pwsh -File check.ps1')).toBeTruthy()
    expect(screen.getByText('активен')).toBeTruthy()
  })
})
