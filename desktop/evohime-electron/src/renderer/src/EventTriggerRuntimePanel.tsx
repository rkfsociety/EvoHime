import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import type { ShellEvent } from '@shared/api'

import { useShellApi } from './shell-api'

type SourceKind = 'local_workspace_event' | 'system_event'
type TriggerState = 'draft' | 'paused' | 'active' | 'broken' | 'connecting' | 'revoked' | 'deleted'

interface TriggerDefinition {
  readonly contract_version: string
  readonly trigger_id: string
  readonly owner_scope: string
  readonly source_kind: SourceKind
  readonly event_kind: string
  readonly workspace_path: string
  readonly workflow: { readonly workflow_id: string; readonly workflow_version: number; readonly execution_hash: string }
  readonly mapping: Readonly<Record<string, string>>
  readonly state: TriggerState
  readonly content_hash: string
  readonly created_at_ms: number
}

interface StoredTrigger { readonly definition: TriggerDefinition; readonly version: number; readonly updated_at_ms: number }
interface WorkflowTemplate {
  readonly template_id: string
  readonly version: number
  readonly display_name: string
  readonly execution_hash: string
  readonly inputs: readonly { readonly name: string; readonly title: string; readonly required: boolean; readonly max_chars: number }[]
}
interface EventSummary {
  readonly event_id: string
  readonly outcome: string
  readonly accepted_at_ms: number
  readonly correlation_id: string
}
interface RuntimeResponse {
  readonly request_id: string
  readonly operation: string
  readonly status: string
  readonly error_code: string
  readonly triggers?: readonly StoredTrigger[]
  readonly trigger?: TriggerDefinition
  readonly version?: number
  readonly workflow_templates?: readonly WorkflowTemplate[]
  readonly sources?: Partial<Record<SourceKind | 'integration_webhook', string>>
  readonly events?: readonly EventSummary[]
}

const OWNER_SCOPE = 'settings'
const EVENT_KINDS: Readonly<Record<SourceKind, readonly { readonly value: string; readonly label: string }[]>> = {
  local_workspace_event: [{ value: 'file_changed', label: 'Файл изменён' }],
  system_event: [
    { value: 'task_completed', label: 'Задача завершена' },
    { value: 'task_failed', label: 'Задача завершилась с ошибкой' }
  ]
}

function describeError(code: string): string {
  switch (code) {
    case 'source_unavailable': return 'Этот источник пока не подключён к Core.'
    case 'stale_version': return 'Правило изменилось в другом месте. Обнови список и повтори действие.'
    case 'unknown_trigger': return 'Правило больше не существует.'
    case 'storage_error': return 'Core не смог сохранить или прочитать правила.'
    case 'trigger_id_conflict': return 'Этот ID правила уже занят в другой области.'
    case 'revision_limit': return 'Достигнут предел версий правила.'
    case 'invalid_definition': return 'Проверь источник, workflow и поля входных данных.'
    case 'workflow_binding_invalid': return 'Версия workflow устарела или mapping не покрывает обязательные входы.'
    case 'invalid_payload': return 'Core получил некорректные данные правила.'
    default: return 'Core не выполнил операцию с триггером.'
  }
}

