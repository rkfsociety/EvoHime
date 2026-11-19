// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { PROVIDER_PROFILE_IDS, type CommandOutcome, type CoreEvent, type EvoHimeApiV1, type RendererCommand } from '../src/shared/api'
import { HiveDashboard } from '../src/renderer/src/OperationsPanel'
import { PROVIDER_PROFILE_LABELS } from '../src/renderer/src/provider-labels'
import { getProviderSchema } from '../src/renderer/src/provider-schema'

const calls: Array<{ command: string; payload: unknown }> = []
let nextSequence = 1

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function event(eventType: string, payload: Record<string, unknown>, taskId = ''): CoreEvent {
  return { sequenceId: nextSequence++, taskId, eventType, payload: JSON.stringify(payload) }
}

/** Snapshot ровно в том виде, в каком его публикует Core (schema v5). */
function snapshot(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    schema_version: 5,
    queen: { status: 'active', name: 'EvoHime', reason: '', wake_at_ms: null },
    metrics: {
      total_agents_online: 2,
      active_tasks: 3,
      average_task_time_sec: 0,
      tasks_today: 0,
      efficiency_percent: 98
    },
    departments: [
      { role: 'developer', active_count: 1, max_capacity: 4 },
      { role: 'researcher', active_count: 1, max_capacity: 4 }
    ],
    active_agents: [
      {
        id: 'run-1:dev-01',
        role: 'developer',
        current_action: 'executing',
        depth: 1,
        spawned_subagents: 1,
        ceiling: 4
      },
      {
        id: 'run-1:worker:hop2',
        role: 'researcher',
        current_action: 'validating report',
        depth: 2,
        spawned_subagents: 0,
        ceiling: 1
      }
    ],
    task_flow: { processing: 2, analyzing: 1, generating_plan: 0 },
    limits: { max_subagents_per_worker: 16, max_parallel_subagents: 4 },
    budget: {
      provider: 'literouter',
      history_available: true,
      model: 'gpt-4o-mini:free',
      requests_last_minute: 1,
      requests_last_hour: 3,
      requests_last_day: 7,
      minute_cap: 5,
      hour_cap: 190,
      day_cap: 0,
      minute_exhausted: false,
      hour_exhausted: false,
      day_exhausted: false,
      daily_cost_available: true,
      daily_cost_nano_usd: 120_000_000
    },
    daily_budget_usd: 0.5,
    eva_workspace_path: null,
    proactive_evolution_enabled: false,
    pulse_status: 'active',
    next_wake_at_ms: Date.now() + 1_800_000,
    ...overrides
  }
}

function providerProfile(model: string, configured: boolean): Record<string, unknown> {
  return { model, baseUrl: '', tier: configured ? 'free' : 'paid', configured }
}

function providerSummary(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    provider: 'literouter',
    model: 'gpt-4o-mini:free',
    baseUrl: '',
    tier: 'free',
    configured: true,
    profiles: {
      literouter: providerProfile('gpt-4o-mini:free', true),
      openai_compatible: providerProfile('', false),
      openai_responses: providerProfile('', false),
      ollama: providerProfile('llama3.2', true)
    },
    ...overrides
  }
}

