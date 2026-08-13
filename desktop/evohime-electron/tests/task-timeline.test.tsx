// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { TaskTimeline } from '../src/renderer/src/TaskTimeline'

const calls: Array<{ command: string; payload: unknown }> = []
let respond: (command: RendererCommand) => unknown

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

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
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
}

function event(eventType: string, payload: Record<string, unknown>, taskId = 'task-1'): CoreEvent {
  return { sequenceId: 1, taskId, eventType, payload: JSON.stringify(payload) }
}

beforeEach(() => {
  calls.length = 0
  respond = () => ok({ selected: 'C:\\work\\repo', options: [] })
  installApi()
})

afterEach(() => cleanup())

describe('task timeline', () => {
  it('starts a task only through the typed bridge', async () => {
    render(<TaskTimeline connection="connected" events={[]} />)
    await userEvent.type(await screen.findByLabelText('Задача'), 'Проверь тесты')
    await userEvent.click(screen.getByRole('button', { name: 'Запустить задачу' }))

    await waitFor(() => expect(calls.some((call) => call.command === 'core.startTask')).toBe(true))
    expect(calls.find((call) => call.command === 'core.startTask')?.payload).toMatchObject({
      prompt: 'Проверь тесты',
      workspacePath: 'C:\\work\\repo'
    })
  })

  it('renders streamed output and terminal recovery state', async () => {
    render(
      <TaskTimeline
        connection="connected"
        events={[
          event('task.started', { prompt: 'Проверь' }),
          event('agent.message.delta', { content: 'Проверка выполнена' }),
          event('task.failed', { error: 'Провайдер недоступен' })
        ]}
      />
    )

    expect(await screen.findByText('Ответ агента')).toBeTruthy()
    expect(screen.getByText('Проверка выполнена')).toBeTruthy()
    expect(screen.getByText('Задача завершилась ошибкой')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Остановить' })).toBeNull()
  })

  it('shows approval details and forwards the decision', async () => {
    render(
      <TaskTimeline
        connection="connected"
        events={[event('approval.required', {
          approval_id: 'approval-1',
          tool_name: 'filesystem.write',
          permission: 'FilesystemWrite',
          scope: 'src/app.ts'
        })]}
      />
    )

    expect(await screen.findByText('Нужно разрешение: filesystem.write')).toBeTruthy()
    await userEvent.click(screen.getByRole('button', { name: 'Разрешить' }))
    await waitFor(() => expect(calls.at(-1)).toEqual({
      command: 'core.resolveApproval',
      payload: { approvalId: 'approval-1', granted: true }
    }))
  })
})