export function EventTriggerRuntimePanel({ workspace }: { readonly workspace: string | null }): React.JSX.Element {
  const api = useShellApi()
  const pending = useRef(new Set<string>())
  const [triggers, setTriggers] = useState<readonly StoredTrigger[]>([])
  const [templates, setTemplates] = useState<readonly WorkflowTemplate[]>([])
  const [sources, setSources] = useState<RuntimeResponse['sources']>({})
  const [events, setEvents] = useState<readonly EventSummary[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [status, setStatus] = useState('Загрузка состояния Core…')
  const [busy, setBusy] = useState(false)
  const [sourceKind, setSourceKind] = useState<SourceKind>('local_workspace_event')
  const [eventKind, setEventKind] = useState('file_changed')
  const [workspacePath, setWorkspacePath] = useState(workspace ?? '')
  const [triggerId, setTriggerId] = useState('')
  const [templateId, setTemplateId] = useState('')
  const [mappingText, setMappingText] = useState('{}')

  const selectedTemplate = useMemo(() => templates.find((template) => template.template_id === templateId), [templates, templateId])

  const send = useCallback(async (operation: string, payload?: unknown, expectedVersion = 0) => {
    if (!api) { setStatus('Core недоступен.'); return }
    const requestId = crypto.randomUUID()
    pending.current.add(requestId)
    setBusy(true)
    try {
      const outcome = await api.invoke('eventTriggerRuntime.command', {
        requestId,
        ownerScope: OWNER_SCOPE,
        operation,
        ...(payload === undefined ? {} : { payload: JSON.stringify(payload) }),
        expectedVersion,
        idempotencyKey: requestId
      })
      if (outcome.ok && outcome.value.accepted) return
      pending.current.delete(requestId)
      setBusy(pending.current.size > 0)
      setStatus('Core не принял запрос. Проверь соединение и повтори попытку.')
    } catch {
      pending.current.delete(requestId)
      setBusy(pending.current.size > 0)
      setStatus('Не удалось отправить запрос в Core.')
    }
  }, [api])

  const refresh = useCallback(async () => {
    if (!api) { setStatus('Core недоступен.'); return }
    const requestId = crypto.randomUUID()
    pending.current.add(requestId)
    setBusy(true)
    try {
      const outcome = await api.invoke('eventTriggerRuntime.list', { requestId, ownerScope: OWNER_SCOPE })
      if (outcome.ok && outcome.value.accepted) return
      pending.current.delete(requestId)
      setBusy(pending.current.size > 0)
      setStatus('Core не смог прочитать список триггеров.')
    } catch {
      pending.current.delete(requestId)
      setBusy(pending.current.size > 0)
      setStatus('Не удалось прочитать состояние триггеров из Core.')
    }
  }, [api])

  useEffect(() => {
    if (!api) { setStatus('Core недоступен.'); return }
    const unsubscribe = api.subscribe((event: ShellEvent) => {
      if (event.kind !== 'core-event' || event.event.eventType !== 'event_trigger_runtime.result') return
      let response: RuntimeResponse
      try { response = JSON.parse(event.event.payload) as RuntimeResponse } catch {
        pending.current.clear()
        setBusy(false)
        setStatus('Core вернул некорректный ответ по триггерам.')
        return
      }
      if (!pending.current.delete(response.request_id)) return
      setBusy(pending.current.size > 0)
      if (response.status !== 'ok') { setStatus(describeError(response.error_code)); return }
      if (response.operation === 'list') {
        setTriggers(response.triggers ?? [])
        setTemplates(response.workflow_templates ?? [])
        setSources(response.sources ?? {})
        setStatus('Список правил обновлён.')
      } else if (response.operation === 'save') {
        setSelectedId(null)
        setTriggerId('')
        setMappingText('{}')
        setStatus('Черновик сохранён. Включи правило, когда проверишь рабочую область и mapping.')
        void refresh()
      } else if (response.operation === 'pause' || response.operation === 'delete') {
        setStatus(response.operation === 'delete' ? 'Правило удалено.' : 'Правило приостановлено.')
        void refresh()
      } else if (response.operation === 'resume') {
        setStatus('Правило включено. Core будет обрабатывать новые события журнала.')
        void refresh()
      } else if (response.operation === 'events') {
        setEvents(response.events ?? [])
      }
    })
    void refresh()
    return unsubscribe
  }, [api, refresh])

  const edit = (stored: StoredTrigger) => {
    const definition = stored.definition
    setSelectedId(definition.trigger_id)
    setTriggerId(definition.trigger_id)
    setSourceKind(definition.source_kind)
    setEventKind(definition.event_kind)
    setWorkspacePath(definition.workspace_path)
    setTemplateId(definition.workflow.workflow_id)
    setMappingText(JSON.stringify(definition.mapping, null, 2))
    setEvents([])
    void send('events', { trigger_id: definition.trigger_id, limit: 20 })
  }

  const save = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!selectedTemplate || !triggerId.trim()) { setStatus('Укажи ID правила и доступный workflow.'); return }
    let mapping: Record<string, string>
    try {
      const parsed: unknown = JSON.parse(mappingText)
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)
        || Object.values(parsed).some((value) => typeof value !== 'string')) throw new Error('mapping')
      mapping = parsed as Record<string, string>
    } catch {
      setStatus('Mapping должен быть JSON-объектом вида {"вход_workflow": "поле_события"}.')
      return
    }
    const current = triggers.find((item) => item.definition.trigger_id === selectedId)
    const definition: TriggerDefinition = {
      contract_version: 'event-trigger/v1',
      trigger_id: triggerId.trim(),
      owner_scope: OWNER_SCOPE,
      source_kind: sourceKind,
      event_kind: eventKind,
      workspace_path: workspacePath.trim(),
      workflow: { workflow_id: selectedTemplate.template_id, workflow_version: selectedTemplate.version, execution_hash: selectedTemplate.execution_hash },
      mapping,
      state: 'draft',
      content_hash: '',
      created_at_ms: current?.definition.created_at_ms ?? Date.now()
    }
    void send('save', definition, current?.version ?? 0)
  }

  const beginNew = () => {
    setSelectedId(null)
    setTriggerId(`trigger-${crypto.randomUUID().slice(0, 8)}`)
    setSourceKind('local_workspace_event')
    setEventKind('file_changed')
    setWorkspacePath(workspace ?? '')
    setTemplateId(templates[0]?.template_id ?? '')
    setMappingText('{}')
  }

  const selectedSourceState = sources?.[sourceKind] ?? 'unavailable'

  return <section className="settings-info event-trigger-runtime" aria-label="Триггеры событий">
    <h3>Триггеры событий</h3>
    <p>Правило связывает событие с версией workflow и ограниченным mapping его входных данных. Каждое правило хранится в Core.</p>
    <div className="settings-info__badge">{selectedSourceState === 'available' ? 'Выбранный источник подключён' : 'Выбранный источник недоступен'}</div>
    <p role="status">{status}</p>
    <p className="event-trigger-runtime__notice">Системные события задач Core и изменения файлов в Windows можно включить. Чтобы workflow не запускал сам себя по собственным изменениям, файловые события в рабочей области временно пропускаются, пока там выполняется запуск от триггера; пропуски отмечаются в истории. Webhook-провайдер не подключён.</p>

    <div className="event-trigger-runtime__toolbar">
      <h4>Правила <span>({triggers.length})</span></h4>
      <div><button type="button" onClick={() => void refresh()} disabled={busy}>Обновить</button><button type="button" onClick={beginNew} disabled={busy || templates.length === 0}>Новое правило</button></div>
    </div>
    {triggers.length === 0 ? <p>Правил пока нет. Для файлового события укажи рабочую область; системные события приходят из журнала Core.</p> : (
      <ul className="event-trigger-runtime__list">
        {triggers.map((stored) => <li key={stored.definition.trigger_id}>
          <div><strong>{stored.definition.trigger_id}</strong><span>{stored.definition.source_kind} · {stored.definition.event_kind}</span><span>{templates.find((item) => item.template_id === stored.definition.workflow.workflow_id)?.display_name ?? stored.definition.workflow.workflow_id}</span></div>
          <span>{stored.definition.state === 'draft' ? 'Черновик' : stored.definition.state === 'paused' ? 'Приостановлен' : stored.definition.state === 'active' ? 'Включён' : 'Недоступен'}</span>
          <button type="button" onClick={() => edit(stored)} disabled={busy}>Изменить</button>
          {stored.definition.state !== 'active' ? <button type="button" onClick={() => void send('resume', { trigger_id: stored.definition.trigger_id }, stored.version)} disabled={busy || sources?.[stored.definition.source_kind] !== 'available'}>Включить</button> : null}
          {stored.definition.state === 'active' ? <button type="button" onClick={() => void send('pause', { trigger_id: stored.definition.trigger_id }, stored.version)} disabled={busy}>Приостановить</button> : null}
          <button type="button" onClick={() => { if (window.confirm(`Удалить триггер «${stored.definition.trigger_id}»?`)) void send('delete', { trigger_id: stored.definition.trigger_id }, stored.version) }} disabled={busy}>Удалить</button>
        </li>)}
      </ul>
    )}

    {selectedId !== null || triggerId !== '' ? <form className="event-trigger-runtime__form" onSubmit={save}>
      <h4>{selectedId ? 'Изменить правило' : 'Новое правило'}</h4>
      <label>ID правила<input required maxLength={128} value={triggerId} onChange={(event) => setTriggerId(event.target.value)} /></label>
      <label>Источник<select value={sourceKind} onChange={(event) => { const next: SourceKind = event.target.value === 'system_event' ? 'system_event' : 'local_workspace_event'; setSourceKind(next); setEventKind(EVENT_KINDS[next][0]?.value ?? '') }}><option value="local_workspace_event">Событие рабочей области</option><option value="system_event">Системное событие</option></select></label>
      <label>Событие<select value={eventKind} onChange={(event) => setEventKind(event.target.value)}>{EVENT_KINDS[sourceKind].map((item) => <option key={item.value} value={item.value}>{item.label}</option>)}</select></label>
      <label>Рабочая область<input required maxLength={32768} value={workspacePath} onChange={(event) => setWorkspacePath(event.target.value)} placeholder="C:\\Projects\\my-app" /></label>
      <label>Workflow<select required value={templateId} onChange={(event) => setTemplateId(event.target.value)}><option value="">Выбери workflow</option>{templates.map((item) => <option key={item.template_id} value={item.template_id}>{item.display_name}</option>)}</select></label>
      {selectedTemplate ? <small>Входы workflow: {selectedTemplate.inputs.map((input) => `${input.name}${input.required ? ' *' : ''}`).join(', ')}. Отметь обязательные поля в mapping.</small> : null}
      <label>Mapping входов<textarea aria-label="Mapping входов workflow" value={mappingText} onChange={(event) => setMappingText(event.target.value)} maxLength={8192} placeholder={'{"scope": "path"}'} /></label>
      <div><button type="submit" disabled={busy}>Сохранить черновик</button><button type="button" onClick={() => { setSelectedId(null); setTriggerId('') }} disabled={busy}>Отмена</button></div>
      {selectedSourceState !== 'available' ? <small>Черновик можно подготовить сейчас; события не будут приниматься, пока Core не подключит этот источник.</small> : null}
    </form> : null}

    {events.length > 0 ? <div className="event-trigger-runtime__history"><h4>Последние срабатывания</h4><ul>{events.map((item) => <li key={item.event_id}>{new Date(item.accepted_at_ms).toLocaleString()} · {item.outcome} · {item.correlation_id}</li>)}</ul></div> : null}
  </section>
}
