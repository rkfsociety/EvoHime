// @vitest-environment jsdom
import { cleanup, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it } from 'vitest'

import type { CoreEvent } from '../src/shared/api'
import { OverviewPanel, summarize } from '../src/renderer/src/OverviewPanel'

function event(sequenceId: number, eventType: string, payload: unknown, taskId = 'task-1'): CoreEvent {
  return { sequenceId, taskId, eventType, payload: typeof payload === 'string' ? payload : JSON.stringify(payload) }
}

/** Секция «Что требует внимания» — остальная лента дублирует те же тексты. */
function attentionSection(): HTMLElement {
  return screen.getByLabelText('Что требует внимания')
}

afterEach(cleanup)

describe('OverviewPanel', () => {
  it('раскрывает группу сигнала и показывает сами события', async () => {
    const user = userEvent.setup()
    render(
      <OverviewPanel
        connection="connected"
        workspace="D:/github/EvoHime"
        events={[
          event(953, 'task.failed', { error: 'провайдер вернул 429' }),
          event(952, 'task.failed', { error: 'нет доступа к файлу' }, 'task-2')
        ]}
      />
    )

    const section = within(attentionSection())
    const toggle = section.getByRole('button', { name: /Ошибки задач/ })
    expect(toggle.getAttribute('aria-expanded')).toBe('false')
    expect(section.queryByText('провайдер вернул 429')).toBeNull()

    await user.click(toggle)

    expect(toggle.getAttribute('aria-expanded')).toBe('true')
    expect(section.getByText('провайдер вернул 429')).toBeTruthy()
    expect(section.getByText('нет доступа к файлу')).toBeTruthy()
    expect(section.getByText('#953')).toBeTruthy()
    expect(section.getByText('task: task-2')).toBeTruthy()

    await user.click(toggle)
    expect(section.queryByText('провайдер вернул 429')).toBeNull()
  })

  it('ограничивает раскрытый список и сообщает, сколько событий скрыто', async () => {
    const user = userEvent.setup()
    const events = Array.from({ length: 9 }, (_, index) =>
      event(900 + index, 'task.failed', { error: `сбой ${index}` })
    )
    render(<OverviewPanel connection="connected" workspace={null} events={events} />)

    const section = within(attentionSection())
    await user.click(section.getByRole('button', { name: /Ошибки задач/ }))

    expect(section.getByText('сбой 5')).toBeTruthy()
    expect(section.queryByText('сбой 6')).toBeNull()
    expect(section.getByText(/Ещё 3/)).toBeTruthy()
  })

  it('показывает краткое описание рядом с последними событиями', () => {
    render(
      <OverviewPanel
        connection="connected"
        workspace={null}
        events={[event(10, 'ambient.state', { status: 'listening' })]}
      />
    )

    expect(within(screen.getByLabelText('Последние события')).getByText('listening')).toBeTruthy()
  })

  it('отличает старую ошибку от последнего успешного состояния задачи', async () => {
    const user = userEvent.setup()
    render(
      <OverviewPanel
        connection="connected"
        workspace={null}
        events={[
          event(12, 'task.completed', { final_message: 'готово' }),
          event(11, 'task.failed', { error: 'старый сбой' })
        ]}
      />
    )

    expect(screen.getAllByText('0')).toHaveLength(2)
    const section = within(attentionSection())
    await user.click(section.getByRole('button', { name: /Ошибки задач/ }))
    expect(section.getByText('история')).toBeTruthy()
  })

  it('раскрывает полный payload события', async () => {
    const user = userEvent.setup()
    render(<OverviewPanel connection="connected" workspace={null} events={[event(15, 'task.failed', { error: 'подробный сбой', code: 429 })]} />)

    const section = within(attentionSection())
    await user.click(section.getByRole('button', { name: /Ошибки задач/ }))
    await user.click(section.getByRole('button', { name: 'Подробнее' }))
    expect(section.getByText(/"code": 429/)).toBeTruthy()
  })
})

describe('summarize', () => {
  it('берёт первое понятное поле payload', () => {
    expect(summarize(JSON.stringify({ code: 7, reason: 'таймаут ядра' }))).toBe('таймаут ядра')
  })

  it('переживает не-JSON и пустой payload', () => {
    expect(summarize('')).toBe('без payload')
    expect(summarize('  просто   текст ')).toBe('просто текст')
  })

  it('усекает длинные значения', () => {
    expect(summarize(JSON.stringify({ message: 'я'.repeat(200) })).length).toBe(140)
  })

  it('падает обратно на компактный JSON, если понятных полей нет', () => {
    expect(summarize(JSON.stringify({ a: 1, b: [2] }))).toBe('{"a":1,"b":[2]}')
  })
})
