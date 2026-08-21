// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { WorkflowPanel } from '../src/renderer/src/WorkflowPanel'

/**
 * Панель составных задач показывает проекцию ядра и ничего не вычисляет сама.
 * Тесты проверяют именно это: какие команды уходят, что рисуется из ответа и
 * что панель говорит вслух, когда ядро недоступно или состояние неизвестно.
 */

const WORKSPACE = 'C:\work'

const calls: Array<{ command: string; payload: unknown }> = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function event(eventType: string, payload: Record<string, unknown>): CoreEvent {
  return { sequenceId: 0, taskId: '', eventType, payload: JSON.stringify(payload) }
}

const templates = event('workflow.templates', {
  error_code: '',
  templates: [
    {
      template_id: 'repository-research',
      version: 1,
      display_name: 'Исследование репозитория',
      description: 'Собирает контекст и отвечает на вопрос',
      inputs: [{ name: 'question', title: 'Вопрос по репозиторию', required: true, max_chars: 512 }],
      required_capabilities: ['workspace.knowledge'],
      schedule_eligibility: 'interval_only',
      preview: ['context: контекст рабочего каталога (read-only)'],
      node_count: 4
    },
    {
      template_id: 'plan-implement-review',
      version: 1,
      display_name: 'План → реализация → ревью',
      description: 'План, подтверждение, реализация и ревью',
      inputs: [{ name: 'goal', title: 'Цель задачи', required: true, max_chars: 512 }],
      required_capabilities: ['child.planner'],
      schedule_eligibility: 'unavailable',
      preview: ['planner: child-планировщик'],
      node_count: 4
    }
  ]
})

function runProjection(state: string, nodeState: string): CoreEvent {
  return event('workflow.run', {
    run_id: 'run-1',
    task_id: 'task-1',
    template_id: 'repository-research',
    template_version: 1,
    graph_id: 'template.repository-research',
    graph_version: 1,
    graph_hash: 'a'.repeat(64),
    state,
    terminal_reason: '',
    created_at_ms: 1,
    updated_at_ms: 2,
    error_code: '',
    nodes: [
      {
        node_id: 'context',
        action_kind: 'context_provider',
        role: 'workspace.knowledge',
        state: nodeState,
        attempts: 1,
        error_code: '',
        message: '',
        approval_id: '',
        dependencies: []
      },
      {
        node_id: 'researcher',
        action_kind: 'child',
        role: 'researcher',
        state: 'pending',
        attempts: 0,
        error_code: '',
        message: '',
        approval_id: '',
        dependencies: ['evidence']
      }
    ]
  })
}

