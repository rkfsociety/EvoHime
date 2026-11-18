// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { HiveDashboard } from '../src/renderer/src/OperationsPanel'

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
    expect(screen.getByText('Запустить тесты')).toBeTruthy()
    expect(screen.getByLabelText('Запрос подтверждения').querySelector('.hive-approval__cost strong')?.textContent).toBe('—')

    await userEvent.click(screen.getByRole('button', { name: 'Разрешить' }))

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

  it('переключает провайдера через provider.select и показывает бюджет', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot())]}
      />
    )

    expect(await screen.findByText('Потрачено сегодня')).toBeTruthy()
    expect(screen.getByText('$0.12 / $0.50')).toBeTruthy()
    expect(screen.getByText(/Лимит: \$0\.50\/день • Остаток: \$0\.38/)).toBeTruthy()

    const picker = await screen.findByLabelText('Провайдер улья')
    await waitFor(() => {
      expect((picker as HTMLSelectElement).options.length).toBeGreaterThan(1)
    })
    expect((picker as HTMLSelectElement).value).toBe('literouter')
    expect(screen.getByRole('option', { name: 'LiteRouter (Free)' })).toBeTruthy()

    await userEvent.selectOptions(picker, 'ollama')
    await waitFor(() => {
      expect(calls.find((call) => call.command === 'provider.select')?.payload).toEqual({ provider: 'ollama' })
    })
    expect(await screen.findByText('Core перезапущен с выбранным провайдером.')).toBeTruthy()
  })

  it('ui_reads_budget_from_snapshot', async () => {
    render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot({ daily_budget_usd: 3.25 }))]} />)
    expect(await screen.findByText('$0.12 / $3.25')).toBeTruthy()
    expect(screen.getByText(/Лимит: \$3\.25\/день/)).toBeTruthy()
  })

  it('ui_can_save_eva_workspace_path', async () => {
    render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot())]} />)
    const input = await screen.findByRole('textbox', { name: 'Путь к Git-репозиторию саморазвития' })
    await userEvent.type(input, 'D:/eva-checkout')
    await userEvent.click(await screen.findByRole('button', { name: 'Сохранить путь' }))
    await waitFor(() => {
      expect(calls.find((call) => call.command === 'core.setEvaWorkspacePath')?.payload).toEqual({ path: 'D:/eva-checkout' })
    })
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
})
