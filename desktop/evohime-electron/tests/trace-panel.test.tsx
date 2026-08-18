// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { CommandOutcome, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { formatTrace, TracePanel } from '../src/renderer/src/TracePanel'

let taskIds = ['task-1']

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

beforeEach(() => {
  taskIds = ['task-1']
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand) => {
      if (command === 'chat.open') {
        return ok({
          id: 'chat-1',
          workspacePath: 'G:/github/EvoHime',
          title: 'Чат',
          createdMs: 0,
          updatedMs: 0,
          taskIds,
          messages: []
        })
      }
      return ok(null)
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

describe('trace panel', () => {
  it('shows recent redacted events and closes from the header', async () => {
    const onClose = vi.fn()
    render(
      <TracePanel
        chatId="chat-1"
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

    expect(screen.getByRole('complementary', { name: 'Трейс текущего чата' })).toBeTruthy()
    await waitFor(() => {
      expect(screen.getByText('tool.output')).toBeTruthy()
      expect(screen.getByText('task: task-1')).toBeTruthy()
      expect(screen.getByText((text) => text.includes('"output": "готово"'))).toBeTruthy()
    })

    await userEvent.click(screen.getByRole('complementary', { name: 'Трейс текущего чата' }).querySelector('.trace-panel__close')!)
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('closes on Escape', async () => {
    const onClose = vi.fn()
    render(<TracePanel chatId={null} events={[]} state={null} workspace={null} onClose={onClose} />)

    await userEvent.keyboard('{Escape}')
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('saves the complete trace through the main-process bridge', async () => {
    const invoke = vi.fn(async (command: RendererCommand) => {
      if (command === 'chat.open') {
        return ok({
          id: 'chat-1', workspacePath: 'G:/github/EvoHime', title: 'Чат',
          createdMs: 0, updatedMs: 0, taskIds: ['task-1'], messages: []
        })
      }
      return ok({ cancelled: false, path: 'G:/trace.md' })
    })
    const current = window.evohime.v1
    Object.defineProperty(window, 'evohime', {
      value: Object.freeze({ v1: { ...current, invoke } }), configurable: true
    })
    render(
      <TracePanel chatId="chat-1" state={null} workspace="G:/github/EvoHime" onClose={() => {}}
        events={[{ sequenceId: 12, taskId: 'task-1', eventType: 'tool.output', payload: '{"output":"готово"}' }]} />
    )
    await userEvent.click(await screen.findByRole('button', { name: 'Сохранить .md' }))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('trace.export', expect.objectContaining({ content: expect.stringContaining('[12] tool.output') })))
    expect(await screen.findByText('Трейс сохранён в Markdown-файл.')).toBeTruthy()
  })

  it('reloads task ids after the chat receives a new task', async () => {
    const view = render(
      <TracePanel
        chatId="chat-1"
        chatRevision={0}
        state={null}
        workspace="G:/github/EvoHime"
        onClose={() => {}}
        events={[{
          sequenceId: 12,
          taskId: 'task-1',
          eventType: 'tool.output',
          payload: '{}'
        }]}
      />
    )

    expect(await screen.findByText('tool.output')).toBeTruthy()

    taskIds = ['task-2']
    view.rerender(
      <TracePanel
        chatId="chat-1"
        chatRevision={1}
        state={null}
        workspace="G:/github/EvoHime"
        onClose={() => {}}
        events={[{
          sequenceId: 13,
          taskId: 'task-2',
          eventType: 'task.failed',
          payload: '{"error":"boom"}'
        }]}
      />
    )

    await waitFor(() => expect(screen.getByText('task.failed')).toBeTruthy())
    expect(screen.queryByText('tool.output')).toBeNull()
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