beforeEach(() => {
  calls.length = 0
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
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

describe('панель составных задач', () => {
  it('запрашивает каталог у ядра и показывает версию и пригодность к расписанию', async () => {
    render(<WorkflowPanel connection="connected" events={[templates]} workspace={WORKSPACE} />)
    await waitFor(() =>
      expect(calls.some((call) => call.command === 'workflow.listTemplates')).toBe(true)
    )
    expect(screen.getByRole('button', { name: 'Исследование репозитория' })).toBeTruthy()
    expect(screen.getByText(/версия 1 · узлов 4 · расписание: только интервал/)).toBeTruthy()
    expect(screen.getByText(/расписание недоступно/)).toBeTruthy()
  })

  it('не запускает шаблон без обязательного входа и без рабочей папки', async () => {
    render(<WorkflowPanel connection="connected" events={[templates]} workspace={WORKSPACE} />)
    await userEvent.click(screen.getByRole('button', { name: 'Исследование репозитория' }))
    await userEvent.click(screen.getByRole('button', { name: 'Запустить' }))
    expect(screen.getByRole('alert').textContent).toContain('Вопрос по репозиторию')
    expect(calls.some((call) => call.command === 'workflow.start')).toBe(false)

    cleanup()
    render(<WorkflowPanel connection="connected" events={[templates]} workspace={null} />)
    await userEvent.click(screen.getByRole('button', { name: 'Исследование репозитория' }))
    await userEvent.type(screen.getByRole('textbox'), 'вопрос')
    await userEvent.click(screen.getByRole('button', { name: 'Запустить' }))
    expect(screen.getByRole('alert').textContent).toContain('рабочую папку')
    expect(calls.some((call) => call.command === 'workflow.start')).toBe(false)
  })

  it('отправляет запуск с ключом идемпотентности и рабочей папкой', async () => {
    render(<WorkflowPanel connection="connected" events={[templates]} workspace={WORKSPACE} />)
    await userEvent.click(screen.getByRole('button', { name: 'Исследование репозитория' }))
    await userEvent.type(screen.getByRole('textbox'), 'вопрос')
    await userEvent.click(screen.getByRole('button', { name: 'Запустить' }))
    const start = calls.find((call) => call.command === 'workflow.start')
    expect(start).toBeTruthy()
    expect(start?.payload).toMatchObject({
      templateId: 'repository-research',
      workspacePath: WORKSPACE,
      inputs: { question: 'вопрос' }
    })
    expect(
      (start?.payload as { idempotencyKey: string }).idempotencyKey.length
    ).toBeGreaterThan(0)
  })

  it('рисует узлы, зависимости и состояния из проекции ядра', async () => {
    const events = [
      templates,
      event('workflow.started', { run_id: 'run-1', error_code: '' }),
      runProjection('running', 'succeeded')
    ]
    render(<WorkflowPanel connection="connected" events={events} workspace={WORKSPACE} />)
    await waitFor(() => expect(screen.getByLabelText('Текущий запуск')).toBeTruthy())
    expect(screen.getByText(/состояние: выполняется/)).toBeTruthy()
    expect(screen.getByText(/context_provider \(workspace.knowledge\) · успешно/)).toBeTruthy()
    expect(screen.getByText(/зависит от: evidence/)).toBeTruthy()
  })

  it('называет ожидание подтверждения и не предлагает своей кнопки approval', async () => {
    const events = [
      templates,
      event('workflow.started', { run_id: 'run-1', error_code: '' }),
      runProjection('waiting_approval', 'waiting_approval')
    ]
    render(<WorkflowPanel connection="connected" events={events} workspace={WORKSPACE} />)
    await waitFor(() => expect(screen.getByLabelText('Текущий запуск')).toBeTruthy())
    expect(screen.getAllByText(/ждёт подтверждения/).length).toBeGreaterThan(0)
    expect(screen.queryByRole('button', { name: /Подтвердить/ })).toBeNull()
  })

  /** Неизвестное состояние называется словами, а не изображается успехом. */
  it('честно показывает неизвестное состояние и ошибку запуска', async () => {
    const events = [
      templates,
      event('workflow.started', { run_id: '', error_code: 'missing_input' })
    ]
    render(<WorkflowPanel connection="connected" events={events} workspace={WORKSPACE} />)
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('missing_input'))

    cleanup()
    const unknown = [
      templates,
      event('workflow.started', { run_id: 'run-1', error_code: '' }),
      event('workflow.run', {
        run_id: 'run-1',
        nodes: [],
        state: 'unknown_state',
        error_code: 'unknown_run'
      })
    ]
    render(<WorkflowPanel connection="connected" events={unknown} workspace={WORKSPACE} />)
    await waitFor(() =>
      expect(screen.getByText(/состояние: состояние неизвестно/)).toBeTruthy()
    )
  })

  /**
   * Событие неизвестного типа не должно ломать список: additive-протокол
   * означает, что старый клиент увидит незнакомое имя и продолжит работать.
   */
  it('переживает неизвестное состояние узла и неизвестное событие', async () => {
    const events = [
      templates,
      event('workflow.started', { run_id: 'run-1', error_code: '' }),
      runProjection('running', 'brand_new_state'),
      event('workflow.events', {
        run_id: 'run-1',
        error_code: '',
        events: [
          { sequence: 0, node_id: '', event_type: 'workflow.run_started', payload: '{}', created_at_ms: 1 },
          { sequence: 1, node_id: 'context', event_type: 'workflow.future_event', payload: '{}', created_at_ms: 2 }
        ]
      })
    ]
    render(<WorkflowPanel connection="connected" events={events} workspace={WORKSPACE} />)
    await waitFor(() => expect(screen.getByLabelText('Текущий запуск')).toBeTruthy())
    expect(screen.getByText(/неизвестно \(brand_new_state\)/)).toBeTruthy()
    expect(screen.getByText(/#1 workflow.future_event · context/)).toBeTruthy()
  })

  it('при недоступном ядре говорит об этом и не шлёт команд', async () => {
    render(<WorkflowPanel connection="disconnected" events={[templates]} workspace={WORKSPACE} />)
    expect(screen.getByText(/Ядро недоступно/)).toBeTruthy()
    expect(calls).toHaveLength(0)
  })
})
