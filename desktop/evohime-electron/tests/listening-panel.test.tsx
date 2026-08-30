// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import {
  ListeningPanel,
  clockToMinutes,
  errorText,
  minutesToClock
} from '../src/renderer/src/ListeningPanel'

const calls: Array<{ command: string; payload: unknown }> = []
let hotkeyRegistered = true

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function event(eventType: string, payload: Record<string, unknown>): CoreEvent {
  return { sequenceId: 0, taskId: '', eventType, payload: JSON.stringify(payload) }
}

const runtimeStatus = {
  state: 'ready' as const,
  installedVersion: 'whisper-base-q5_1',
  availableVersion: 'whisper-base-q5_1',
  progressPct: 100,
  message: 'Набор распознавания установлен.',
  missingOptional: [],
  toolsDirectory: 'C:/tools'
}

beforeEach(() => {
  calls.length = 0
  hotkeyRegistered = true
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      if (command === 'ambient.hotkeyStatus') {
        return ok({ combination: 'Control+Alt+M', registered: hotkeyRegistered })
      }
      if (command === 'listener.getRuntimeStatus') return ok(runtimeStatus)
      return ok({ accepted: true })
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true,
    pathForFile: () => ''
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

describe('панель «Слух»', () => {
  it('спрашивает состояние при открытии, не полагаясь на застигнутое событие', async () => {
    render(<ListeningPanel connection="connected" events={[]} />)
    await waitFor(() => {
      expect(calls.map((call) => call.command)).toContain('ambient.getStatus')
    })
    expect(calls.map((call) => call.command)).toContain('ambient.listEpisodes')
    expect(calls.map((call) => call.command)).toContain('ambient.getPolicy')
  })

  it('перечитывает устройства после сообщения о состоянии listener', async () => {
    const { rerender } = render(<ListeningPanel connection="connected" events={[]} />)
    const initialStatusRequests = calls.filter((call) => call.command === 'ambient.getStatus').length

    rerender(
      <ListeningPanel
        connection="connected"
        events={[event('ambient.state', { state: 'listening', reason: 'user_request' })]}
      />
    )

    await waitFor(() => {
      expect(calls.filter((call) => call.command === 'ambient.getStatus').length).toBeGreaterThan(
        initialStatusRequests
      )
    })
  })

  it('без состояния показывает «проверка состояния», а не «выключено»', () => {
    render(<ListeningPanel connection="connected" events={[]} />)
    expect(screen.getByText(/проверка состояния/i)).toBeTruthy()
    expect(screen.queryByText('Слушание выключено')).toBeNull()
  })

  it('показывает состояние и причину из события ядра', () => {
    render(
      <ListeningPanel
        connection="connected"
        events={[
          event('ambient.state', {
            state: 'device_disconnected',
            reason: 'device_disconnected',
            active_device_id: 'mic-1'
          })
        ]}
      />
    )
    expect(screen.getByText('Микрофон отключён')).toBeTruthy()
    expect(screen.getByText(/выбранное устройство пропало/i)).toBeTruthy()
  })

  it('берёт состояние из самого нового события, а не из самого старого в буфере', () => {
    // App.tsx кладёт новое событие в начало массива ([event, ...current]),
    // так что первое совпадение в `events` — самое свежее. Раньше здесь
    // брали последнее совпадение (.at(-1)), что на длинной сессии с полным
    // буфером находило самое старое ещё не вытесненное событие вместо
    // текущего состояния.
    render(
      <ListeningPanel
        connection="connected"
        events={[
          event('ambient.state', { state: 'listening', reason: 'user_request' }),
          event('ambient.state', { state: 'device_disconnected', reason: 'device_disconnected' })
        ]}
      />
    )
    expect(screen.getByText('Ева слушает')).toBeTruthy()
    expect(screen.queryByText('Микрофон отключён')).toBeNull()
  })

  it('сообщает, что глобальный хоткей занят, вместо молчаливого отказа', async () => {
    hotkeyRegistered = false
    render(<ListeningPanel connection="connected" events={[]} />)
    await waitFor(() => {
      expect(screen.getByText(/занят другим приложением и недоступен/i)).toBeTruthy()
    })
  })

  it('пауза уходит в ядро одной командой, а не меняет состояние на месте', async () => {
    render(
      <ListeningPanel
        connection="connected"
        events={[event('ambient.state', { state: 'listening', reason: 'user_request' })]}
      />
    )
    await userEvent.click(screen.getByRole('button', { name: 'Пауза' }))
    const request = calls.find((call) => call.command === 'ambient.setListening')?.payload
    expect(request).toMatchObject({ enabled: true, paused: true })
    // Панель не подменяет состояние сама: без нового события заголовок прежний.
    expect(screen.getByText('Ева слушает')).toBeTruthy()
  })

  it('текста в списке нет; он запрашивается только явным кликом', async () => {
    render(
      <ListeningPanel
        connection="connected"
        events={[
          event('ambient.episodes', {
            episodes: [
              {
                episode_id: 'ep-1',
                started_at_ms: 1_700_000_000_000,
                speech_duration_ms: 4_000,
                utterance_count: 2,
                extraction_state: 'disabled'
              }
            ],
            next_cursor: ''
          })
        ]}
      />
    )
    expect(screen.getByText('высказываний: 2')).toBeTruthy()
    expect(calls.some((call) => call.command === 'ambient.getEpisode')).toBe(false)
    await userEvent.click(screen.getByRole('button', { name: 'Показать текст' }))
    expect(calls.find((call) => call.command === 'ambient.getEpisode')?.payload).toEqual({
      episodeId: 'ep-1'
    })
  })

  it('удаление не отправляется без подтверждения в модальном диалоге', async () => {
    render(<ListeningPanel connection="connected" events={[]} />)
    await userEvent.click(screen.getByRole('button', { name: /Забыть последние 5 минут/i }))
    expect(calls.some((call) => call.command === 'ambient.forgetWindow')).toBe(false)
    const dialog = screen.getByRole('dialog')
    expect(dialog).toBeTruthy()
    await userEvent.click(screen.getByRole('button', { name: 'Удалить' }))
    expect(calls.find((call) => call.command === 'ambient.forgetWindow')?.payload).toEqual({
      windowMs: 5 * 60 * 1000,
      confirmed: true
    })
  })

  it('отмена в диалоге не отправляет удаление всего', async () => {
    render(<ListeningPanel connection="connected" events={[]} />)
    await userEvent.click(screen.getByRole('button', { name: 'Удалить все транскрипты' }))
    await userEvent.click(screen.getByRole('button', { name: 'Отмена' }))
    expect(calls.some((call) => call.command === 'ambient.deleteTranscripts')).toBe(false)
  })

  it('смена устройства уходит командой без перезапуска приложения', async () => {
    render(
      <ListeningPanel
        connection="connected"
        events={[
          event('ambient.status', {
            state: 'listening',
            reason: 'user_request',
            active_device_id: 'mic-1',
            engine_version: 'whisper-base-q5_1',
            engine_ready: true,
            devices: [
              { device_id: 'mic-1', display_name: 'Встроенный', is_default: true, is_active: true },
              { device_id: 'mic-2', display_name: 'Гарнитура', is_default: false, is_active: false }
            ],
            watching_devices: true
          })
        ]}
      />
    )
    await userEvent.click(screen.getByRole('button', { name: /Гарнитура/ }))
    expect(calls.find((call) => call.command === 'ambient.setListening')?.payload).toMatchObject({
      deviceId: 'mic-2'
    })
  })

  it('говорит, что список устройств не обновляется сам, когда подписка не поднялась', () => {
    render(
      <ListeningPanel
        connection="connected"
        events={[
          event('ambient.status', {
            state: 'stopped',
            reason: 'user_request',
            active_device_id: '',
            engine_version: '',
            engine_ready: false,
            devices: [
              { device_id: 'mic-1', display_name: 'Встроенный', is_default: true, is_active: true }
            ],
            watching_devices: false
          })
        ]}
      />
    )
    expect(screen.getByText(/список не обновится сам/i)).toBeTruthy()
  })

  it('известный код ошибки показывается строкой, неизвестный — общей фразой', () => {
    expect(errorText('ENGINE_NOT_READY')).toMatch(/Движок распознавания не готов/)
    expect(errorText('MICROPHONE_ON_FIRE')).toBe('Ошибка слушателя')
  })

  it('показывает код ошибки последней команды', () => {
    render(
      <ListeningPanel
        connection="connected"
        events={[event('ambient.listening', { state: 'stopped', error_code: 'LISTENER_UNAVAILABLE' })]}
      />
    )
    expect(screen.getByRole('alert').textContent).toMatch(/Процесс слушателя недоступен/)
  })

  it('редактор политики отправляет всю политику целиком', async () => {
    render(
      <ListeningPanel
        connection="connected"
        events={[
          event('ambient.policy', {
            quiet_hours: [{ start_minute: 1380, end_minute: 420 }],
            blocklist_patterns: ['zoom*.exe'],
            window_title_blocklist: [],
            retention_days: 7
          })
        ]}
      />
    )
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить политику' }))
    expect(calls.find((call) => call.command === 'ambient.savePolicy')?.payload).toEqual({
      quietHours: [{ startMinute: 1380, endMinute: 420 }],
      blocklistPatterns: ['zoom*.exe'],
      windowTitleBlocklist: [],
      retentionDays: 7
    })
  })

  it('часы суток переводятся в минуты и обратно без потерь', () => {
    expect(minutesToClock(1380)).toBe('23:00')
    expect(clockToMinutes('23:00')).toBe(1380)
    expect(clockToMinutes('24:00')).toBeNull()
    expect(clockToMinutes('не время')).toBeNull()
  })

  /**
   * Панель называет потолок и то, что сверх него предложение отбрасывается, —
   * иначе «Ева ничего не предложила» и «Ева упёрлась в потолок» выглядели бы
   * одинаково.
   */
  it('называет потолок проактивности и запрашивает список предложений', async () => {
    render(
      <ListeningPanel
        connection="connected"
        events={[
          event('ambient.proposals', {
            proposals: [],
            max_per_hour: 3,
            max_per_day: 10,
            min_interval_ms: 600_000,
            error_code: ''
          })
        ]}
      />
    )
    await waitFor(() => {
      expect(calls.some((call) => call.command === 'ambient.listProposals')).toBe(true)
    })
    expect(screen.getByText(/не больше 3 в час и 10 в сутки/)).toBeTruthy()
    expect(screen.getByText(/отбрасывается, а не копится в очередь/)).toBeTruthy()
  })

  /**
   * Услышанная команда — это карточка, а не запуск: панель обязана показать
   * вопрос и отправить решение, а не сообщить о свершившемся факте.
   */
  it('показывает услышанную команду и отправляет решение по ней', async () => {
    render(
      <ListeningPanel
        connection="connected"
        events={[
          event('ambient.voice_commands', {
            commands: [
              {
                command_id: 'voice-1',
                kind: 'open_app',
                app_id: 'chrome',
                title: 'Google Chrome',
                created_at_ms: 1_700_000_000_000,
                expires_at_ms: 1_700_000_300_000
              }
            ],
            requires_confirmation: true,
            error_code: ''
          })
        ]}
      />
    )
    expect(screen.getByText('Открыть Google Chrome?')).toBeTruthy()
    await userEvent.click(screen.getByRole('button', { name: 'Открыть' }))
    expect(calls.find((call) => call.command === 'ambient.resolveVoiceCommand')?.payload).toEqual({
      commandId: 'voice-1',
      accepted: true
    })
  })

  it('называет включённый автозапуск, чтобы пустая очередь не выглядела поломкой', () => {
    render(
      <ListeningPanel
        connection="connected"
        events={[
          event('ambient.voice_commands', {
            commands: [],
            requires_confirmation: false,
            error_code: ''
          })
        ]}
      />
    )
    expect(screen.getByText(/Автозапуск включён/)).toBeTruthy()
  })

  it('отправляет голосовые настройки вместе с остальной политикой', async () => {
    render(
      <ListeningPanel
        connection="connected"
        events={[
          event('ambient.policy', {
            quiet_hours: [],
            blocklist_patterns: [],
            window_title_blocklist: [],
            retention_days: 7,
            voice_commands: true,
            voice_commands_autorun: false
          })
        ]}
      />
    )
    await userEvent.click(
      screen.getByRole('checkbox', { name: /Открывать сразу, без подтверждения/ })
    )
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить политику' }))
    expect(calls.find((call) => call.command === 'ambient.savePolicy')?.payload).toEqual({
      quietHours: [],
      blocklistPatterns: [],
      windowTitleBlocklist: [],
      retentionDays: 7,
      voiceCommands: true,
      voiceCommandsAutorun: true
    })
  })

  /** Без ответа ядра панель не утверждает, что предложений нет. */
  it('не выдумывает пустой список предложений, пока ядро не ответило', () => {
    render(<ListeningPanel connection="connected" events={[]} />)
    expect(screen.getByText('Состояние предложений ещё не получено.')).toBeTruthy()
  })
})
