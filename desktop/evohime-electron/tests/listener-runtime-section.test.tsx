// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, EvoHimeApiV1, RendererCommand, ShellEvent } from '../src/shared/api'
import type { ListenerRuntimeStatus } from '../src/shared/listener-runtime'
import { ListenerRuntimeSection } from '../src/renderer/src/ListenerRuntimeSection'

/**
 * Экран рантайма распознавания ничего не решает сам: он показывает состояние
 * main-процесса. Тесты закрепляют именно это — «готово» не появляется без
 * ответа main, а нехватка опциональных файлов называется вслух.
 */

const calls: string[] = []
let status: ListenerRuntimeStatus
let listeners: ((event: ShellEvent) => void)[] = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

beforeEach(() => {
  calls.length = 0
  listeners = []
  status = {
    state: 'missing',
    installedVersion: null,
    availableVersion: 'whisper-small-q5_1',
    progressPct: 0,
    message: 'Распознавание речи не установлено.',
    missingOptional: [],
    toolsDirectory: 'C:\\Users\\u\\AppData\\Local\\EvoHime\\tools\\listener'
  }
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand) => {
      calls.push(command)
      return ok(status)
    }) as EvoHimeApiV1['invoke'],
    subscribe: (listener) => {
      listeners.push(listener)
      return () => {
        listeners = listeners.filter((item) => item !== listener)
      }
    },
    writeClipboardText: async () => true,
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

describe('listener runtime section', () => {
  it('shows the missing state and offers installation', async () => {
    render(<ListenerRuntimeSection />)
    await waitFor(() => expect(screen.getByText('не установлено')).toBeTruthy())
    expect(calls).toContain('listener.getRuntimeStatus')

    await userEvent.click(screen.getByRole('button', { name: 'Установить' }))
    expect(calls).toContain('listener.downloadRuntime')
  })

  /** Прогресс приходит событиями: команда возвращается только в конце. */
  it('follows download progress from shell events', async () => {
    render(<ListenerRuntimeSection />)
    await waitFor(() => expect(listeners.length).toBe(1))

    listeners[0]!({
      kind: 'listener-runtime',
      status: { ...status, state: 'downloading', progressPct: 42, message: 'Загрузка…' }
    })

    await waitFor(() => expect(screen.getByText('загрузка')).toBeTruthy())
    const progress = screen.getByLabelText('Ход загрузки распознавания речи') as HTMLProgressElement
    expect(progress.value).toBe(42)
    // Во время загрузки кнопки не предлагают начать её ещё раз.
    expect((screen.getByRole('button', { name: 'Установить' }) as HTMLButtonElement).disabled).toBe(true)
  })

  /** Неполная установка не выдаётся за полную. */
  it('names the optional files that are missing', async () => {
    status = {
      ...status,
      state: 'ready',
      installedVersion: 'whisper-small-q5_1',
      missingOptional: ['onnxruntime_dll', 'silero_vad'],
      message: 'Установлена версия whisper-small-q5_1.'
    }
    render(<ListenerRuntimeSection />)
    await waitFor(() => expect(screen.getByText('готово')).toBeTruthy())
    expect(screen.getByText(/ONNX Runtime, модель Silero VAD/)).toBeTruthy()
    // Устанавливать нечего: набор уже актуален.
    expect((screen.getByRole('button', { name: 'Установить' }) as HTMLButtonElement).disabled).toBe(true)
  })
})