beforeEach(() => {
  calls.length = 0
  nextSequence = 1
  window.localStorage.clear()
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      if (command === 'core.setEvaWorkspacePath') return ok({ accepted: true })
      if (command === 'core.validateEvaWorkspacePath') return ok({ accepted: true })
      if (command === 'core.setHiveSetting') return ok({ accepted: true })
      if (command === 'chat.open') return ok(null)
      if (command === 'chat.create') {
        return ok({
          id: 'chat-created-1',
          workspacePath: null,
          title: 'Новый чат',
          createdMs: 1,
          updatedMs: 1,
          taskIds: [],
          messages: [],
          workbenchPresentation: { activeTab: 'summary' }
        })
      }
      if (command === 'chat.appendPrompt') return ok(null)
      if (command === 'provider.get') return ok(providerSummary())
      if (command === 'provider.select') {
        return ok({ summary: providerSummary({ provider: 'ollama', model: 'llama3.2', tier: 'free' }), restarted: true })
      }
      return ok({ accepted: true })
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

describe('hive dashboard', () => {
  it('hive_dashboard_renders_all_sections', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot())]}
      />
    )

    expect(await screen.findByRole('heading', { name: 'Улей (Hive)' })).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Департаменты улья' })).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Королева Ева' })).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Метрики улья' })).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Логи улья' })).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Активные агенты (2)' })).toBeTruthy()

    // Все восемь карточек департаментов присутствуют даже там, где Core пока
    // не заявил ни одной роли.
    for (const label of ['Аналитика', 'Разработка', 'Тестирование', 'Планирование', 'Исследования', 'Интеграции', 'Документация', 'Поддержка']) {
      expect(screen.getAllByText(label).length).toBeGreaterThan(0)
    }
    expect(screen.getByText('Активно: 2 / 16')).toBeTruthy()
    expect(screen.getByText('run-1:dev-01')).toBeTruthy()
    expect(screen.getByText('executing')).toBeTruthy()
    expect(screen.getAllByText('98%').length).toBeGreaterThan(0)
    // Snapshot запрашивается у Core, а не берётся из мока.
    await waitFor(() => {
      expect(calls.some((call) => call.command === 'core.getHiveState')).toBe(true)
    })
  })

  it('approval_banner_shows_and_hides', async () => {
    const events = [
      event('approval.required', {
        task_id: 'task-1',
        approval_id: 'approval-1',
        tool_name: 'terminal_exec',
        permission: 'Execute',
        scope: 'workspace',
        preview: { summary: 'Запустить тесты' }
      })
    ]
    render(<HiveDashboard connection="connected" events={events} />)

    expect(await screen.findByText('Требуется ваше подтверждение')).toBeTruthy()
    const banner = screen.getByLabelText('Запрос подтверждения')
    expect(within(banner).getByText('Запустить тесты')).toBeTruthy()
    expect(banner.querySelector('.hive-approval__cost strong')?.textContent).toBe('—')

    await userEvent.click(within(banner).getByRole('button', { name: 'Разрешить' }))

    await waitFor(() => {
      expect(calls.find((call) => call.command === 'core.resolveApproval')?.payload).toMatchObject({
        approvalId: 'approval-1',
        granted: true
      })
    })
    await waitFor(() => {
      expect(screen.queryByText('Требуется ваше подтверждение')).toBeNull()
      expect(screen.queryByRole('button', { name: 'Разрешить' })).toBeNull()
    })
  })

  it('pulse_toggle_changes_state', async () => {
    render(<HiveDashboard connection="connected" events={[]} />)

    const toggle = await screen.findByRole('switch', { name: 'Авто-режим (EvaPulse)' })
    expect(toggle.getAttribute('aria-checked')).toBe('true')

    await userEvent.click(toggle)
    expect(toggle.getAttribute('aria-checked')).toBe('false')
    expect(screen.getByRole('button', { name: 'Обновить сейчас' })).toBeTruthy()
    expect(JSON.parse(window.localStorage.getItem('evohime.hive.dashboard.v1') ?? '{}')).toMatchObject({ pulse: false })
    await waitFor(() => {
      expect(calls.find((call) => call.command === 'core.togglePulse')?.payload).toEqual({ enabled: false })
    })

    await userEvent.click(toggle)
    expect(toggle.getAttribute('aria-checked')).toBe('true')
    await waitFor(() => {
      expect(calls.filter((call) => call.command === 'core.togglePulse').at(-1)?.payload).toEqual({ enabled: true })
    })
  })

  it('показывает статус EvaPulse из снимка Core', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot({ pulse_status: 'sleeping', next_wake_at_ms: Date.now() + 60_000 }))]}
      />
    )

    expect((await screen.findAllByText('EvaPulse спит')).length).toBeGreaterThan(0)
  })

  it('показывает бюджет на дашборде и настраивает провайдера и модель в Настройках улья', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot()),
          event('hive.model.catalog', { profile: 'ollama', models: ['qwen2.5-coder:7b', 'llama3.2:3b'], error: null })
        ]}
      />
    )

    expect(await screen.findByText('Потрачено сегодня')).toBeTruthy()
    expect(screen.getByText('$0.12 / $0.50')).toBeTruthy()
    expect(screen.getByText(/Лимит: \$0\.50\/день • Остаток: \$0\.38/)).toBeTruthy()
    expect(screen.getByText('LiteRouter (Free)')).toBeTruthy()
    expect(screen.queryByLabelText('Провайдер улья')).toBeNull()
    expect(screen.queryByLabelText('Модель королевы')).toBeNull()

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    const providerSelect = within(dialog).getByLabelText('Провайдер Улья')
    const modelSelect = within(dialog).getByLabelText('Модель Улья')
    await userEvent.selectOptions(providerSelect, 'ollama')
    await userEvent.selectOptions(modelSelect, 'qwen2.5-coder:7b')
    await userEvent.click(within(dialog).getByRole('button', { name: 'Сохранить провайдер Улья' }))

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'core.setHiveSetting',
        payload: { key: 'hive_provider_profile', value: 'ollama' }
      })
      expect(calls).toContainEqual({
        command: 'core.setHiveSetting',
        payload: { key: 'hive_model', value: 'qwen2.5-coder:7b' }
      })
    })
  })

  it('ui_reads_budget_from_snapshot', async () => {
    render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot({ daily_budget_usd: 3.25 }))]} />)
    expect(await screen.findByText('$0.12 / $3.25')).toBeTruthy()
    expect(screen.getByText(/Лимит: \$3\.25\/день/)).toBeTruthy()
  })

  it('ui_can_save_eva_workspace_path', async () => {
    const { rerender } = render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot())]} />)
    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    expect(await screen.findByRole('dialog', { name: 'Настройки улья' })).toBeTruthy()
    const input = await screen.findByRole('textbox', { name: 'Путь к исходникам Евы (eva_workspace_path)' })
    await userEvent.type(input, 'D:/eva-checkout')

    await userEvent.click(await screen.findByRole('button', { name: 'Проверить' }))
    await waitFor(() => {
      expect(calls.find((call) => call.command === 'core.validateEvaWorkspacePath')?.payload).toEqual({ path: 'D:/eva-checkout' })
    })
    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot()),
          event('hive_settings.validated', { valid: true, eva_workspace_path: 'D:/eva-checkout', error_code: '' })
        ]}
      />
    )
    expect(await screen.findByTestId('eva-workspace-valid')).toBeTruthy()

    await userEvent.click(await screen.findByRole('button', { name: 'Сохранить' }))
    await waitFor(() => {
      expect(calls.find((call) => call.command === 'core.setHiveSetting')?.payload).toEqual({
        key: 'eva_workspace_path',
        value: 'D:/eva-checkout'
      })
    })
  })

  it('ui_can_save_daily_budget', async () => {
    const { rerender } = render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot({ daily_budget_usd: 0.5 }))]} />)
    expect(await screen.findByText('$0.12 / $0.50')).toBeTruthy()

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    expect(await screen.findByRole('dialog', { name: 'Настройки улья' })).toBeTruthy()

    const budgetInput = await screen.findByRole('spinbutton', { name: 'Дневной лимит бюджета ($)' })
    await userEvent.clear(budgetInput)
    await userEvent.type(budgetInput, '2.00')

    await userEvent.click(await screen.findByRole('button', { name: 'Сохранить лимит' }))
    await waitFor(() => {
      expect(calls.find((call) => call.command === 'core.setHiveSetting')?.payload).toEqual({
        key: 'daily_budget_usd',
        value: '2'
      })
    })
    expect(calls.some((call) => call.command === 'core.getHiveState')).toBe(true)

    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_settings.saved', { accepted: true, daily_budget_usd: 2.0, error_code: '' }),
          event('hive_state.snapshot', snapshot({ daily_budget_usd: 2.0 }))
        ]}
      />
    )
    expect(await screen.findByText('$0.12 / $2.00')).toBeTruthy()
    expect(screen.getByText(/Лимит: \$2\.00\/день/)).toBeTruthy()
  })

  it('ui_can_toggle_proactive_evolution', async () => {
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot({ proactive_evolution_enabled: false }))]}
      />
    )
    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    expect(await screen.findByRole('dialog', { name: 'Настройки улья' })).toBeTruthy()
    expect(screen.queryByRole('textbox', { name: /eva_roadmap_path|планов/i })).toBeNull()

    const toggle = await screen.findByRole('switch', { name: 'Проактивное саморазвитие' })
    expect((toggle as HTMLInputElement).checked).toBe(false)
    await userEvent.click(toggle)

    await waitFor(() => {
      expect(calls.find((call) => call.command === 'core.setHiveSetting' && (call.payload as { key?: string })?.key === 'proactive_evolution_enabled')?.payload).toEqual({
        key: 'proactive_evolution_enabled',
        value: 'true'
      })
    })

    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_settings.saved', {
            accepted: true,
            setting_key: 'proactive_evolution_enabled',
            proactive_evolution_enabled: true,
            error_code: ''
          }),
          event(
            'hive_state.snapshot',
            snapshot({ proactive_evolution_enabled: true })
          )
        ]}
      />
    )
    expect((screen.getByRole('switch', { name: 'Проактивное саморазвитие' }) as HTMLInputElement).checked).toBe(true)
  })

  it('shows zero task metrics when snapshot has no completed-task history', async () => {
    render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot())]} />)
    expect(await screen.findByText('Задач сегодня')).toBeTruthy()
    const list = screen.getByText('Задач сегодня').closest('li')
    expect(list?.querySelector('strong')?.textContent).toBe('0')
    expect(screen.getByText('Среднее время').closest('li')?.querySelector('strong')?.textContent).toBe('0с')
  })

  it('показывает спящую Королеву с точным моментом сброса лимита', async () => {
    const wakeAt = Date.now() + 5 * 60_000
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({
            queen: { status: 'budget_exhausted', name: 'EvoHime', reason: 'provider_minute_cap_reached', wake_at_ms: wakeAt },
            departments: [],
            active_agents: [],
            metrics: { total_agents_online: 0, active_tasks: 0, average_task_time_sec: 0, efficiency_percent: 0 },
            budget: {
              provider: 'literouter',
              history_available: true,
              model: 'free-model:free',
              requests_last_minute: 5,
              requests_last_hour: 5,
              requests_last_day: 5,
              minute_cap: 5,
              hour_cap: 0,
              day_cap: 0,
              minute_exhausted: true,
              hour_exhausted: false,
              day_exhausted: false,
              daily_cost_available: false,
              daily_cost_nano_usd: null
            }
          }))
        ]}
      />
    )

    expect(await screen.findByText('Спит')).toBeTruthy()
    expect(screen.getByText(/лимит запросов за минуту достигнут · сброс в/)).toBeTruthy()
    expect(screen.getByText('Активных агентов нет.')).toBeTruthy()
  })

  it('строит логи из реальных событий Core', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('task.completed', { summary: 'done' }, 'task-9'),
          event('task.failed', { error: 'boom' }, 'task-10'),
          event('hive_state.snapshot', snapshot())
        ]}
      />
    )

    expect(await screen.findByText('Задача завершена · task-9')).toBeTruthy()
    expect(screen.getByText('Задача остановлена с ошибкой · task-10')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Все логи' })).toBeTruthy()
  })

  it('сохраняет выбранный интервал обновления и показывает время получения событий', async () => {
    render(<HiveDashboard connection="connected" events={[event('task.completed', {}, 'task-1')]} />)

    const interval = screen.getByLabelText('Интервал обновления')
    await userEvent.selectOptions(interval, '5000')
    expect(JSON.parse(window.localStorage.getItem('evohime.hive.dashboard.v1') ?? '{}')).toMatchObject({ intervalMs: 5000 })
    await waitFor(() => {
      expect(screen.getByText(/\d{2}:\d{2}:\d{2}/)).toBeTruthy()
    })
  })

  it('читает опубликованную проекцию старой версии', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', {
            schema_version: 1,
            queen: { status: 'idle', name: 'EvoHime' },
            metrics: { total_agents_online: 1, active_tasks: 1, efficiency_percent: 40 },
            departments: [{ role: 'researcher', active_count: 1, max_capacity: 4 }],
            active_agents: [{ id: 'legacy-worker', role: 'researcher', current_action: 'executing' }],
            task_flow: { processing: 1 },
            limits: { max_subagents_per_worker: 4 }
          })
        ]}
      />
    )

    // v1 не содержит budget и части полей — отсутствующие значения заменяются
    // безопасными defaults, но проекция всё равно отображается.
    expect(await screen.findByText('legacy-worker')).toBeTruthy()
    expect(screen.getByText('Модель ещё не известна · лимиты запросов не настроены · история отправок недоступна')).toBeTruthy()
    expect(screen.getByText('Активно: 1 / 4')).toBeTruthy()
  })

  it('игнорирует снимок с неизвестной версией схемы', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', { schema_version: 99, queen: {}, metrics: {}, departments: [], active_agents: [], task_flow: {}, limits: {}, budget: {} })]}
      />
    )

    expect(await screen.findByText('Активных агентов нет.')).toBeTruthy()
    expect(screen.getByText('Активно: 0 / 0')).toBeTruthy()
  })

  it('hive_graph_renders_queen_and_active_agents', async () => {
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={[
          event(
            'hive_state.snapshot',
            snapshot({
              active_agents: [
                { id: 'worker-run-1', role: 'developer', current_action: 'executing', depth: 1, spawned_subagents: 0, ceiling: 4 },
                { id: 'worker-idle-2', role: 'researcher', current_action: 'idle', depth: 1, spawned_subagents: 0, ceiling: 2 }
              ]
            })
          )
        ]}
      />
    )

    const graph = await screen.findByTestId('hive-graph')
    expect(graph.getAttribute('data-queen-state')).toBe('active')

    const queenNode = screen.getByTestId('hive-graph-queen')
    expect(queenNode.getAttribute('data-queen-state')).toBe('active')

    const agentNodes = screen.getAllByTestId('hive-graph-agent')
    expect(agentNodes).toHaveLength(2)
    expect(agentNodes.map((node) => node.getAttribute('data-agent-id'))).toEqual(['worker-run-1', 'worker-idle-2'])
    expect(agentNodes.map((node) => node.getAttribute('data-agent-state'))).toEqual(['running', 'idle'])

    const edges = screen.getAllByTestId('hive-graph-edge')
    expect(edges).toHaveLength(2)
    expect(edges.map((edge) => edge.getAttribute('data-edge-state'))).toEqual(['running', 'idle'])

    // Плашка с именем и статусом остаётся поверх графа.
    expect(screen.getByRole('heading', { name: 'Королева Ева' })).toBeTruthy()

    // При пустом active_agents граф показывает спящую Королеву.
    rerender(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot({ active_agents: [] }))]}
      />
    )

    await waitFor(() => {
      expect(screen.getByTestId('hive-graph').getAttribute('data-queen-state')).toBe('sleeping')
    })
    expect(screen.queryAllByTestId('hive-graph-agent')).toHaveLength(0)
  })

  it('ui_can_send_message_from_hive_dashboard', async () => {
    const user = userEvent.setup()
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot())]}
      />
    )

    const input = await screen.findByLabelText('Сообщение Королеве Еве')
    await user.type(input, 'Исправь баг в коде{enter}')

    await waitFor(() => {
      const startCall = calls.find((item) => item.command === 'core.startTask')
      expect(startCall).toBeTruthy()
      expect(startCall?.payload).toMatchObject({
        prompt: 'Исправь баг в коде',
        workspacePath: '',
        conversationId: 'hive-system-chat',
        preferredRouteHint: 'hive',
        executionKind: 'dialogue'
      })
    })

    await waitFor(() => {
      const appendCall = calls.find((item) => item.command === 'chat.appendPrompt')
      expect(appendCall).toBeTruthy()
      expect(appendCall?.payload).toMatchObject({
        chatId: 'hive-system-chat',
        prompt: 'Исправь баг в коде'
      })
    })

    expect(screen.getByText('Исправь баг в коде')).toBeTruthy()
  })

  it('ui_shows_queen_typing_indicator', async () => {
    const initialEvents = [
      event('hive_state.snapshot', snapshot()),
      event('task.started', { prompt: 'Проверь состояние улья' }, 'task-typing-1')
    ]
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={initialEvents}
      />
    )

    const indicator = await screen.findByTestId('queen-typing-indicator')
    expect(indicator.textContent).toContain('Королева печатает')

    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event('task.completed', { final_message: 'Улей работает штатно.' }, 'task-typing-1'),
          ...initialEvents
        ]}
      />
    )

    await waitFor(() => {
      expect(screen.queryByTestId('queen-typing-indicator')).toBeNull()
    })
    expect(screen.getByText('Улей работает штатно.')).toBeTruthy()
  })

  it('ui_stops_typing_indicator_on_task_failed', async () => {
    const initialEvents = [
      event('hive_state.snapshot', snapshot()),
      event('task.started', { prompt: 'Проверь файлы песочницы' }, 'task-fail-1')
    ]
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={initialEvents}
      />
    )

    const indicator = await screen.findByTestId('queen-typing-indicator')
    expect(indicator.textContent).toContain('Королева печатает')

    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event('task.failed', { error_code: 'permission_denied' }, 'task-fail-1'),
          ...initialEvents
        ]}
      />
    )

    await waitFor(() => {
      expect(screen.queryByTestId('queen-typing-indicator')).toBeNull()
    })
    expect(screen.getByText(/Королева Ева: У меня нет прав на выполнение этого действия/)).toBeTruthy()
  })

  it('ui_duplicates_approval_request_in_hive_chat', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot()),
          event(
            'approval.required',
            {
              approval_id: 'apr-chat-1',
              tool_name: 'eva_workspace_git',
              permission: 'GitWrite',
              scope: 'origin/main',
              preview: { summary: 'Разрешить push изменений?' }
            },
            'task-apr-1'
          )
        ]}
      />
    )

    expect(await screen.findByTestId('hive-chat-approval')).toBeTruthy()
    expect(screen.getAllByText('Разрешить push изменений?')).toHaveLength(2)
  })

  it('ui_displays_provider_limits_and_model', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event(
            'hive_state.snapshot',
            snapshot({
              budget: {
                provider: 'literouter',
                history_available: true,
                model: 'llama-3.3-70b-instruct-turbo-free',
                requests_last_minute: 2,
                requests_last_hour: 15,
                requests_last_day: 40,
                minute_cap: 7,
                hour_cap: 190,
                day_cap: 0,
                limits: { rpm: 7, rph: 190, rpd: 0 },
                minute_exhausted: false,
                hour_exhausted: false,
                day_exhausted: false,
                daily_cost_available: false,
                daily_cost_nano_usd: null
              }
            })
          )
        ]}
      />
    )

    expect(await screen.findByText(/2 \/ 7 за минуту · 15 \/ 190 за час · 40 за сутки/)).toBeTruthy()
    expect(screen.getByText(/Текущая модель: llama-3\.3-70b-instruct-turbo-free/)).toBeTruthy()
    expect(screen.queryByText(/лимиты запросов не настроены/)).toBeNull()
  })

  it('ui_displays_fallback_provider_status', async () => {
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={[
          event(
            'hive_state.snapshot',
            snapshot({
              budget: {
                provider: 'literouter',
                history_available: true,
                model: 'gpt-4o-mini:free',
                requests_last_minute: 1,
                requests_last_hour: 3,
                requests_last_day: 7,
                minute_cap: 5,
                hour_cap: 190,
                day_cap: 0,
                minute_exhausted: false,
                hour_exhausted: false,
                day_exhausted: false,
                daily_cost_available: false,
                daily_cost_nano_usd: null,
                fallback_provider: 'rout',
                fallback_tokens_remaining: 650000
              }
            })
          )
        ]}
        repair={null}
      />
    )
    expect(await screen.findByText('Резерв: Rout (650000 токенов)')).toBeTruthy()

    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event(
            'hive_state.snapshot',
            snapshot({
              budget: {
                provider: 'rout',
                history_available: true,
                model: 'deepseek-chat',
                requests_last_minute: 0,
                requests_last_hour: 0,
                requests_last_day: 0,
                minute_cap: 0,
                hour_cap: 0,
                day_cap: 0,
                minute_exhausted: false,
                hour_exhausted: false,
                day_exhausted: false,
                daily_cost_available: false,
                daily_cost_nano_usd: null,
                daily_token_quota: 650000,
                daily_tokens_remaining: 650000,
                quota_reset_utc_midnight: true
              }
            })
          )
        ]}
        repair={null}
      />
    )
    expect(await screen.findByText('Осталось токенов: 650000 до 00:00 UTC')).toBeTruthy()

    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event(
            'hive_state.snapshot',
            snapshot({
              budget: {
                provider: 'literouter',
                history_available: true,
                model: 'gpt-4o-mini:free',
                requests_last_minute: 1,
                requests_last_hour: 3,
                requests_last_day: 7,
                minute_cap: 5,
                hour_cap: 190,
                day_cap: 0,
                minute_exhausted: false,
                hour_exhausted: false,
                day_exhausted: false,
                daily_cost_available: false,
                daily_cost_nano_usd: null
              }
            })
          )
        ]}
        repair={null}
      />
    )
    await waitFor(() => {
      expect(screen.queryByText(/Резерв: Rout/)).toBeNull()
      expect(screen.queryByText(/Осталось токенов:/)).toBeNull()
    })
  })

  it('ui_only_shows_one_rout_provider', () => {
    expect(PROVIDER_PROFILE_IDS).toContain('rout')
    expect((PROVIDER_PROFILE_IDS as readonly string[])).not.toContain('routmy')
    expect(PROVIDER_PROFILE_IDS.filter((id) => id === 'rout')).toHaveLength(1)
    expect(Object.keys(PROVIDER_PROFILE_LABELS)).not.toContain('routmy')
    expect(PROVIDER_PROFILE_LABELS.rout).toBe('Rout')

    const routSchema = getProviderSchema('openai_compatible', 'rout')
    expect(routSchema.defaultTier).toBe('free')
    expect(routSchema.supportsModelLimits).toBe(false)
  })

  it('hive_logs_keep_latest_on_top_and_reset_scroll', async () => {
    const initialEvents: CoreEvent[] = [
      { sequenceId: 1, coreInstanceId: 'core-1', sessionEpoch: 1, taskId: 'task-old', eventType: 'task.completed', payload: '{}' }
    ]
    const { rerender } = render(<HiveDashboard connection="connected" events={initialEvents} />)

    const logsSection = await screen.findByLabelText('Логи улья')
    const list = logsSection.querySelector('.hive-log-list') as HTMLOListElement
    expect(list).toBeTruthy()
    list.scrollTop = 120

    const updatedEvents: CoreEvent[] = [
      { sequenceId: 1, coreInstanceId: 'core-2', sessionEpoch: 2, taskId: 'task-new', eventType: 'task.failed', payload: '{}' },
      ...initialEvents
    ]
    rerender(<HiveDashboard connection="connected" events={updatedEvents} />)

    await waitFor(() => {
      const rows = list.querySelectorAll('.hive-log-row__text')
      expect(rows[0]?.textContent).toBe('Задача остановлена с ошибкой · task-new')
      expect(rows[1]?.textContent).toBe('Задача завершена · task-old')
      expect(list.scrollTop).toBe(0)
    })
  })

  it('не отображает выбор модели на главном экране Улья — настройка перенесена в Настройки улья', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot()),
          event('model.config', { model: 'gpt-4o-mini:free' }),
          event('model.catalog', { mode: 'free', models: ['gpt-4o-mini:free', 'claude-3.5-haiku:free'] })
        ]}
      />
    )

    expect(await screen.findByText('Текущая модель: gpt-4o-mini:free')).toBeTruthy()
    expect(screen.queryByLabelText('Модель королевы')).toBeNull()
    expect(screen.queryByLabelText('Провайдер улья')).toBeNull()
  })

  it('скрывает блок бюджета если у провайдера не настроена цена', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({
            budget: {
              provider: 'literouter',
              history_available: true,
              model: 'gpt-4o-mini:free',
              requests_last_minute: 0,
              requests_last_hour: 0,
              requests_last_day: 0,
              limits: { rpm: 3, rph: 190, rpd: 0 },
              minute_exhausted: false,
              hour_exhausted: false,
              day_exhausted: false,
              daily_cost_available: false,
              daily_cost_nano_usd: null
            }
          }))
        ]}
      />
    )

    expect(await screen.findByText('Провайдер и лимиты')).toBeTruthy()
    expect(screen.queryByText('Потрачено сегодня')).toBeNull()
    expect(screen.queryByLabelText('Расход дневного бюджета')).toBeNull()
    expect(screen.queryByText(/Лимит: .*Остаток:/)).toBeNull()
  })

  it('ui_shows_token_quota_for_rout', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event(
            'hive_state.snapshot',
            snapshot({
              budget: {
                provider: 'rout',
                history_available: true,
                model: 'openai/gpt-6-luna',
                requests_last_minute: 0,
                requests_last_hour: 0,
                requests_last_day: 0,
                minute_cap: 0,
                hour_cap: 0,
                day_cap: 0,
                minute_exhausted: false,
                hour_exhausted: false,
                day_exhausted: false,
                daily_cost_available: false,
                daily_cost_nano_usd: null,
                daily_token_quota: 650000,
                daily_tokens_remaining: 650000,
                quota_reset_utc_midnight: true
              }
            })
          )
        ]}
      />
    )

    expect(await screen.findByText('Дневной лимит: 650 000 токенов (сброс в 00:00 UTC)')).toBeTruthy()
    expect(screen.queryByText(/Лимит запросов провайдера/)).toBeNull()
  })

  it('ui_stops_typing_when_pulse_sleeps', async () => {
    const initialEvents = [
      event('hive_state.snapshot', snapshot({ pulse_status: 'active' })),
      event('task.started', { prompt: 'Проверь состояние улья' }, 'task-sleep-1')
    ]
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={initialEvents}
      />
    )

    const indicator = await screen.findByTestId('queen-typing-indicator')
    expect(indicator.textContent).toContain('Королева печатает')

    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({ pulse_status: 'sleeping' })),
          event('task.started', { prompt: 'Проверь состояние улья' }, 'task-sleep-1')
        ]}
      />
    )

    await waitFor(() => {
      expect(screen.queryByTestId('queen-typing-indicator')).toBeNull()
    })
  })

  it('hive_uses_dedicated_chat', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot())]}
      />
    )

    const input = await screen.findByRole('textbox', { name: 'Сообщение Королеве Еве' })
    await userEvent.type(input, 'Статус роя')
    await userEvent.click(screen.getByRole('button', { name: 'Отправить' }))

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'core.startTask',
        payload: expect.objectContaining({
          prompt: 'Статус роя',
          conversationId: 'hive-system-chat',
          workspacePath: ''
        })
      })
      expect(calls).toContainEqual({
        command: 'chat.appendPrompt',
        payload: expect.objectContaining({
          chatId: 'hive-system-chat',
          prompt: 'Статус роя'
        })
      })
    })
    expect(calls.some((call) => call.command === 'chat.create')).toBe(false)

    await userEvent.click(screen.getByRole('button', { name: 'Открыть чат Улья' }))
    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'chat.open',
        payload: { chatId: 'hive-system-chat' }
      })
    })
  })

  it('ui_can_add_and_remove_watched_repositories', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event(
            'hive_state.snapshot',
            snapshot({
              eva_workspace_path: 'C:\\github\\EvoHime',
              watched_repositories: ['C:\\github\\EvoHime', 'D:/repo-one']
            })
          )
        ]}
      />
    )

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    const watchedSection = within(dialog).getByTestId('watched-repositories-section')
    // Path to Eva sources must NOT be rendered inside third-party watched repositories
    expect(within(watchedSection).queryByText('C:\\github\\EvoHime')).toBeNull()
    expect(within(watchedSection).getByText('D:/repo-one')).toBeTruthy()

    const repoInput = screen.getByRole('textbox', { name: 'Путь к проекту' })
    await userEvent.type(repoInput, 'c:/github/evohime/')
    await userEvent.click(screen.getByRole('button', { name: 'Добавить проект' }))
    expect(
      within(watchedSection).getByText(
        'Папка исходников Евы задаётся отдельно выше и не добавляется в сторонние проекты.'
      )
    ).toBeTruthy()

    await userEvent.clear(repoInput)
    await userEvent.type(repoInput, 'D:/repo-two')
    await userEvent.click(screen.getByRole('button', { name: 'Добавить проект' }))

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'core.validateEvaWorkspacePath',
        payload: { path: 'D:/repo-two' }
      })
      expect(calls).toContainEqual({
        command: 'core.setHiveSetting',
        payload: {
          key: 'watched_repositories',
          value: JSON.stringify(['D:/repo-one', 'D:/repo-two'])
        }
      })
    })

    // Adding or removing a watched repository must not overwrite eva_workspace_path input
    const evaInput = within(dialog).getByRole('textbox', {
      name: 'Путь к исходникам Евы (eva_workspace_path)'
    }) as HTMLInputElement
    expect(evaInput.value).toBe('C:\\github\\EvoHime')

    await userEvent.click(screen.getByRole('button', { name: 'Удалить проект D:/repo-one' }))
    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'core.setHiveSetting',
        payload: {
          key: 'watched_repositories',
          value: JSON.stringify(['D:/repo-two'])
        }
      })
    })
    expect(evaInput.value).toBe('C:\\github\\EvoHime')
  })

  it('ui_can_save_hive_api_key', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot())]}
      />
    )

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    expect(await screen.findByRole('dialog', { name: 'Настройки улья' })).toBeTruthy()

    const keyInput = screen.getByLabelText('API-ключ Улья (обязателен для работы Улья)') as HTMLInputElement
    expect(keyInput.type).toBe('password')

    await userEvent.type(keyInput, 'sk-hive-secret-123')
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить ключ' }))

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'core.setHiveSetting',
        payload: {
          key: 'hive_api_key',
          value: 'sk-hive-secret-123'
        }
      })
      expect(screen.getByText(/API-ключ Улья сохранён/)).toBeTruthy()
    })
  })

  it('ui_can_save_hive_fallback_provider_and_api_key', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot()),
          event('hive.fallback_model.catalog', { profile: 'rout', models: ['openai/gpt-6-luna', 'deepseek/deepseek-v4.1-flash'], error: null })
        ]}
      />
    )

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })

    const fallbackProviderSelect = within(dialog).getByLabelText('Резервный провайдер Улья')
    const fallbackModelSelect = within(dialog).getByLabelText('Резервная модель Улья')
    await userEvent.selectOptions(fallbackProviderSelect, 'rout')
    await userEvent.selectOptions(fallbackModelSelect, 'openai/gpt-6-luna')
    await userEvent.click(within(dialog).getByRole('button', { name: 'Сохранить резервный провайдер' }))

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'core.setHiveSetting',
        payload: { key: 'hive_fallback_provider_profile', value: 'rout' }
      })
      expect(calls).toContainEqual({
        command: 'core.setHiveSetting',
        payload: { key: 'hive_fallback_model', value: 'openai/gpt-6-luna' }
      })
      expect(within(dialog).getByText(/Резервный провайдер и модель Улья сохранены/)).toBeTruthy()
    })

    const fallbackKeyInput = within(dialog).getByLabelText('Резервный API-ключ Улья') as HTMLInputElement
    expect(fallbackKeyInput.type).toBe('password')
    await userEvent.type(fallbackKeyInput, 'sk-hive-fallback-456')
    await userEvent.click(within(dialog).getByRole('button', { name: 'Сохранить резервный ключ' }))

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'core.setHiveSetting',
        payload: {
          key: 'hive_fallback_api_key',
          value: 'sk-hive-fallback-456'
        }
      })
      expect(within(dialog).getByText(/Резервный API-ключ Улья сохранён/)).toBeTruthy()
    })
  })

  it('does not treat literouter hive budget as rout when global chat provider is rout', async () => {
    const previousApi = window.evohime
    Object.defineProperty(window, 'evohime', {
      configurable: true,
      value: {
        ...previousApi,
        invoke: (async (command: RendererCommand, payload: unknown) => {
          calls.push({ command, payload })
          if (command === 'provider.get') {
            return ok({
              provider: 'openai_compatible',
              model: 'anthropic/claude-fable-5.1-cc',
              baseUrl: 'https://api.rout.my/v1',
              tier: 'free',
              configured: true,
              profileId: 'rout',
              compatibleProfiles: {
                rout: {
                  profileId: 'rout',
                  model: 'anthropic/claude-fable-5.1-cc',
                  baseUrl: 'https://api.rout.my/v1',
                  configured: true
                }
              }
            })
          }
          if (command === 'chat.open') {
            return ok({ chatId: '__hive__', messages: [] })
          }
          return ok({})
        }) as EvoHimeApiV1['invoke']
      }
    })

    render(
      <HiveDashboard
        connection="connected"
        events={[
          event(
            'hive_state.snapshot',
            snapshot({
              budget: {
                provider: 'literouter',
                model: 'glm-5.3-flash-cheap:free',
                daily_cost_nano_usd: null,
                daily_cost_available: false,
                daily_budget_usd: 0.5,
                requests_last_minute: 1,
                requests_last_hour: 2,
                requests_last_day: 3,
                limits: { rpm: 10, rph: 100, rpd: 500 },
                history_available: true,
                exhausted: false
              }
            })
          )
        ]}
      />
    )

    expect(await screen.findByText('LiteRouter (Free)')).toBeTruthy()
    expect(screen.queryByText(/Дневной лимит: 650 000 токенов/)).toBeNull()
  })

  it('copies_hive_logs_with_context_and_payloads_to_clipboard', async () => {
    const copied: string[] = []
    const previousApi = window.evohime?.v1
    Object.defineProperty(window, 'evohime', {
      configurable: true,
      value: {
        v1: {
          ...(previousApi ?? {}),
          invoke: (async () => ok({})) as EvoHimeApiV1['invoke'],
          subscribe: () => () => undefined,
          writeClipboardText: async (text: string) => {
            copied.push(text)
            return true
          }
        }
      }
    })

    try {
      const user = userEvent.setup()
      const failedEvent = event('task.failed', { error: 'provider rate limit reached' }, 'task-hive-42')
      render(
        <HiveDashboard
          connection="connected"
          events={[
            failedEvent,
            event(
              'hive_state.snapshot',
              snapshot({
                budget: {
                  provider: 'literouter',
                  model: 'llama-3.3-70b-instruct-turbo-free',
                  fallback_provider: 'rout',
                  fallback_tokens_remaining: 650000
                }
              })
            )
          ]}
          repair={null}
        />
      )

      const copyButton = await screen.findByRole('button', { name: 'Копировать' })
      await user.click(copyButton)

      await waitFor(() => {
        expect(copied).toHaveLength(1)
        expect(screen.getByRole('button', { name: 'Скопировано' })).toBeTruthy()
      })
      expect(copied[0]).toContain('=== Логи Улья (EvoHime) ===')
      expect(copied[0]).toContain('Провайдер: literouter | Модель: llama-3.3-70b-instruct-turbo-free | Резерв: rout (650000 токенов)')
      expect(copied[0]).toContain(`[WARNING] task.failed (seq=${failedEvent.sequenceId}, task=task-hive-42)`)
      expect(copied[0]).toContain('provider rate limit reached')
    } finally {
      Object.defineProperty(window, 'evohime', {
        configurable: true,
        value: previousApi ? { v1: previousApi } : undefined
      })
    }
  })

})
