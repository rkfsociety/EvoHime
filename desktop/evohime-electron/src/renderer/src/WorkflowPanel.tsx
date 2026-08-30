import { useCallback, useEffect, useMemo, useState } from 'react'

import type {
  ConnectionState,
  CoreEvent,
  WorkflowEventList,
  WorkflowDefinition,
  WorkflowRunProjection,
  WorkflowTemplateList,
  WorkflowTemplateSummary
} from '@shared/api'

import { useShellApi } from './shell-api'

/**
 * Панель составных задач (план 06.3).
 *
 * Панель ничего не планирует и не выполняет: она показывает проекцию Core и
 * отправляет ровно три намерения — «показать шаблоны», «запустить», «отменить».
 * Порядок узлов, зависимости, повторы и подтверждения принадлежат ядру;
 * подтверждение узла решается той же карточкой approval, что и у инструментов.
 *
 * Раскладка узлов — стабильный topological order, который прислал Core:
 * визуального редактора графа ещё нет, и придумывать своё расположение
 * панель не имеет права.
 */

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

/** Как часто панель перезапрашивает проекцию активного запуска. */
const POLL_MS = 2_000

const RUN_STATE_LABELS: Readonly<Record<string, string>> = {
  pending: 'ожидает',
  running: 'выполняется',
  waiting_approval: 'ждёт подтверждения',
  completed: 'завершён',
  failed: 'неуспешно',
  cancelled: 'отменён',
  degraded: 'частичный результат',
  interrupted: 'прервано, исход неизвестен',
  unknown_state: 'состояние неизвестно'
}

const NODE_STATE_LABELS: Readonly<Record<string, string>> = {
  pending: 'ожидает',
  ready: 'готов',
  running: 'выполняется',
  waiting_approval: 'ждёт подтверждения',
  succeeded: 'успешно',
  failed: 'ошибка',
  timed_out: 'таймаут',
  cancelled: 'отменён',
  blocked: 'заблокирован',
  denied: 'отклонено',
  skipped: 'пропущен',
  degraded: 'частичный результат',
  unknown_outcome: 'исход неизвестен',
  dead_letter: 'исчерпаны повторы'
}

const SCHEDULE_LABELS: Readonly<Record<string, string>> = {
  interval_only: 'расписание: только интервал',
  unavailable: 'расписание недоступно'
}

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly workspace: string | null
}

// `events` holds the newest event first (App.tsx prepends on receipt), so
// the latest match is the FIRST one found here — not the last.
function latestPayload<T>(events: readonly CoreEvent[], eventType: string): T | null {
  const event = events.find((item) => item.eventType === eventType)
  if (!event) return null
  try {
    return JSON.parse(event.payload) as T
  } catch {
    return null
  }
}

/** Неизвестное состояние называется словами, а не выдаётся за успех. */
function runStateLabel(state: string): string {
  return RUN_STATE_LABELS[state] ?? `неизвестное состояние (${state})`
}

function nodeStateLabel(state: string): string {
  return NODE_STATE_LABELS[state] ?? `неизвестно (${state})`
}

