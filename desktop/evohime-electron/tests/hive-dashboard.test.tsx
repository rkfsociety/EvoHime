// @vitest-environment jsdom
import { act, cleanup, fireEvent, render as rtlRender, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { PROVIDER_PROFILE_IDS, type CommandOutcome, type ConversationEventProjection, type CoreEvent, type EvoHimeApiV1, type RendererCommand } from '../src/shared/api'
import { HiveDashboard } from '../src/renderer/src/OperationsPanel'
import { ProviderStateProvider } from '../src/renderer/src/provider-state'
import { PROVIDER_PROFILE_LABELS } from '../src/renderer/src/provider-labels'
import { getProviderSchema } from '../src/renderer/src/provider-schema'

async function openQueenChatDrawer(): Promise<void> {
  await userEvent.click(await screen.findByRole('button', { name: 'Диалог с Королевой Евой' }))
}

function render(ui: React.ReactElement): ReturnType<typeof rtlRender> {
  const result = rtlRender(<ProviderStateProvider>{ui}</ProviderStateProvider>)
  return {
    ...result,
    rerender: (nextUi: React.ReactElement) => result.rerender(<ProviderStateProvider>{nextUi}</ProviderStateProvider>)
  }
}

const calls: Array<{ command: string; payload: unknown }> = []
let nextSequence = 1
let providerOverrides: Record<string, unknown> = {}

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function event(eventType: string, payload: Record<string, unknown>, taskId = ''): CoreEvent {
  return { sequenceId: nextSequence++, taskId, eventType, payload: JSON.stringify(payload) }
}

function hiveConversationPage(operation: 'history' | 'live', entries: readonly { kind: string; taskId: string; payload: Record<string, unknown> }[], firstSequence = 1): CoreEvent {
  const events: ConversationEventProjection[] = entries.map((entry, index) => ({
    schemaVersion: 1,
    conversationId: 'hive-system-chat',
    eventId: `hive-${operation}-${nextSequence}-${index}`,
    sequence: firstSequence + index,
    timestampMs: firstSequence + index,
    kind: entry.kind,
    category: 'message',
    payload: entry.payload,
    correlationId: '',
    causationId: '',
    taskId: entry.taskId,
    runId: entry.taskId,
    turnId: entry.taskId,
    clientMessageId: '',
    persistenceClass: 'durable',
    sensitivity: 'internal'
  }))
  return {
    coreInstanceId: 'test-core',
    sessionEpoch: 1,
    sequenceId: 0,
    taskId: '',
    eventType: `conversation.${operation}`,
    payload: '',
    conversationEventLog: {
      schemaVersion: 1,
      operation,
      conversationId: 'hive-system-chat',
      events,
      oldestSequence: events[0]?.sequence ?? 0,
      newestSequence: events.at(-1)?.sequence ?? 0,
      hasOlder: false,
      hasNewer: false,
      earliestAvailableSequence: 1,
      errorCode: ''
    }
  }
}

function hiveConversationAcceptError(errorCode: string): CoreEvent {
  return {
    coreInstanceId: 'test-core',
    sessionEpoch: 1,
    sequenceId: 0,
    taskId: '',
    eventType: 'conversation.accept',
    payload: '',
    conversationEventLog: {
      schemaVersion: 1,
      operation: 'accept',
      conversationId: 'hive-system-chat',
      events: [],
      oldestSequence: 0,
      newestSequence: 0,
      hasOlder: false,
      hasNewer: false,
      earliestAvailableSequence: 0,
      errorCode
    }
  }
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
  providerOverrides = {}
  window.localStorage.clear()
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      if (command === 'core.setEvaWorkspacePath') return ok({ accepted: true })
      if (command === 'core.validateEvaWorkspacePath') return ok({ accepted: true })
      if (command === 'core.setHiveSetting') return ok({ accepted: true })
      if (command === 'chat.open') return ok(null)
      if (command === 'core.getConversationEvents' || command === 'core.subscribeConversationEvents') return ok({ accepted: true })
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
      if (command === 'provider.get') return ok(providerSummary(providerOverrides))
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

afterEach(() => {
  vi.useRealTimers()
  cleanup()
})

describe('hive dashboard', () => {
  it('does not render watched repositories panel in the hive dock', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot({
          watched_repositories: [
            { path: 'D:/work/projects/EvoHime', status: 'idle', last_scanned_at_ms: null, current_task: null }
          ]
        }))]}
      />
    )

    expect(await screen.findByRole('heading', { name: 'Улей (Hive)' })).toBeTruthy()
    expect(screen.queryByRole('region', { name: 'Проекты под наблюдением' })).toBeNull()
  })

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
    const logsSection = screen.getByRole('region', { name: 'Ход работы Королевы' })
    expect(within(logsSection).getByRole('button', { name: 'Диалог с Королевой Евой' })).toBeTruthy()
    expect(document.querySelector('.hive-dock__card--launcher')).toBeNull()
    const agentsHeading = screen.getByRole('heading', { name: 'Активные агенты (2)' })
    expect(agentsHeading).toBeTruthy()
    expect(agentsHeading.closest('.hive-hud-top')).toBeTruthy()
    expect(agentsHeading.closest('.hive-dock')).toBeNull()

    // Департаменты теперь отрисовываются внутри сцены архипелага: в jsdom
    // WebGL/3D-текст не попадает в DOM как обычные текстовые узлы. Наличие
    // секции и сцены проверяет сам контракт отображения, а данные департаментов
    // участвуют в расчёте домов и подписей сцены.
    expect(screen.getByTestId('hive-city-scene')).toBeTruthy()
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
      hiveConversationPage('history', [{
        kind: 'approval_required',
        taskId: 'task-1',
        payload: {
          approval_id: 'approval-1',
          tool_name: 'terminal_exec',
          permission: 'Execute',
          scope: 'workspace',
          preview: { summary: 'Запустить тесты' }
        }
      }])
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

  it('ui_shows_approval_from_an_eva_pulse_origin_task', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot()),
          event('approval.required', {
            approval_id: 'apr-pulse-1',
            tool_name: 'eva_workspace_git',
            permission: 'GitWrite',
            scope: 'D:/eva',
            preview: { summary: 'Разрешить commit recovery-задачи?' }
          }, 'eva-pulse-7f1c')
        ]}
      />
    )

    const approval = screen.getByLabelText('Запрос подтверждения')
    expect(await within(approval).findByText('Разрешить commit recovery-задачи?')).toBeTruthy()
    await userEvent.click(within(approval).getByRole('button', { name: 'Разрешить' }))
    await waitFor(() => {
      expect(calls.find((call) => call.command === 'core.resolveApproval')?.payload).toMatchObject({
        approvalId: 'apr-pulse-1',
        granted: true
      })
    })
  })

  it('показывает статус наблюдения Улья из снимка Core', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot({ observation_status: 'waiting_for_limits', next_wake_at_ms: Date.now() + 60_000 }))]}
      />
    )

    expect((await screen.findAllByText('Улей ожидает (лимиты)')).length).toBeGreaterThan(0)
  })

  it('предпочитает отдельную hive.observation_status projection снимку Улья', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({ observation_status: 'observing', next_wake_at_ms: null })),
          event('hive.observation_status', {
            schema_version: 1,
            owner: 'hive_observation',
            status: 'waiting_for_limits',
            enabled: true,
            next_wake_at_ms: Date.now() + 60_000
          })
        ]}
      />
    )

    expect((await screen.findAllByText('Улей ожидает (лимиты)')).length).toBeGreaterThan(0)
    await waitFor(() => {
      expect(calls.some((call) => call.command === 'core.getHiveObservationStatus')).toBe(true)
    })
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
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))
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
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Лимиты и бюджет' }))

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
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    expect(screen.queryByRole('textbox', { name: /eva_roadmap_path|планов/i })).toBeNull()
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Лимиты и бюджет' }))

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

  it('toggles queen autonomy via core.setHiveSetting proactive_evolution_enabled on the main queen panel', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({ proactive_evolution_enabled: true })),
          event('hive.observation_status', { schema_version: 1, owner: 'hive_observation', status: 'waiting_for_limits', enabled: true, next_wake_at_ms: null })
        ]}
      />
    )

    const toggle = await screen.findByRole('switch', { name: 'Автономность Королевы' })
    expect((toggle as HTMLInputElement).checked).toBe(true)
    await userEvent.click(toggle)

    await waitFor(() => {
      expect(calls.find((call) => call.command === 'core.setHiveSetting' && (call.payload as { key?: string })?.key === 'proactive_evolution_enabled')?.payload).toEqual({
        key: 'proactive_evolution_enabled',
        value: 'false'
      })
    })
    expect(screen.queryByText(/hive\.observation_status/)).toBeNull()
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

    expect(await screen.findByText('Ожидает (лимиты)')).toBeTruthy()
    expect(screen.getByText(/лимит запросов за минуту достигнут · сброс в/)).toBeTruthy()
    expect(screen.getByText('Активных агентов нет.')).toBeTruthy()
  })

  it('строит логи из реальных событий Core', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          hiveConversationPage('history', [
            { kind: 'task_completed', taskId: 'task-9', payload: { status: 'completed' } },
            { kind: 'task_failed', taskId: 'task-10', payload: { error_code: 'task_failed' } }
          ]),
          event('hive_state.snapshot', snapshot())
        ]}
      />
    )

    const activity = await screen.findByLabelText('Ход работы Королевы')
    expect(within(activity).getByText('Задача завершена')).toBeTruthy()
    expect(within(activity).getByText('Задача остановлена с ошибкой')).toBeTruthy()
    expect((screen.getByText('Подробные логи').closest('details') as HTMLDetailsElement).open).toBe(false)
  })

  it('сохраняет выбранный интервал обновления и показывает время получения событий', async () => {
    render(<HiveDashboard connection="connected" events={[
      hiveConversationPage('history', [
        { kind: 'task_completed', taskId: 'task-timestamp', payload: { status: 'completed' } }
      ]),
      event('hive_state.snapshot', snapshot())
    ]} />)

    const interval = screen.getByLabelText('Интервал обновления')
    await userEvent.selectOptions(interval, '5000')
    expect(JSON.parse(window.localStorage.getItem('evohime.hive.dashboard.v1') ?? '{}')).toMatchObject({ intervalMs: 5000 })
    await waitFor(() => {
      expect(within(screen.getByLabelText('Ход работы Королевы')).getAllByText(/\d{2}:\d{2}:\d{2}/).length).toBeGreaterThan(0)
    })
  })

  it('shows queen actions without technical noise or unrelated tasks', async () => {
    render(<HiveDashboard connection="connected" events={[
      event('hive_state.snapshot', snapshot()),
      event('tool.started', { tool_name: 'filesystem_read', arguments: '{"path":"README.md"}' }, 'eva-pulse-test'),
      event('tool.started', { tool_name: 'hive_delegate_agent', arguments: '{"role":"researcher","agent_name":"Исследователь"}' }, 'eva-pulse-test'),
      event('model.trace', { model: 'noise-model' }, 'eva-pulse-test'),
      event('tool.started', { tool_name: 'filesystem_read', arguments: '{"path":"other.txt"}' }, 'unrelated-task')
    ]} />)
    const activity = await screen.findByLabelText('Ход работы Королевы')
    expect(within(activity).getByText('Королева Ева координирует агентов')).toBeTruthy()
    expect(within(activity).getByText('Королева Ева поручает задачу агенту: Исследователь')).toBeTruthy()
    expect(activity.textContent).not.toContain('Королева Ева читает файл')
    expect(activity.textContent).not.toContain('noise-model')
    expect(activity.textContent).not.toContain('other.txt')
  })

  it('shows queen delegation label instead of eva answering on empty message delta', async () => {
    render(<HiveDashboard connection="connected" events={[
      event('hive_state.snapshot', snapshot()),
      event('agent.message.delta', { content: '' }, 'eva-pulse-test')
    ]} />)
    const activity = await screen.findByLabelText('Ход работы Королевы')
    expect(within(activity).getByText('Королева Ева делегирует задачи')).toBeTruthy()
    expect(activity.textContent).not.toContain('Ева отвечает')
  })

  it('attributes delegated hive agent events to the agent rather than Eva', async () => {
    const parentTaskId = 'task-queen-1'
    const activeChildTaskId = `${parentTaskId}:role-researcher-11111111-2222-3333-4444-555555555555`
    const finishedChildTaskId = `${parentTaskId}:role-implementer-aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee`
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({
            active_agents: [{
              id: activeChildTaskId,
              display_name: 'Исследователь ядра',
              role: 'researcher',
              current_action: 'reading'
            }]
          })),
          hiveConversationPage('history', [
            { kind: 'tool_started', taskId: parentTaskId, payload: { tool_name: 'hive_delegate_agent', arguments: '{}' } },
            { kind: 'tool_started', taskId: activeChildTaskId, payload: { tool_name: 'filesystem_read', arguments: '{"path":"AGENTS.md"}' } },
            { kind: 'tool_started', taskId: finishedChildTaskId, payload: { tool_name: 'filesystem_read', arguments: '{"path":"docs/architecture.md"}' } }
          ])
        ]}
      />
    )
    const activity = await screen.findByLabelText('Ход работы Королевы')
    const activityList = activity.querySelector('.hive-activity-list') as HTMLElement
    expect(within(activityList).getByText('Королева Ева поручает задачу агенту')).toBeTruthy()
    expect(within(activityList).getByText('Исследователь ядра читает файл: AGENTS.md')).toBeTruthy()
    expect(within(activityList).getByText('Разработчик читает файл: docs/architecture.md')).toBeTruthy()
    expect(activityList.textContent).not.toContain('Королева Ева читает файл: AGENTS.md')
    expect(activityList.textContent).not.toContain('Королева Ева читает файл: docs/architecture.md')
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

  it('ui_shows_idle_state_for_empty_departments', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot({ active_agents: [] }))]}
      />
    )

    expect(await screen.findAllByText('Нет активных агентов')).not.toHaveLength(0)
    expect(screen.queryByText('0 / 0')).toBeNull()
    expect(screen.queryByText('0%')).toBeNull()
  })

  it('ui_shows_hive_city_scene', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot({ active_agents: [], observation_status: 'observing' }))]}
      />
    )

    const scene = await screen.findByTestId('hive-city-scene')
    expect(scene.getAttribute('data-status')).toBe('observing')
    expect(screen.getByRole('heading', { name: 'Королева Ева' })).toBeTruthy()
  })

  it('hive_city_scene_reflects_observation_status', async () => {
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={[
          event(
            'hive_state.snapshot',
            snapshot({
              observation_status: 'active',
              active_agents: [
                { id: 'worker-run-1', role: 'developer', current_action: 'executing', depth: 1, spawned_subagents: 0, ceiling: 4 }
              ]
            })
          )
        ]}
      />
    )

    const scene = await screen.findByTestId('hive-city-scene')
    expect(scene.getAttribute('data-status')).toBe('active')
    expect(screen.getByRole('heading', { name: 'Королева Ева' })).toBeTruthy()

    rerender(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot({ observation_status: 'waiting_for_tasks', active_agents: [] }))]}
      />
    )

    await waitFor(() => {
      expect(screen.getByTestId('hive-city-scene').getAttribute('data-status')).toBe('waiting_for_tasks')
    })
  })

  it('ui_can_send_message_from_hive_dashboard', async () => {
    const user = userEvent.setup()
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot())]}
      />
    )

    await openQueenChatDrawer()
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
      hiveConversationPage('history', [
        { kind: 'task_started', taskId: 'task-typing-1', payload: { prompt: 'Проверь состояние улья' } }
      ])
    ]
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={initialEvents}
      />
    )

    await openQueenChatDrawer()
    const indicator = await screen.findByTestId('queen-typing-indicator')
    expect(indicator.textContent).toContain('Королева печатает')

    rerender(
      <HiveDashboard
        connection="connected"
      events={[
          hiveConversationPage('live', [
            { kind: 'assistant_message_finalized', taskId: 'task-typing-1', payload: { content: 'Улей работает штатно.' } },
            { kind: 'task_completed', taskId: 'task-typing-1', payload: { status: 'completed' } }
          ], 2),
          ...initialEvents
        ]}
      />
    )

    await waitFor(() => {
      expect(screen.queryByTestId('queen-typing-indicator')).toBeNull()
    })
    expect(within(screen.getByRole('log')).getByText('Улей работает штатно.')).toBeTruthy()
  })

  it('hive_chat_ignores_tasks_from_regular_conversations', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot()),
          event('task.started', { prompt: 'Обычный пользовательский запрос' }, 'regular-chat-task'),
          event('agent.message.delta', { content: 'Чужой ответ' }, 'regular-chat-task')
        ]}
      />
    )

    await openQueenChatDrawer()
    expect(screen.queryByTestId('queen-typing-indicator')).toBeNull()
    expect(screen.queryByText('Обычный пользовательский запрос')).toBeNull()
    expect(screen.queryByText('Чужой ответ')).toBeNull()
  })

  it('hive_chat_subscribes_to_its_own_conversation_log', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot())]}
      />
    )

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'core.subscribeConversationEvents',
        payload: expect.objectContaining({ conversationId: 'hive-system-chat', afterSequence: 0 })
      })
    })
  })

  it('ui_stops_typing_indicator_on_task_failed', async () => {
    const initialEvents = [
      event('hive_state.snapshot', snapshot()),
      hiveConversationPage('history', [
        { kind: 'task_started', taskId: 'task-fail-1', payload: { prompt: 'Проверь файлы песочницы' } }
      ])
    ]
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={initialEvents}
      />
    )

    await openQueenChatDrawer()
    const indicator = await screen.findByTestId('queen-typing-indicator')
    expect(indicator.textContent).toContain('Королева печатает')

    rerender(
      <HiveDashboard
        connection="connected"
      events={[
          hiveConversationPage('live', [
            { kind: 'assistant_message_failed', taskId: 'task-fail-1', payload: { error_code: 'permission_denied' } },
            { kind: 'task_failed', taskId: 'task-fail-1', payload: { error_code: 'permission_denied' } }
          ], 2),
          ...initialEvents
        ]}
      />
    )

    await waitFor(() => {
      expect(screen.queryByTestId('queen-typing-indicator')).toBeNull()
    })
    expect(screen.getByText(/Королева Ева: У меня нет прав на выполнение этого действия/)).toBeTruthy()
  })

  it('ui_typing_indicator_has_watchdog_timeout', async () => {
    vi.useFakeTimers()
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot()),
          hiveConversationPage('history', [
            { kind: 'task_started', taskId: 'task-watchdog-1', payload: { prompt: 'Проверь состояние улья' } }
          ])
        ]}
      />
    )

    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })
    fireEvent.click(screen.getByRole('button', { name: 'Диалог с Королевой Евой' }))
    expect(screen.getByTestId('queen-typing-indicator')).toBeTruthy()
    await act(async () => {
      vi.advanceTimersByTime(60_000)
    })
    expect(screen.queryByTestId('queen-typing-indicator')).toBeNull()
  })

  it('ui_stops_typing_indicator_when_core_rejects_the_message', async () => {
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot())]}
      />
    )
    await openQueenChatDrawer()
    const input = await screen.findByRole('textbox', { name: 'Сообщение Королеве Еве' })
    await userEvent.type(input, 'Запрос Улью')
    await userEvent.click(screen.getByRole('button', { name: 'Отправить' }))
    expect(await screen.findByTestId('queen-typing-indicator')).toBeTruthy()

    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          hiveConversationAcceptError('idempotency_conflict'),
          event('hive_state.snapshot', snapshot())
        ]}
      />
    )

    await waitFor(() => {
      expect(screen.queryByTestId('queen-typing-indicator')).toBeNull()
    })
    expect(screen.getByRole('alert').textContent).toContain('уже отправлено')
  })

  it('hive_chat_indicates_pending_send_without_leaking_other_chat_activity', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot()),
          event('task.started', { prompt: 'Чужая задача' }, 'another-chat-task')
        ]}
      />
    )
    await openQueenChatDrawer()
    const input = await screen.findByRole('textbox', { name: 'Сообщение Королеве Еве' })
    await userEvent.type(input, 'Запрос Улью')
    await userEvent.click(screen.getByRole('button', { name: 'Отправить' }))
    expect(await screen.findByTestId('queen-typing-indicator')).toBeTruthy()
    expect(screen.getByText('Запрос Улью')).toBeTruthy()
    expect(screen.queryByText('Чужая задача')).toBeNull()
  })

  it('ui_duplicates_approval_request_in_hive_chat', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot()),
          hiveConversationPage('history', [{
            kind: 'approval_required',
            taskId: 'task-apr-1',
            payload: {
              approval_id: 'apr-chat-1',
              tool_name: 'eva_workspace_git',
              permission: 'GitWrite',
              scope: 'origin/main',
              preview: { summary: 'Разрешить push изменений?' }
            }
          }])
        ]}
      />
    )

    await openQueenChatDrawer()
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
                requests_last_4_hours: 25,
                requests_last_day: 40,
                minute_cap: 7,
                hour_cap: 190,
                four_hour_cap: 600,
                day_cap: 0,
                limits: { rpm: 7, rph: 190, rp4h: 600, rpd: 0 },
                minute_exhausted: false,
                hour_exhausted: false,
                four_hour_exhausted: false,
                day_exhausted: false,
                daily_cost_available: false,
                daily_cost_nano_usd: null
              }
            })
          )
        ]}
      />
    )

    expect(await screen.findByText(/2 \/ 7 за минуту · 15 \/ 190 за час · 25 \/ 600 за 4 часа · 40 за сутки/)).toBeTruthy()
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
                fallback_model: 'openai/gpt-6-luna',
                fallback_tokens_remaining: 650000
              }
            })
          )
        ]}
        repair={null}
      />
    )
    expect(await screen.findByText('Резерв: Rout (650000 токенов) · Модель резерва: openai/gpt-6-luna')).toBeTruthy()

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
    // EvoHime no longer imposes a hidden Rout quota: request limits and the
    // daily token quota are user-configured like for any other provider.
    expect(routSchema.supportsModelLimits).toBe(true)
  })

  it('hive_logs_keep_latest_on_top_and_reset_scroll', async () => {
    const initialEvents: CoreEvent[] = [
      event('hive_state.snapshot', snapshot()),
      hiveConversationPage('history', [
        { kind: 'task_completed', taskId: 'task-old', payload: { status: 'completed' } }
      ])
    ]
    const { rerender } = render(<HiveDashboard connection="connected" events={initialEvents} />)

    await userEvent.setup().click(screen.getByText('Подробные логи'))

    const logsSection = await screen.findByLabelText('Логи улья')
    const list = logsSection.querySelector('.hive-log-list') as HTMLOListElement
    expect(list).toBeTruthy()
    list.scrollTop = 120

    const updatedEvents: CoreEvent[] = [
      hiveConversationPage('live', [
        { kind: 'task_failed', taskId: 'task-new', payload: { error_code: 'task_failed' } }
      ], 2),
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

  it('ui_typing_indicator_does_not_depend_on_pulse_status', async () => {
    const conversation = hiveConversationPage('history', [
      { kind: 'task_started', taskId: 'task-sleep-1', payload: { prompt: 'Проверь состояние улья' } }
    ])
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot({ pulse_status: 'active' })), conversation]}
      />
    )

    await openQueenChatDrawer()
    const indicator = await screen.findByTestId('queen-typing-indicator')
    expect(indicator.textContent).toContain('Королева печатает')

    // Pulse lifecycle и typing Королевы — независимые проекции: засыпание
    // Обновление статуса наблюдения не обрывает индикатор незавершённой задачи Улья.
    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({ observation_status: 'waiting_for_limits' })),
          event('hive.observation_status', {
            schema_version: 1,
            owner: 'hive_observation',
            status: 'waiting_for_limits',
            enabled: true,
            next_wake_at_ms: Date.now() + 60_000
          }),
          conversation
        ]}
      />
    )

    expect(await screen.findByTestId('queen-typing-indicator')).toBeTruthy()
    expect((await screen.findAllByText('Улей ожидает (лимиты)')).length).toBeGreaterThan(0)
  })

  it('hive_uses_dedicated_chat', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot())]}
      />
    )

    await openQueenChatDrawer()
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
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))

    const keyInput = screen.getByLabelText('API-ключ Улья (обязателен для работы Улья)') as HTMLInputElement
    expect(keyInput.type).toBe('password')

    await userEvent.type(keyInput, 'sk-hive-secret-123')
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить ключ' }))

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'core.setHiveSetting',
        payload: {
          key: 'hive_api_key',
          value: 'sk-hive-secret-123',
          provider: 'rout'
        }
      })
      expect(screen.getByText(/API-ключ Улья сохранён/)).toBeTruthy()
    })
  })

  it('ui_hides_api_key_when_configured', async () => {
    providerOverrides = { hiveApiKeyConfigured: true, hiveApiKeysConfigured: { rout: true } }
    render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot())]} />)

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))

    expect(within(dialog).queryByLabelText('API-ключ Улья (обязателен для работы Улья)')).toBeNull()
    expect(within(dialog).getByText('API-ключ сохранен ✓')).toBeTruthy()
    expect(within(dialog).getByRole('button', { name: 'Изменить' })).toBeTruthy()
  })

  it('ui_shows_hive_api_key_per_provider', async () => {
    providerOverrides = { hiveApiKeyConfigured: true, hiveApiKeysConfigured: { literouter: true } }
    render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot({ hive_provider_profile: 'literouter' }))]} />)

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))

    expect(within(dialog).getByText('API-ключ сохранен ✓')).toBeTruthy()
    expect(within(dialog).queryByLabelText('API-ключ Улья (обязателен для работы Улья)')).toBeNull()

    const providerSelect = within(dialog).getByLabelText('Провайдер Улья')
    await userEvent.selectOptions(providerSelect, 'deepseek')

    expect(within(dialog).queryByText('API-ключ сохранен ✓')).toBeNull()
    const keyInput = within(dialog).getByLabelText('API-ключ Улья (обязателен для работы Улья)') as HTMLInputElement
    await userEvent.type(keyInput, 'sk-deepseek-key')
    await userEvent.click(within(dialog).getByRole('button', { name: 'Сохранить ключ' }))

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'core.setHiveSetting',
        payload: {
          key: 'hive_api_key',
          value: 'sk-deepseek-key',
          provider: 'deepseek'
        }
      })
      expect(within(dialog).getByText('API-ключ сохранен ✓')).toBeTruthy()
    })

    await userEvent.selectOptions(providerSelect, 'openai_responses')
    expect(within(dialog).queryByText('API-ключ сохранен ✓')).toBeNull()
    expect(within(dialog).getByLabelText('API-ключ Улья (обязателен для работы Улья)')).toBeTruthy()
  })

  it('ui_switches_saved_model_and_limits_per_provider', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event(
            'hive_state.snapshot',
            snapshot({
              hive_provider_profile: 'literouter',
              hive_model: 'glm-5.3-flash-cheap:free',
              hive_provider_limits: { rpm: 10, rph: 0, rp4h: 0, rpd: 720 },
              hive_provider_configs: {
                literouter: {
                  model: 'glm-5.3-flash-cheap:free',
                  limits: { rpm: 10, rph: 0, rp4h: 0, rpd: 720 },
                  daily_token_quota: 50000,
                  quota_reset_utc_midnight: false,
                  reset_schedule: { enabled: false, time_minutes: 0, utc_offset_hours: 0 },
                  token_quota_configured: true
                },
                deepseek: {
                  model: 'deepseek-chat',
                  limits: { rpm: 60, rph: 0, rp4h: 0, rpd: 0 },
                  daily_token_quota: 0,
                  quota_reset_utc_midnight: false,
                  reset_schedule: { enabled: false, time_minutes: 0, utc_offset_hours: 0 },
                  token_quota_configured: false
                }
              }
            })
          )
        ]}
      />
    )

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))

    const providerSelect = within(dialog).getByLabelText('Провайдер Улья') as HTMLSelectElement
    const modelSelect = within(dialog).getByLabelText('Модель Улья') as HTMLSelectElement

    expect(providerSelect.value).toBe('literouter')
    expect(modelSelect.value).toBe('glm-5.3-flash-cheap:free')

    await userEvent.click(within(dialog).getByRole('tab', { name: 'Лимиты и бюджет' }))
    const getPrimaryRpm = () => {
      const primaryLimitsGroup = within(dialog).getByRole('group', { name: 'Лимиты основного маршрута Улья' })
      return within(primaryLimitsGroup).getByLabelText('Лимиты основного маршрута Улья RPM') as HTMLInputElement
    }
    expect(getPrimaryRpm().value).toBe('10')

    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))
    await userEvent.selectOptions(within(dialog).getByLabelText('Провайдер Улья'), 'deepseek')
    expect((within(dialog).getByLabelText('Модель Улья') as HTMLSelectElement).value).toBe('deepseek-chat')
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Лимиты и бюджет' }))
    expect(getPrimaryRpm().value).toBe('60')

    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))
    await userEvent.selectOptions(within(dialog).getByLabelText('Провайдер Улья'), 'openai_responses')
    expect((within(dialog).getByLabelText('Модель Улья') as HTMLSelectElement).value).toBe('')
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Лимиты и бюджет' }))
    expect(getPrimaryRpm().value).toBe('0')

    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))
    await userEvent.selectOptions(within(dialog).getByLabelText('Провайдер Улья'), 'literouter')
    expect((within(dialog).getByLabelText('Модель Улья') as HTMLSelectElement).value).toBe('glm-5.3-flash-cheap:free')
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Лимиты и бюджет' }))
    expect(getPrimaryRpm().value).toBe('10')
  })

  it('ui_shows_api_key_input_when_editing', async () => {
    providerOverrides = { hiveApiKeyConfigured: true, hiveApiKeysConfigured: { rout: true } }
    render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot())]} />)

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))
    await userEvent.click(within(dialog).getByRole('button', { name: 'Изменить' }))

    expect(within(dialog).getByLabelText('API-ключ Улья (обязателен для работы Улья)')).toBeTruthy()
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
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))

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
          value: 'sk-hive-fallback-456',
          provider: 'rout'
        }
      })
      expect(within(dialog).getByText(/Резервный API-ключ Улья сохранён/)).toBeTruthy()
    })
  })

  it('ui_clears_fallback_catalog_error_on_save', async () => {
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({ hive_fallback_provider_profile: 'rout' }))
        ]}
      />
    )

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))

    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({ hive_fallback_provider_profile: 'rout' })),
          event('hive.fallback_model.catalog', { profile: 'rout', models: [], error: 'provider_not_configured' })
        ]}
      />
    )

    expect(await within(dialog).findByText(/Провайдер не настроен для получения каталога моделей/)).toBeTruthy()

    const catalogCallsBeforeSave = calls.filter(
      (call) => call.command === 'core.listModelCatalog' && (call.payload as { mode?: string }).mode === 'hive_fallback_catalog'
    ).length

    const fallbackKeyInput = within(dialog).getByLabelText('Резервный API-ключ Улья') as HTMLInputElement
    await userEvent.type(fallbackKeyInput, 'sk-hive-fallback-456')
    await userEvent.click(within(dialog).getByRole('button', { name: 'Сохранить резервный ключ' }))

    await waitFor(() => {
      expect(within(dialog).getByText(/Резервный API-ключ Улья сохранён/)).toBeTruthy()
      expect(within(dialog).queryByText(/Провайдер не настроен для получения каталога моделей/)).toBeNull()
      const catalogCallsAfterSave = calls.filter(
        (call) => call.command === 'core.listModelCatalog' && (call.payload as { mode?: string }).mode === 'hive_fallback_catalog'
      ).length
      expect(catalogCallsAfterSave).toBeGreaterThan(catalogCallsBeforeSave)
    })
  })

  it('ui_can_save_hive_request_limits_and_token_quota', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot())]}
      />
    )

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Лимиты и бюджет' }))

    const primary = within(dialog).getByRole('group', { name: 'Лимиты основного маршрута Улья' })
    const rpm = within(primary).getByLabelText('Лимиты основного маршрута Улья RPM') as HTMLInputElement
    await userEvent.clear(rpm)
    await userEvent.type(rpm, '12')
    const quota = within(primary).getByLabelText('Дневная квота токенов (0 — без лимита)') as HTMLInputElement
    await userEvent.clear(quota)
    await userEvent.type(quota, '500000')
    await userEvent.selectOptions(within(primary).getByLabelText('Сброс лимитов'), 'scheduled')
    const resetTime = within(primary).getByLabelText('Время сброса') as HTMLInputElement
    await userEvent.clear(resetTime)
    await userEvent.type(resetTime, '03:30')
    await userEvent.selectOptions(within(primary).getByLabelText('Часовой пояс'), '2')

    await userEvent.click(within(dialog).getByRole('button', { name: 'Сохранить лимиты Улья' }))

    await waitFor(() => {
      const save = calls.find((call) =>
        call.command === 'core.setHiveSetting'
        && (call.payload as { key?: string }).key === 'hive_request_limits')
      expect(save).toBeTruthy()
      expect(JSON.parse((save?.payload as { value: string }).value)).toEqual({
        primary_profile: 'rout',
        fallback_profile: null,
        primary_limits: { rpm: 12, rph: 720, rp4h: 2880, rpd: 17280 },
        fallback_limits: { rpm: 0, rph: 0, rp4h: 0, rpd: 0 },
        primary_token_quota: 500_000,
        fallback_token_quota: 0,
        primary_quota_reset_utc_midnight: true,
        fallback_quota_reset_utc_midnight: false,
        primary_reset_schedule: { enabled: true, time_minutes: 210, utc_offset_hours: 2 },
        fallback_reset_schedule: { enabled: false, time_minutes: 0, utc_offset_hours: 0 }
      })
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

  it('позволяет выбрать режим бесплатных или платных моделей для LiteRouter в Настройках улья и оставляет в бесплатном режиме только модели с :free', async () => {
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot({ hive_provider_profile: 'literouter', hive_model: '' }))]}
      />
    )

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))

    const tierSelect = within(dialog).getByLabelText('Режим моделей Улья') as HTMLSelectElement
    expect(tierSelect.value).toBe('free')

    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'core.listModelCatalog',
        payload: { mode: 'hive_catalog', profile: 'literouter:free' }
      })
    })

    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({ hive_provider_profile: 'literouter', hive_model: '' })),
          event('hive.model.catalog', {
            profile: 'literouter',
            tier: 'free',
            models: [
              'glm-5.3-flash-cheap:free',
              'llama-3.3-70b-instruct-turbo-free',
              'deepseek-r1:free',
              'claude-3-7-sonnet'
            ],
            error: null
          })
        ]}
      />
    )

    const modelSelect = within(dialog).getByLabelText('Модель Улья') as HTMLSelectElement
    const freeOptionValues = Array.from(modelSelect.options).map((option) => option.value).filter(Boolean)
    expect(freeOptionValues).toEqual(['glm-5.3-flash-cheap:free', 'deepseek-r1:free'])
    expect(freeOptionValues.every((modelId) => modelId.endsWith(':free'))).toBe(true)

    await userEvent.selectOptions(tierSelect, 'paid')
    await waitFor(() => {
      expect(calls).toContainEqual({
        command: 'core.listModelCatalog',
        payload: { mode: 'hive_catalog', profile: 'literouter:paid' }
      })
    })

    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({ hive_provider_profile: 'literouter', hive_model: '' })),
          event('hive.model.catalog', {
            profile: 'literouter',
            tier: 'paid',
            models: [
              'glm-5.3-flash-cheap:free',
              'claude-3-7-sonnet',
              'gpt-4.1'
            ],
            error: null
          })
        ]}
      />
    )

    await waitFor(() => {
      const paidOptionValues = Array.from(modelSelect.options).map((option) => option.value).filter(Boolean)
      expect(paidOptionValues).toEqual(['claude-3-7-sonnet', 'gpt-4.1'])
    })
  })

  it('не подставляет статический список моделей Улья при ошибке каталога и показывает причину', async () => {
    const { rerender } = render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot({ hive_provider_profile: 'literouter', hive_model: '' }))]}
      />
    )

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    rerender(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({ hive_provider_profile: 'literouter', hive_model: '' })),
          event('hive.model.catalog', {
            profile: 'literouter',
            tier: 'free',
            models: [],
            error: 'provider_timeout'
          })
        ]}
      />
    )
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))
    const modelSelect = within(dialog).getByLabelText('Модель Улья') as HTMLSelectElement

    const catalogError = await within(dialog).findByRole('alert')
    expect(catalogError.textContent).toContain('Провайдер не ответил вовремя.')
    expect(Array.from(modelSelect.options).map((option) => option.value).filter(Boolean)).toEqual([])
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
      const failedEvent = hiveConversationPage('history', [
        { kind: 'task_failed', taskId: 'task-hive-42', payload: { error_code: 'provider_rate_limit' } }
      ])
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

      await user.click(screen.getByText('Подробные логи'))
      const copyButton = await screen.findByRole('button', { name: 'Копировать' })
      await user.click(copyButton)

      await waitFor(() => {
        expect(copied).toHaveLength(1)
        expect(screen.getByRole('button', { name: 'Скопировано' })).toBeTruthy()
      })
      expect(copied[0]).toContain('=== Логи Улья (EvoHime) ===')
      expect(copied[0]).toContain('Провайдер: literouter | Модель: llama-3.3-70b-instruct-turbo-free | Резерв: rout (650000 токенов)')
      expect(copied[0]).toContain('[WARNING] task.failed (seq=1, task=task-hive-42)')
      expect(copied[0]).toContain('provider_rate_limit')
    } finally {
      Object.defineProperty(window, 'evohime', {
        configurable: true,
        value: previousApi ? { v1: previousApi } : undefined
      })
    }
  })

  it('не показывает null токенов в резерве Rout при отсутствии fallback_tokens_remaining', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({
            budget: {
              ...snapshot().budget,
              provider: 'literouter',
              fallback_provider: 'rout',
              fallback_tokens_remaining: null
            }
          }))
        ]}
      />
    )

    expect(await screen.findByText(/Резерв: Rout \(0 токенов\)/)).toBeTruthy()
    expect(screen.queryByText(/null токенов/)).toBeNull()
  })

  it('показывает нейтральное Ожидание и скрывает пустую проверку окружения без данных наблюдения', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[event('hive_state.snapshot', snapshot({ active_agents: [], observation_status: '', next_wake_at_ms: null }))]}
      />
    )

    expect(await screen.findByText('Ожидание...')).toBeTruthy()
    expect(screen.queryByText(/нет данных Core/)).toBeNull()
    expect(screen.queryByText('Следующая проверка окружения')).toBeNull()
  })

  it('показывает Наблюдение активно при активном цикле без next_wake_at_ms', async () => {
    render(
      <HiveDashboard
        connection="connected"
        events={[
          event('hive_state.snapshot', snapshot({ active_agents: [], next_wake_at_ms: null })),
          event('hive.observation_status', {
            schema_version: 1,
            owner: 'hive_observation',
            status: 'observing',
            enabled: true,
            next_wake_at_ms: null
          })
        ]}
      />
    )

    expect(await screen.findByText('Наблюдение активно')).toBeTruthy()
    expect(screen.getByTestId('hive-city-scene').getAttribute('data-status')).toBe('observing')
  })

  it('ui_hides_api_key_input_while_provider_get_is_loading', async () => {
    let resolveProviderGet!: (value: CommandOutcome<'provider.get'>) => void
    const pendingProviderGet = new Promise<CommandOutcome<'provider.get'>>((resolve) => {
      resolveProviderGet = resolve
    })
    const previousApi = window.evohime
    Object.defineProperty(window, 'evohime', {
      configurable: true,
      value: Object.freeze({
        v1: {
          ...previousApi.v1,
          invoke: (async (command: RendererCommand, payload: unknown) => {
            calls.push({ command, payload })
            if (command === 'provider.get') return pendingProviderGet
            if (command === 'chat.open') return ok(null)
            return ok({ accepted: true })
          }) as EvoHimeApiV1['invoke']
        }
      })
    })

    render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot())]} />)

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))

    expect(within(dialog).queryByLabelText('API-ключ Улья (обязателен для работы Улья)')).toBeNull()
    expect(within(dialog).getByText('Загрузка настроек API-ключа…')).toBeTruthy()

    await act(async () => {
      resolveProviderGet(ok(providerSummary({ hiveApiKeyConfigured: true, hiveApiKeysConfigured: { rout: true } })) as CommandOutcome<'provider.get'>)
    })

    await waitFor(() => {
      expect(within(dialog).getByText('API-ключ сохранен ✓')).toBeTruthy()
    })
    expect(within(dialog).queryByLabelText('API-ключ Улья (обязателен для работы Улья)')).toBeNull()
  })

  it('ui_shows_error_and_retry_when_provider_get_fails_and_recovers_on_retry', async () => {
    let attempt = 0
    const previousApi = window.evohime
    Object.defineProperty(window, 'evohime', {
      configurable: true,
      value: Object.freeze({
        v1: {
          ...previousApi.v1,
          invoke: (async (command: RendererCommand, payload: unknown) => {
            calls.push({ command, payload })
            if (command === 'provider.get') {
              attempt += 1
              if (attempt === 1) {
                return { ok: false, code: 'core_unavailable', message: 'Ошибка загрузки настроек провайдера' }
              }
              return ok(providerSummary({ hiveApiKeyConfigured: true, hiveApiKeysConfigured: { rout: true } }))
            }
            if (command === 'chat.open') return ok(null)
            return ok({ accepted: true })
          }) as EvoHimeApiV1['invoke']
        }
      })
    })

    render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot())]} />)

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))

    expect(await within(dialog).findByText('Ошибка загрузки настроек провайдера')).toBeTruthy()
    expect(within(dialog).queryByLabelText('API-ключ Улья (обязателен для работы Улья)')).toBeNull()

    await userEvent.click(within(dialog).getByRole('button', { name: 'Повторить' }))

    await waitFor(() => {
      expect(within(dialog).getByText('API-ключ сохранен ✓')).toBeTruthy()
    })
  })

  it('ui_resets_editing_state_on_cancel', async () => {
    providerOverrides = { hiveApiKeyConfigured: true, hiveApiKeysConfigured: { rout: true } }
    render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot())]} />)

    await userEvent.click(await screen.findByRole('button', { name: 'Настройки улья' }))
    const dialog = await screen.findByRole('dialog', { name: 'Настройки улья' })
    await userEvent.click(within(dialog).getByRole('tab', { name: 'Провайдеры' }))

    await userEvent.click(within(dialog).getByRole('button', { name: 'Изменить' }))
    const keyInput = within(dialog).getByLabelText('API-ключ Улья (обязателен для работы Улья)') as HTMLInputElement
    await userEvent.type(keyInput, 'sk-temp')
    await userEvent.click(within(dialog).getByRole('button', { name: 'Отмена' }))

    expect(within(dialog).queryByLabelText('API-ключ Улья (обязателен для работы Улья)')).toBeNull()
    expect(within(dialog).getByText('API-ключ сохранен ✓')).toBeTruthy()
  })

  it('ui_opens_and_closes_queen_chat_drawer', async () => {
    render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot())]} />)

    expect(screen.queryByRole('textbox', { name: 'Сообщение Королеве Еве' })).toBeNull()

    await openQueenChatDrawer()
    expect(await screen.findByRole('textbox', { name: 'Сообщение Королеве Еве' })).toBeTruthy()

    await userEvent.click(screen.getByRole('button', { name: 'Закрыть диалог' }))
    expect(screen.queryByRole('textbox', { name: 'Сообщение Королеве Еве' })).toBeNull()

    await userEvent.click(screen.getByRole('button', { name: 'Написать Еве' }))
    expect(await screen.findByRole('textbox', { name: 'Сообщение Королеве Еве' })).toBeTruthy()

    await userEvent.keyboard('{Escape}')
    expect(screen.queryByRole('textbox', { name: 'Сообщение Королеве Еве' })).toBeNull()
  })

  it('ui_renders_department_scene_for_agent_filtering', async () => {
    render(<HiveDashboard connection="connected" events={[event('hive_state.snapshot', snapshot())]} />)

    expect(await screen.findByText('run-1:dev-01')).toBeTruthy()
    expect(screen.getByText('run-1:worker:hop2')).toBeTruthy()
    expect(screen.getByTestId('hive-city-scene')).toBeTruthy()
  })

  it('ui_clears_hive_logs_and_chat_visually_without_deleting_backend_state', async () => {
    const events = [
      event('hive_state.snapshot', snapshot()),
      event('hive.evolution_dispatched', { task_id: 'eva-pulse-1', items_count: 2 })
    ]

    render(<HiveDashboard connection="connected" events={events} />)

    const logSection = screen.getByRole('region', { name: 'Логи улья' })
    expect(within(logSection).getByText(/hive\.evolution_dispatched/)).toBeTruthy()
    await userEvent.click(within(logSection).getByRole('button', { name: 'Очистить' }))
    expect(within(logSection).queryByText(/hive\.evolution_dispatched/)).toBeNull()
    expect(within(logSection).getByText('Событий пока нет.')).toBeTruthy()

    await openQueenChatDrawer()
    const input = await screen.findByLabelText('Сообщение Королеве Еве')
    await userEvent.type(input, 'Привет Королева{enter}')

    const chatDrawer = screen.getByRole('complementary', { name: 'Диалог с Королевой Евой' })
    expect(await within(chatDrawer).findByText('Привет Королева')).toBeTruthy()
    const callsAfterSend = calls.length
    await userEvent.click(within(chatDrawer).getByRole('button', { name: 'Очистить' }))
    expect(within(chatDrawer).queryByText('Привет Королева')).toBeNull()
    expect(calls.length).toBe(callsAfterSend)
  })

  it('hides_stale_approval_when_ledger_decision_or_recovery_blocked_exists', () => {
    const events = [
      event('hive_state.snapshot', snapshot()),
      hiveConversationPage('history', [
        {
          kind: 'approval_required',
          taskId: 'eva-pulse-stale-1',
          sequence: 10,
          payload: {
            approval_id: 'apr-stale-1',
            tool_name: 'shell_execute',
            permission: 'ShellExecute',
            scope: 'workspace',
            preview: { summary: 'Запустить команду в workspace' }
          }
        },
        {
          kind: 'assistant_message_delta',
          taskId: 'eva-pulse-stale-1',
          sequence: 11,
          payload: { content: 'Проверяю...' }
        }
      ]),
      event('ledger.approval_decision', {
        body: {
          kind: 'approval_decision',
          approval_intent_id: 'apr-stale-1',
          decision: 'approved'
        }
      }, 'eva-pulse-stale-1'),
      event('run.recovery.blocked', {
        decision: 'blocked',
        reason: 'recovery_unknown_effect'
      }, 'eva-pulse-stale-1')
    ]

    render(<HiveDashboard connection="connected" events={events} />)

    expect(screen.queryByLabelText('Запрос подтверждения')).toBeNull()
    expect(screen.queryByText('Королева печатает')).toBeNull()
  })

  it('hides_approval_when_conversation_log_contains_approval_decision', () => {
    const events = [
      event('hive_state.snapshot', snapshot()),
      hiveConversationPage('history', [
        {
          kind: 'approval_required',
          taskId: 'eva-pulse-stale-2',
          sequence: 20,
          payload: {
            approval_id: 'apr-stale-2',
            tool_name: 'shell_execute',
            permission: 'ShellExecute',
            scope: 'workspace',
            preview: { summary: 'Запустить команду в workspace' }
          }
        },
        {
          kind: 'approval_decision',
          taskId: 'eva-pulse-stale-2',
          sequence: 21,
          payload: {
            approval_id: 'apr-stale-2',
            granted: false
          }
        }
      ])
    ]

    render(<HiveDashboard connection="connected" events={events} />)

    expect(screen.queryByLabelText('Запрос подтверждения')).toBeNull()
  })

})
