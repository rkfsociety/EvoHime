/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'

import { HumanChatQuestions } from '../src/renderer/src/HumanWorkItemsPanel'
import type { ShellStreamEvent } from '../src/shared/api'

type Listener = (event: ShellStreamEvent) => void

let listeners: Listener[] = []
const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })

afterEach(() => {
  cleanup()
  listeners = []
  invoke.mockClear()
  delete (window as unknown as { evohime?: unknown }).evohime
})

function installApi(): void {
  invoke.mockResolvedValue({ ok: true, value: { accepted: true } })
  ;(window as unknown as { evohime: unknown }).evohime = {
    v1: {
      apiVersion: 1,
      invoke,
      subscribe: (listener: Listener) => {
        listeners.push(listener)
        return () => {
          listeners = listeners.filter((item) => item !== listener)
        }
      },
      writeClipboardText: async () => true
    }
  }
}

function emitHumanWorkItems(payload: unknown): void {
  const event: ShellStreamEvent = {
    kind: 'core-event',
    event: {
      sequenceId: 1,
      taskId: '',
      eventType: 'human_work_items.result',
      payload: JSON.stringify(payload),
      executionEvent: null
    }
  }
  for (const listener of listeners) {
    listener(event)
  }
}

describe('HumanChatQuestions', () => {
  it('не рендерит ничего в чате, если активных вопросов нет', () => {
    installApi()
    const { container } = render(<HumanChatQuestions connection="connected" />)

    expect(invoke).toHaveBeenCalledWith('humanWorkItems.list', expect.any(Object))
    act(() => {
      emitHumanWorkItems({
        operation: 'list',
        status: 'ok',
        projection_json: {
          count: 1,
          items: [{ id: 'done-1', revision: 3, title: 'Готово', state: 'accepted' }]
        }
      })
    })
    expect(container.firstChild).toBeNull()
  })

  it('показывает активный вопрос в чате и отправляет ответ через start -> submit', async () => {
    installApi()
    render(<HumanChatQuestions connection="connected" />)

    act(() => {
      emitHumanWorkItems({
        operation: 'list',
        status: 'ok',
        projection_json: {
          count: 1,
          items: [{ id: 'q-1', revision: 1, title: 'Выбор ветки', state: 'waiting_for_human' }]
        }
      })
    })

    expect(invoke).toHaveBeenCalledWith('humanWorkItems.get', expect.objectContaining({ itemId: 'q-1' }))

    act(() => {
      emitHumanWorkItems({
        operation: 'get',
        status: 'ok',
        item_id: 'q-1',
        revision: 1,
        projection_json: {
          id: 'q-1',
          revision: 1,
          title: 'Выбор ветки',
          instructions: 'В какую ветку сливать изменения?',
          response_schema: { kind: 'choice', choices: ['main', 'dev'] },
          state: 'waiting_for_human'
        }
      })
    })

    expect(screen.getByText('В какую ветку сливать изменения?')).toBeTruthy()

    fireEvent.click(screen.getByRole('button', { name: 'main' }))
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Отправить ответ' }))
    })

    expect(invoke).toHaveBeenCalledWith(
      'humanWorkItems.start',
      expect.objectContaining({ itemId: 'q-1', expectedRevision: 1 })
    )

    act(() => {
      emitHumanWorkItems({
        operation: 'start',
        status: 'ok',
        item_id: 'q-1',
        revision: 2,
        state: 'in_progress'
      })
    })

    expect(invoke).toHaveBeenCalledWith(
      'humanWorkItems.submit',
      expect.objectContaining({ itemId: 'q-1', expectedRevision: 2, response: 'main' })
    )
  })
})
