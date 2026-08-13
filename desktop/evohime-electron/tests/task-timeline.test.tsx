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
  // The transcript only shows tasks of the open chat, so the store must
  // report this chat as owning them.
  const chat = {
    id: 'chat-1',
    workspacePath: 'C:\\work\\repo',
    title: 'Чат',
    createdMs: 0,
    updatedMs: 0,
    taskIds: ['task-1'],
    messages: []
  }
  respond = (command) => {
    if (command === 'chat.open' || command === 'chat.appendPrompt' || command === 'chat.create') {
      return ok(chat)
    }
    if (command === 'chat.list') return ok([])
    return ok({ selected: 'C:\\work\\repo', options: [] })
  }
  installApi()
})

afterEach(() => cleanup())

describe('task timeline', () => {
  it('hides the composer until a project is picked, then makes it usable', async () => {
    // Regression: the composer used to read the workspace once on mount, so a
    // folder picked afterwards in the sidebar left it permanently disabled.
    // With no project there is nothing it could do, so it is not shown at all.
    const view = render(
      <TaskTimeline
        connection="connected"
        events={[]}
        workspace={null}
        chatId={null}
        onChatTouched={() => {}}
        onChatOpened={() => {}}
        identityName={null}
        chatRevision={0}
        onOpenGit={() => {}}
      />
    )
    expect(screen.queryByLabelText('Задача')).toBeNull()

    view.rerender(
      <TaskTimeline
        connection="connected"
        events={[]}
        workspace="C:\work\repo"
        chatId={null}
        onChatTouched={() => {}}
        onChatOpened={() => {}}
        identityName={null}
        chatRevision={0}
        onOpenGit={() => {}}
      />
    )

    expect(screen.getByLabelText('Задача').hasAttribute('disabled')).toBe(false)
  })

  it('creates the chat from the first prompt instead of demanding one', async () => {
    const opened: string[] = []
    render(
      <TaskTimeline
        connection="connected"
        events={[]}
        workspace="C:\work\repo"
        chatId={null}
        onChatTouched={() => {}}
        onChatOpened={(id) => opened.push(id)}
        identityName="rkfsociety"
        chatRevision={0}
        onOpenGit={() => {}}
      />
    )

    await userEvent.type(await screen.findByLabelText('Задача'), 'Изучи проект')
    await userEvent.click(screen.getByRole('button', { name: 'Запустить задачу' }))

    await waitFor(() => expect(opened).toEqual(['chat-1']))
    expect(calls.find((call) => call.command === 'chat.create')?.payload).toEqual({
      workspacePath: 'C:\\work\\repo'
    })
    // The task still goes to Core, now bound to the chat that was just made.
    expect(calls.find((call) => call.command === 'core.startTask')).toBeTruthy()
    expect(calls.find((call) => call.command === 'chat.appendPrompt')?.payload).toMatchObject({
      chatId: 'chat-1',
      prompt: 'Изучи проект'
    })
  })

  it('greets by name on the home screen and offers a first task', async () => {
    render(
      <TaskTimeline
        connection="connected"
        events={[]}
        workspace="C:\work\repo"
        chatId={null}
        onChatTouched={() => {}}
        onChatOpened={() => {}}
        identityName="rkfsociety"
        chatRevision={0}
        onOpenGit={() => {}}
      />
    )

    expect(await screen.findByText('Чем займёмся, rkfsociety?')).toBeTruthy()

    await userEvent.click(screen.getByRole('button', { name: /Изучи проект и расскажи/ }))

    expect((screen.getByLabelText('Задача') as HTMLTextAreaElement).value).toMatch(/Изучи проект/)
  })

  it('starts a task only through the typed bridge', async () => {
    render(<TaskTimeline connection="connected" events={[]} workspace="C:\work\repo" chatId="chat-1" onChatTouched={() => {}} onChatOpened={() => {}} identityName={null} chatRevision={0} onOpenGit={() => {}} />)
    await userEvent.type(await screen.findByLabelText('Задача'), 'Проверь тесты')
    await userEvent.click(screen.getByRole('button', { name: 'Запустить задачу' }))

    await waitFor(() => expect(calls.some((call) => call.command === 'core.startTask')).toBe(true))
    expect(calls.find((call) => call.command === 'core.startTask')?.payload).toMatchObject({
      prompt: 'Проверь тесты',
      workspacePath: 'C:\\work\\repo'
    })
  })

  it('shows the unfinished tool directly in the chat and turns the composer into stop', async () => {
    const view = render(
      <TaskTimeline
        connection="connected"
        events={[]}
        workspace="C:\\work\\repo"
        chatId="chat-1"
        onChatTouched={() => {}}
        onChatOpened={() => {}}
        identityName={null}
        chatRevision={0}
        onOpenGit={() => {}}
      />
    )

    await userEvent.type(await screen.findByLabelText('Задача'), 'Проверь тесты')
    await userEvent.click(screen.getByRole('button', { name: 'Запустить задачу' }))
    await waitFor(() => expect(calls.some((call) => call.command === 'core.startTask')).toBe(true))

    const taskId = (calls.find((call) => call.command === 'core.startTask')?.payload as { taskId: string }).taskId
    view.rerender(
      <TaskTimeline
        connection="connected"
        events={[event('tool.started', { tool_name: 'filesystem.read' }, taskId)]}
        workspace="C:\\work\\repo"
        chatId="chat-1"
        onChatTouched={() => {}}
        onChatOpened={() => {}}
        identityName={null}
        chatRevision={0}
        onOpenGit={() => {}}
      />
    )

    expect(screen.queryByText('Модель работает')).toBeNull()
    expect(screen.getByText('Выполняю: Читаю файл')).toBeTruthy()
    expect(screen.queryByText('Остановить', { selector: 'button' })).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: 'Остановить задачу' }))
    await waitFor(() => expect(calls.at(-1)?.command).toBe('core.stopTask'))
  })

  it('shows an animated chat indicator while the agent is forming an answer', async () => {
    const view = render(
      <TaskTimeline
        connection="connected"
        events={[]}
        workspace="C:\\work\\repo"
        chatId="chat-1"
        onChatTouched={() => {}}
        onChatOpened={() => {}}
        identityName={null}
        chatRevision={0}
        onOpenGit={() => {}}
      />
    )

    await userEvent.type(await screen.findByLabelText('Задача'), 'Продолжи проверку')
    await userEvent.click(screen.getByRole('button', { name: 'Запустить задачу' }))
    await waitFor(() => expect(calls.some((call) => call.command === 'core.startTask')).toBe(true))

    const taskId = (calls.find((call) => call.command === 'core.startTask')?.payload as { taskId: string }).taskId
    view.rerender(
      <TaskTimeline
        connection="connected"
        events={[event('agent.message.delta', { content: 'Проверяю результаты…' }, taskId)]}
        workspace="C:\\work\\repo"
        chatId="chat-1"
        onChatTouched={() => {}}
        onChatOpened={() => {}}
        identityName={null}
        chatRevision={0}
        onOpenGit={() => {}}
      />
    )

    expect(screen.getByRole('status', { name: 'Агент формирует ответ' })).toBeTruthy()
    expect(screen.getByText('Проверяю результаты…')).toBeTruthy()
  })

  it('renders streamed output and terminal recovery state', async () => {
    render(
      <TaskTimeline
        workspace="C:\work\repo"
        connection="connected"
        events={[
          event('task.started', { prompt: 'Проверь' }),
          event('agent.message.delta', { content: 'Проверка выполнена' }),
          event('task.failed', { error: 'Провайдер недоступен' })
        ]}
      />
    )

    // Служебные события не попадают в ленту, а ошибка читается текстом.
    expect(await screen.findByText('Проверка выполнена')).toBeTruthy()
    expect(screen.getByText('Провайдер недоступен')).toBeTruthy()
    expect(screen.queryByText('task.started')).toBeNull()
    expect(screen.queryByRole('button', { name: 'Остановить' })).toBeNull()
  })

  it('shows approval details and forwards the decision', async () => {
    render(
      <TaskTimeline
        workspace="C:\work\repo"
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
