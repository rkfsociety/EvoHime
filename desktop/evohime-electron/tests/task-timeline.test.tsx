// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { TaskTimeline } from '../src/renderer/src/TaskTimeline'

const calls: Array<{ command: string; payload: unknown }> = []
let respond: (command: RendererCommand) => unknown
let clipboardText = ''

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
    writeClipboardText: async (text) => {
      clipboardText = text
      return true
    },
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
}

function event(eventType: string, payload: Record<string, unknown>, taskId = 'task-1'): CoreEvent {
  return { sequenceId: 1, taskId, eventType, payload: JSON.stringify(payload) }
}

beforeEach(() => {
  calls.length = 0
  clipboardText = ''
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
      />
    )

    expect(screen.getByLabelText('Задача').hasAttribute('disabled')).toBe(false)
  })

  it('grows the composer with multiline text and shrinks after clearing', async () => {
    render(
      <TaskTimeline
        connection="connected"
        events={[]}
        workspace="C:\\work\\repo"
        chatId={null}
        onChatTouched={() => {}}
        onChatOpened={() => {}}
        identityName={null}
        chatRevision={0}
      />
    )

    const textarea = await screen.findByLabelText('Задача') as HTMLTextAreaElement
    Object.defineProperty(textarea, 'scrollHeight', { configurable: true, value: 96 })
    await userEvent.type(textarea, 'строка 1\\nстрока 2')
    expect(textarea.style.height).toBe('96px')
    expect(textarea.style.overflowY).toBe('hidden')

    Object.defineProperty(textarea, 'scrollHeight', { configurable: true, value: 24 })
    await userEvent.clear(textarea)
    expect(textarea.style.height).toBe('24px')
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
      />
    )

    expect(await screen.findByText('Чем займёмся, rkfsociety?')).toBeTruthy()

    await userEvent.click(screen.getByRole('button', { name: /Изучи проект и расскажи/ }))

    expect((screen.getByLabelText('Задача') as HTMLTextAreaElement).value).toMatch(/Изучи проект/)
  })

  it('shows message time and copies the message through the preload bridge', async () => {
    const atMs = new Date('2026-08-14T13:26:00.000Z').getTime()
    const chat = {
      id: 'chat-1',
      workspacePath: 'C:\\work\\repo',
      title: 'Чат',
      createdMs: atMs,
      updatedMs: atMs,
      taskIds: ['task-1'],
      messages: [{ taskId: 'task-1', prompt: 'Покажи время', atMs }]
    }
    respond = (command) => command === 'chat.open' ? ok(chat) : ok([])
    render(
      <TaskTimeline
        connection="connected"
        events={[]}
        workspace="C:\\work\\repo"
        chatId="chat-1"
        onChatTouched={() => {}}
        onChatOpened={() => {}}
        identityName={null}
        chatRevision={0}
      />
    )

    expect(await screen.findByText('Покажи время')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Скопировать сообщение' })).toBeTruthy()
    expect(screen.getByText(/\d{2}:\d{2}/)).toBeTruthy()
    await userEvent.click(screen.getByRole('button', { name: 'Скопировать сообщение' }))
    await waitFor(() => expect(clipboardText).toBe('Покажи время'))
    expect(screen.getByRole('button', { name: 'Сообщение скопировано' })).toBeTruthy()
  })

  it('clears the delayed copy-state reset when the message unmounts', async () => {
    const setTimeoutSpy = vi.spyOn(window, 'setTimeout')
    const clearTimeoutSpy = vi.spyOn(window, 'clearTimeout')
    const chat = {
      id: 'chat-1',
      workspacePath: 'C:\\work\\repo',
      title: 'Чат',
      createdMs: 1,
      updatedMs: 1,
      taskIds: ['task-1'],
      messages: [{ taskId: 'task-1', prompt: 'Покажи время', atMs: 1 }]
    }
    respond = (command) => command === 'chat.open' ? ok(chat) : ok([])
    render(
      <TaskTimeline
        events={[]}
        workspace="C:\\work\\repo"
        connection="connected"
        chatId="chat-1"
        onChatTouched={() => {}}
        onChatOpened={() => {}}
        identityName={null}
        chatRevision={0}
      />
    )

    await userEvent.click(await screen.findByRole('button', { name: 'Скопировать сообщение' }))
    await waitFor(() => expect(setTimeoutSpy.mock.calls.some(([, delay]) => delay === 1400)).toBe(true))
    const copyTimeouts = setTimeoutSpy.mock.results.filter((_, index) => setTimeoutSpy.mock.calls[index]?.[1] === 1400)
    const copyTimeout = copyTimeouts.at(-1)?.value

    cleanup()

    expect(clearTimeoutSpy).toHaveBeenCalledWith(copyTimeout)
  })

  it('clears the previous conversation when another chat is selected', async () => {
    const firstChat = {
      id: 'chat-1',
      workspacePath: 'C:\\work\\repo',
      title: 'Первый чат',
      createdMs: 1,
      updatedMs: 1,
      taskIds: ['task-1'],
      messages: [{ taskId: 'task-1', prompt: 'Старый вопрос', atMs: 1 }]
    }
    const secondChat = {
      ...firstChat,
      id: 'chat-2',
      title: 'Второй чат',
      taskIds: [],
      messages: []
    }
    respond = (command) => {
      if (command !== 'chat.open') return ok([])
      return ok(firstChat)
    }
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
      />
    )

    expect(await screen.findByText('Старый вопрос')).toBeTruthy()

    respond = (command) => command === 'chat.open' ? ok(secondChat) : ok([])
    view.rerender(
      <TaskTimeline
        connection="connected"
        events={[]}
        workspace="C:\\work\\repo"
        chatId="chat-2"
        onChatTouched={() => {}}
        onChatOpened={() => {}}
        identityName={null}
        chatRevision={0}
      />
    )

    await waitFor(() => expect(screen.queryByText('Старый вопрос')).toBeNull())
    expect(screen.getByText('Чем займёмся?')).toBeTruthy()
  })

  it('keeps each next prompt after the previous agent response', async () => {
    const firstAtMs = new Date('2026-08-14T13:20:00.000Z').getTime()
    const secondAtMs = new Date('2026-08-14T13:21:00.000Z').getTime()
    const chat = {
      id: 'chat-1',
      workspacePath: 'C:\\work\\repo',
      title: 'Чат',
      createdMs: firstAtMs,
      updatedMs: secondAtMs,
      taskIds: ['task-1', 'task-2'],
      messages: [
        { taskId: 'task-1', prompt: 'Первый вопрос', atMs: firstAtMs },
        { taskId: 'task-2', prompt: 'Второй вопрос', atMs: secondAtMs }
      ]
    }
    respond = (command) => command === 'chat.open' ? ok(chat) : ok([])
    render(
      <TaskTimeline
        connection="connected"
        events={[
          event('agent.message.delta', { content: 'Ответ на первый вопрос' }, 'task-1'),
          event('agent.message.delta', { content: 'Ответ на второй вопрос' }, 'task-2')
        ]}
        workspace="C:\\work\\repo"
        chatId="chat-1"
        onChatTouched={() => {}}
        onChatOpened={() => {}}
        identityName={null}
        chatRevision={0}
      />
    )

    await waitFor(() => expect(screen.getByText('Первый вопрос')).toBeTruthy())
    const items = screen.getAllByRole('listitem')
    const text = items.map((item) => item.textContent ?? '')
    expect(text.findIndex((item) => item.includes('Первый вопрос'))).toBeLessThan(
      text.findIndex((item) => item.includes('Ответ на первый вопрос'))
    )
    expect(text.findIndex((item) => item.includes('Ответ на первый вопрос'))).toBeLessThan(
      text.findIndex((item) => item.includes('Второй вопрос'))
    )
    expect(text.findIndex((item) => item.includes('Второй вопрос'))).toBeLessThan(
      text.findIndex((item) => item.includes('Ответ на второй вопрос'))
    )
  })

  it('starts a task only through the typed bridge', async () => {
    render(<TaskTimeline connection="connected" events={[]} workspace="C:\work\repo" chatId="chat-1" onChatTouched={() => {}} onChatOpened={() => {}} identityName={null} chatRevision={0} />)
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
    expect(screen.getAllByText('Провайдер недоступен')).toHaveLength(2)
    expect(screen.getByRole('status', { name: 'Состояние восстановления: FAILED' })).toBeTruthy()
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
          scope: 'src/app.ts',
          preview: { kind: 'file_write', summary: 'Записать файл (42 байт)', path: 'src/app.ts' }
        })]}
      />
    )

    expect(await screen.findByText('Нужно разрешение: filesystem.write')).toBeTruthy()
    expect(screen.getByText('Записать файл (42 байт)')).toBeTruthy()
    await userEvent.click(screen.getByRole('button', { name: 'Разрешить' }))
    await waitFor(() => expect(calls.at(-1)).toEqual({
      command: 'core.resolveApproval',
      payload: {
        approvalId: 'approval-1',
        granted: true,
        idempotencyKey: 'approval:approval-1:grant',
        cancel: false
      }
    }))
  })
})