export function WorkflowPanel({ connection, events, workspace }: Props): React.JSX.Element {
  const api = useShellApi()
  const connected = CONNECTED_STATES.includes(connection)
  const [selected, setSelected] = useState<string | null>(null)
  const [inputs, setInputs] = useState<Record<string, string>>({})
  const [runId, setRunId] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)

  const catalog = latestPayload<WorkflowTemplateList>(events, 'workflow.templates')
  const started = latestPayload<{ run_id: string; error_code: string }>(events, 'workflow.started')
  const run = latestPayload<WorkflowRunProjection>(events, 'workflow.run')
  const eventList = latestPayload<WorkflowEventList>(events, 'workflow.events')
  const definition = latestPayload<WorkflowDefinition>(events, 'workflow.definition')
  const presetResult = latestPayload<{ status?: string; presets?: { id: string; revision: number; content_hash: string; state: string }[] }>(events, 'invocation_preset.result')

  const templates: readonly WorkflowTemplateSummary[] = catalog?.templates ?? []
  const template = useMemo(
    () => templates.find((item) => item.template_id === selected) ?? null,
    [templates, selected]
  )

  useEffect(() => {
    if (!api || !connected) return
    void api.invoke('workflow.listTemplates', {})
    if (selected) void api.invoke('workflow.getDefinition', { templateId: selected })
    if (workspace) void api.invoke('invocationPreset.list', { requestId: `preset-list:${workspace}`, ownerScope: workspace, limit: 50 })
  }, [api, connected, workspace, selected])

  // Идентификатор запуска приходит ответом ядра, а не придумывается панелью.
  useEffect(() => {
    if (started && started.error_code === '' && started.run_id) {
      setRunId(started.run_id)
      setNotice(null)
    } else if (started && started.error_code !== '') {
      setNotice(`Ядро отклонило запуск: ${started.error_code}`)
    }
  }, [started])

  // Опрос, а не собственный расчёт прогресса: панель не знает, когда узел
  // закончится, и не должна изображать движение.
  useEffect(() => {
    if (!api || !connected || !runId) return
    const poll = (): void => {
      void api.invoke('workflow.getRun', { runId })
      void api.invoke('workflow.listEvents', { runId, afterSequence: -1, limit: 200 })
    }
    poll()
    const timer = setInterval(poll, POLL_MS)
    return () => clearInterval(timer)
  }, [api, connected, runId])

  const start = useCallback(async () => {
    if (!api || !template) return
    if (!workspace) {
      setNotice('Сначала выбери рабочую папку.')
      return
    }
    const missing = template.inputs
      .filter((input) => input.required && (inputs[input.name] ?? '').trim() === '')
      .map((input) => input.title)
    if (missing.length > 0) {
      setNotice(`Заполни обязательные поля: ${missing.join(', ')}`)
      return
    }
    const outcome = await api.invoke('workflow.start', {
      templateId: template.template_id,
      workspacePath: workspace,
      inputs,
      // Ключ идемпотентности берётся из содержимого запроса: повторный клик
      // возвращает тот же запуск, а не создаёт второй.
      idempotencyKey: `${template.template_id}:${template.version}:${JSON.stringify(inputs)}`
    })
    if (!outcome.ok) setNotice(outcome.message)
  }, [api, template, inputs, workspace])

  const cancel = useCallback(async () => {
    if (!api || !runId) return
    const outcome = await api.invoke('workflow.cancel', { runId })
    if (!outcome.ok) setNotice(outcome.message)
  }, [api, runId])

  const activeRun = run && runId && run.run_id === runId ? run : null
  const waitingNodes = activeRun?.nodes.filter((node) => node.state === 'waiting_approval') ?? []

  return (
    <section className="settings-info workflow" aria-label="Составные задачи">
      <h3>Составные задачи</h3>
      <p>
        Шаблон принадлежит ядру: оболочка показывает его версию и входы, но не редактирует граф и не
        решает, какой узел выполнить следующим.
      </p>

      {!connected ? (
        <p role="status">Ядро недоступно — список шаблонов и состояние запуска не обновляются.</p>
      ) : null}

      <h4>Шаблоны</h4>
      {templates.length === 0 ? (
        <p role="status">Шаблоны ещё не получены от ядра.</p>
      ) : (
        <ul className="workflow__templates">
          {templates.map((item) => (
            <li key={item.template_id}>
              <button
                type="button"
                aria-pressed={item.template_id === selected}
                onClick={() => {
                  setSelected(item.template_id)
                  setInputs({})
                  setNotice(null)
                }}
              >
                {item.display_name}
              </button>
              <small>
                версия {item.version} · узлов {item.node_count} ·{' '}
                {SCHEDULE_LABELS[item.schedule_eligibility] ?? item.schedule_eligibility}
              </small>
            </li>
          ))}
        </ul>
      )}

      {template ? (
        <div className="workflow__template" aria-label={`Шаблон ${template.display_name}`}>
          <h4>{template.display_name}</h4>
          <p>{template.description}</p>
          <ul className="workflow__preview">
            {template.preview.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
          <p>
            <small>требуются возможности: {template.required_capabilities.join(', ')}</small>
          </p>
          {template.inputs.map((input) => (
            <label key={input.name} className="workflow__input">
              <span>
                {input.title}
                {input.required ? ' *' : ''}
              </span>
              <input
                type="text"
                maxLength={input.max_chars}
                value={inputs[input.name] ?? ''}
                onChange={(event) =>
                  setInputs((current) => ({ ...current, [input.name]: event.target.value }))
                }
              />
            </label>
          ))}
          <button type="button" disabled={!api || !connected} onClick={() => void start()}>
            Запустить
          </button>
        </div>
      ) : null}

      <h4>Пресеты запусков</h4>
      <p>
        Пресет сохраняет только проверенные значения и ссылки на credentials. Версия workflow и
        revision остаются зафиксированы ядром.
      </p>
      {presetResult?.presets?.length ? (
        <ul className="workflow__presets">
          {presetResult.presets.map((preset) => (
            <li key={`${preset.id}:${preset.revision}`}>
              <strong>{preset.id}</strong> · revision {preset.revision} · {preset.state}
              <small> · {preset.content_hash}</small>
              <button
                type="button"
                disabled={!api || !connected || preset.state !== 'ready'}
                onClick={() =>
                  void api?.invoke('invocationPreset.command', {
                    requestId: `preset-run:${preset.id}:${preset.revision}`,
                    ownerScope: workspace ?? '',
                    operation: 'run',
                    idempotencyKey: `preset-run:${preset.id}:${preset.revision}`,
                    payload: JSON.stringify({
                      preset_id: preset.id,
                      revision: preset.revision,
                      workspace_path: workspace ?? '',
                      temporary_overrides: {}
                    })
                  })
                }
              >
                Запустить
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p role="status">Сохранённых пресетов нет.</p>
      )}
      {template && workspace && definition?.template_id === template.template_id ? (
        <button
          type="button"
          disabled={!api || !connected}
          onClick={() => {
            const presetId = `${template.template_id}:${template.version}`
            void api?.invoke('invocationPreset.command', {
              requestId: `preset-create:${presetId}`,
              ownerScope: workspace,
              operation: 'create',
              idempotencyKey: `preset-create:${presetId}`,
              payload: JSON.stringify({
                schema_version: 1,
                id: presetId,
                owner_scope: workspace,
                name: template.display_name,
                description: template.description,
                workflow_id: template.template_id,
                workflow_version: Number(template.version) || 1,
                workflow_definition_hash: definition.graph_hash,
                input_schema_hash: definition.graph_hash,
                input_values: inputs,
                credential_bindings: {},
                execution_options: {},
                created_from_run_id: null,
                revision: 1,
                created_at_ms: Date.now(),
                updated_at_ms: Date.now(),
                content_hash: '',
                state: 'ready'
              })
            })
          }}
        >
          Сохранить текущие входы как пресет
        </button>
      ) : null}

      {notice ? (
        <p className="listening__error" role="alert">
          {notice}
        </p>
      ) : null}

      {runId ? (
        <div className="workflow__run" aria-label="Текущий запуск">
          <h4>Запуск {runId}</h4>
          <p role="status">
            состояние: {runStateLabel(activeRun?.state ?? 'unknown_state')}
            {activeRun?.terminal_reason ? ` · ${activeRun.terminal_reason}` : ''}
          </p>
          {waitingNodes.length > 0 ? (
            <p role="status">
              Узел ждёт подтверждения: реши карточку подтверждения — отдельной кнопки у workflow
              нет.
            </p>
          ) : null}
          <ol className="workflow__nodes">
            {(activeRun?.nodes ?? []).map((node) => (
              <li key={node.node_id}>
                <strong>{node.node_id}</strong>
                <span>
                  {' '}
                  · {node.action_kind}
                  {node.role ? ` (${node.role})` : ''} · {nodeStateLabel(node.state)}
                  {node.attempts > 0 ? ` · попыток: ${node.attempts}` : ''}
                </span>
                {node.dependencies.length > 0 ? (
                  <small> зависит от: {node.dependencies.join(', ')}</small>
                ) : null}
                {node.error_code ? <small> код: {node.error_code}</small> : null}
              </li>
            ))}
          </ol>
          <button type="button" disabled={!api || !connected} onClick={() => void cancel()}>
            Отменить запуск
          </button>
          <h4>События</h4>
          <ul className="workflow__events">
            {(eventList?.run_id === runId ? eventList.events : []).map((event) => (
              <li key={event.sequence}>
                #{event.sequence} {event.event_type}
                {event.node_id ? ` · ${event.node_id}` : ''}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </section>
  )
}
