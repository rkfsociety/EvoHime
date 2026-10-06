import { translate } from './i18n'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import type {
  ConnectionState,
  CoreEvent,
  WorkflowEventEntry,
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
const POLL_TIMEOUT_MS = 5_000
const EVENT_PAGE_SIZE = 200
const TERMINAL_RUN_STATES = new Set(['completed', 'failed', 'cancelled', 'degraded', 'interrupted'])

interface WorkflowPollCycle {
  readonly baselineSequence: number
  readonly coreIdentity: string
  runSeen: boolean
  eventsSeen: boolean
  eventPageLength: number
  runState: string
}

interface WorkflowPollControl {
  readonly runId: string
  cycle: WorkflowPollCycle | null
  timer: ReturnType<typeof setTimeout> | null
  poll: () => void
}

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
  return RUN_STATE_LABELS[state] ?? `${translate('Неизвестное состояние')} (${state})`
}

function nodeStateLabel(state: string): string {
  return NODE_STATE_LABELS[state] ?? `${translate('неизвестно')} (${state})`
}

export function WorkflowPanel({ connection, events, workspace }: Props): React.JSX.Element {
  const api = useShellApi()
  const connected = CONNECTED_STATES.includes(connection)
  const [selected, setSelected] = useState<string | null>(null)
  const [inputs, setInputs] = useState<Record<string, string>>({})
  const [runId, setRunId] = useState<string | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [workflowEvents, setWorkflowEvents] = useState<readonly WorkflowEventEntry[]>([])
  const workflowEventCursor = useRef(-1)
  const workflowEventMap = useRef(new Map<number, WorkflowEventEntry>())
  const latestEnvelopeSequence = useRef(0)
  const latestCoreIdentity = useRef('legacy:0')
  const workflowPollControl = useRef<WorkflowPollControl | null>(null)

  const catalog = latestPayload<WorkflowTemplateList>(events, 'workflow.templates')
  const started = latestPayload<{ run_id: string; error_code: string }>(events, 'workflow.started')
  const run = latestPayload<WorkflowRunProjection>(events, 'workflow.run')
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

  useEffect(() => {
    workflowEventCursor.current = -1
    workflowEventMap.current.clear()
    setWorkflowEvents([])
    setNotice(null)
  }, [runId])

  useEffect(() => {
    const newest = events[0]
    if (!newest) return
    const identity = `${newest.coreInstanceId ?? 'legacy'}:${newest.sessionEpoch ?? 0}`
    if (identity !== latestCoreIdentity.current) {
      latestCoreIdentity.current = identity
      latestEnvelopeSequence.current = events
        .filter((event) => `${event.coreInstanceId ?? 'legacy'}:${event.sessionEpoch ?? 0}` === identity)
        .reduce((latest, event) => Math.max(latest, event.sequenceId), 0)
      return
    }
    latestEnvelopeSequence.current = events.reduce((latest, event) => {
      const eventIdentity = `${event.coreInstanceId ?? 'legacy'}:${event.sessionEpoch ?? 0}`
      return eventIdentity === identity ? Math.max(latest, event.sequenceId) : latest
    }, latestEnvelopeSequence.current)
  }, [events])

  useEffect(() => {
    const latestPageEvent = events.find((event) => event.eventType === 'workflow.events')
    if (latestPageEvent) {
      try {
        const page = JSON.parse(latestPageEvent.payload) as WorkflowEventList
        if (page.run_id === runId && page.error_code === '') {
          for (const entry of page.events) {
            if (entry.sequence > workflowEventCursor.current) workflowEventMap.current.set(entry.sequence, entry)
          }
          const ordered = [...workflowEventMap.current.values()].sort((left, right) => left.sequence - right.sequence)
          const lastSequence = ordered.at(-1)?.sequence
          if (lastSequence !== undefined) workflowEventCursor.current = Math.max(workflowEventCursor.current, lastSequence)
          setWorkflowEvents(ordered)
        }
      } catch {
        // Ignore malformed event-list payloads and wait for a fresh Core projection.
      }
    }
    const control = workflowPollControl.current
    if (!control || control.runId !== runId || !control.cycle) return
    const cycle = control.cycle
    if (cycle.coreIdentity !== latestCoreIdentity.current) {
      if (control.timer) clearTimeout(control.timer)
      control.cycle = null
      control.timer = setTimeout(() => {
        control.timer = null
        control.poll()
      }, 0)
      return
    }
    const fresh = events
      .filter((event) => event.sequenceId > cycle.baselineSequence
        && `${event.coreInstanceId ?? 'legacy'}:${event.sessionEpoch ?? 0}` === cycle.coreIdentity)
      .slice()
      .reverse()
    for (const event of fresh) {
      if (event.eventType !== 'workflow.run' && event.eventType !== 'workflow.events') continue
      let payload: WorkflowRunProjection | WorkflowEventList
      try { payload = JSON.parse(event.payload) as WorkflowRunProjection | WorkflowEventList } catch { continue }
      if (payload.run_id !== runId) continue
      if (event.eventType === 'workflow.run') {
        const runPayload = payload as WorkflowRunProjection
        cycle.runSeen = true
        cycle.runState = runPayload.state
      } else {
        const page = payload as WorkflowEventList
        cycle.eventsSeen = true
        cycle.eventPageLength = page.error_code === '' ? page.events.length : EVENT_PAGE_SIZE
        if (page.error_code === '') {
          for (const entry of page.events) {
            if (entry.sequence > workflowEventCursor.current) workflowEventMap.current.set(entry.sequence, entry)
          }
          const ordered = [...workflowEventMap.current.values()].sort((left, right) => left.sequence - right.sequence)
          const lastSequence = ordered.at(-1)?.sequence
          if (lastSequence !== undefined) workflowEventCursor.current = Math.max(workflowEventCursor.current, lastSequence)
          setWorkflowEvents(ordered)
        }
      }
    }
    if (cycle.runSeen && cycle.eventsSeen) {
      if (control.timer) clearTimeout(control.timer)
      control.cycle = null
      if (TERMINAL_RUN_STATES.has(cycle.runState) && cycle.eventPageLength < EVENT_PAGE_SIZE) {
        control.timer = null
      } else {
        control.timer = setTimeout(() => {
          control.timer = null
          control.poll()
        }, cycle.eventPageLength >= EVENT_PAGE_SIZE ? 0 : POLL_MS)
      }
    }
  }, [events, runId])

  // Опрос, а не собственный расчёт прогресса: панель не знает, когда узел
  // закончится, и не должна изображать движение.
  useEffect(() => {
    if (!api || !connected || !runId) return
    const control: WorkflowPollControl = {
      runId,
      cycle: null,
      timer: null,
      poll: () => undefined
    }
    const retry = (message?: string): void => {
      if (control.timer) clearTimeout(control.timer)
      control.cycle = null
      if (message) setNotice(message)
      control.timer = setTimeout(() => {
        control.timer = null
        control.poll()
      }, POLL_MS)
    }
    control.poll = () => {
      if (control.cycle) return
      const cycle: WorkflowPollCycle = {
        baselineSequence: latestEnvelopeSequence.current,
        coreIdentity: latestCoreIdentity.current,
        runSeen: false,
        eventsSeen: false,
        eventPageLength: 0,
        runState: 'unknown_state'
      }
      control.cycle = cycle
      control.timer = setTimeout(() => retry('Ядро не вернуло полную проекцию запуска; повторяю запрос.'), POLL_TIMEOUT_MS)
      void api.invoke('workflow.getRun', { runId }).then((outcome) => {
        if (!outcome.ok && control.cycle === cycle) retry(outcome.message)
      })
      void api.invoke('workflow.listEvents', {
        runId,
        afterSequence: workflowEventCursor.current,
        limit: EVENT_PAGE_SIZE
      }).then((outcome) => {
        if (!outcome.ok && control.cycle === cycle) retry(outcome.message)
      })
    }
    workflowPollControl.current = control
    control.poll()
    return () => {
      if (control.timer) clearTimeout(control.timer)
      control.cycle = null
      if (workflowPollControl.current === control) workflowPollControl.current = null
    }
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
    <section className="settings-info workflow" aria-label={translate("Составные задачи")}>
      <h3>{translate("Составные задачи")}</h3>
      <p>
        {translate("Шаблон принадлежит ядру: оболочка показывает его версию и входы, но не редактирует граф и не решает, какой узел выполнить следующим.")}</p>

      {!connected ? (
        <p role="status">{translate("Ядро недоступно — список шаблонов и состояние запуска не обновляются.")}</p>
      ) : null}

      <h4>{translate("Шаблоны")}</h4>
      {templates.length === 0 ? (
        <p role="status">{translate("Шаблоны ещё не получены от ядра.")}</p>
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
                {translate(item.display_name)}
              </button>
              <small>
                {translate("версия")}{translate(item.version)} {translate("· узлов")}{translate(item.node_count)} ·{translate(' ')}
                {translate(SCHEDULE_LABELS[item.schedule_eligibility] ?? item.schedule_eligibility)}
              </small>
            </li>
          ))}
        </ul>
      )}

      {template ? (
        <div className="workflow__template" aria-label={`Шаблон ${template.display_name}`}>
          <h4>{translate(template.display_name)}</h4>
          <p>{template.description}</p>
          <ul className="workflow__preview">
            {template.preview.map((line) => (
              <li key={line}>{translate(line)}</li>
            ))}
          </ul>
          <p>
            <small>{translate("требуются возможности:")}{translate(template.required_capabilities.join(', '))}</small>
          </p>
          {template.inputs.map((input) => (
            <label key={input.name} className="workflow__input">
              <span>
                {input.title}
                {translate(input.required ? ' *' : '')}
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
            {translate("Запустить")}</button>
        </div>
      ) : null}

      <h4>{translate("Пресеты запусков")}</h4>
      <p>
        {translate("Пресет сохраняет только проверенные значения и ссылки на credentials. Версия workflow и revision остаются зафиксированы ядром.")}</p>
      {presetResult?.presets?.length ? (
        <ul className="workflow__presets">
          {presetResult.presets.map((preset) => (
            <li key={`${preset.id}:${preset.revision}`}>
              <strong>{translate(preset.id)}</strong> · revision {translate(preset.revision)} · {translate(preset.state)}
              <small> · {translate(preset.content_hash)}</small>
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
                {translate("Запустить")}</button>
            </li>
          ))}
        </ul>
      ) : (
        <p role="status">{translate("Сохранённых пресетов нет.")}</p>
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
          {translate("Сохранить текущие входы как пресет")}</button>
      ) : null}

      {notice ? (
        <p className="listening__error" role="alert">
          {translate(notice)}
        </p>
      ) : null}

      {runId ? (
        <div className="workflow__run" aria-label={translate("Текущий запуск")}>
          <h4>{translate("Запуск")}{translate(runId)}</h4>
          <p role="status">
            {translate("состояние:")}{translate(runStateLabel(activeRun?.state ?? 'unknown_state'))}
            {translate(activeRun?.terminal_reason ? ` · ${activeRun.terminal_reason}` : '')}
          </p>
          {waitingNodes.length > 0 ? (
            <p role="status">
              {translate("Узел ждёт подтверждения: реши карточку подтверждения — отдельной кнопки у workflow нет.")}</p>
          ) : null}
          <ol className="workflow__nodes">
            {(activeRun?.nodes ?? []).map((node) => (
              <li key={node.node_id}>
                <strong>{translate(node.node_id)}</strong>
                <span>
                  {translate(' ')}
                  · {translate(node.action_kind)}
                  {translate(node.role ? ` (${node.role})` : '')} · {translate(nodeStateLabel(node.state))}
                  {translate(node.attempts > 0 ? ` · попыток: ${node.attempts}` : '')}
                </span>
                {node.dependencies.length > 0 ? (
                  <small> {translate("зависит от:")}{translate(node.dependencies.join(', '))}</small>
                ) : null}
                {node.error_code ? <small> {translate("код:")}{translate(node.error_code)}</small> : null}
              </li>
            ))}
          </ol>
          <button type="button" disabled={!api || !connected} onClick={() => void cancel()}>
            {translate("Отменить запуск")}</button>
          <h4>{translate("События")}</h4>
          <ul className="workflow__events">
            {workflowEvents.map((event) => (
              <li key={event.sequence}>
                #{translate(event.sequence)} {translate(event.event_type)}
                {translate(event.node_id ? ` · ${event.node_id}` : '')}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </section>
  )
}
