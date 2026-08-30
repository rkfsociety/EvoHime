import { useEffect, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'
import { useShellApi } from './shell-api'

interface Props { readonly connection: ConnectionState; readonly events: readonly CoreEvent[]; readonly workspace: string | null }

export function VisualWorkflowBuilderPanel({ connection, events, workspace }: Props): React.JSX.Element {
  const api = useShellApi()
  const [draft, setDraft] = useState('')
  const [revision, setRevision] = useState(0)
  const [notice, setNotice] = useState<string | null>(null)
  const [runId, setRunId] = useState('')
  const last = events.find((event) => event.eventType === 'workflow_builder.result')
  const lastResult = last ? (() => { try { return JSON.parse(last.payload) as { handoff_handle?: string } } catch { return {} } })() : {}
  useEffect(() => {
    if (!last) return
    try {
      const result = JSON.parse(last.payload) as { revision?: unknown }
      if (typeof result.revision === 'number') setRevision(result.revision)
    } catch { /* Core event is rendered as-is below. */ }
  }, [last?.payload])
  const nodes = (() => {
    try {
      const parsed = JSON.parse(draft) as { graph?: { nodes?: Array<{ id?: string; node_type?: unknown }> } }
      return parsed.graph?.nodes?.filter((node): node is { id: string; node_type?: unknown } => typeof node.id === 'string') ?? []
    } catch { return [] }
  })()

  async function validate(): Promise<void> {
    if (!api || !workspace) { setNotice('Сначала выбери рабочую папку и подключи ядро.'); return }
    const outcome = await api.invoke('workflowBuilder.command', {
      requestId: crypto.randomUUID(), ownerScope: workspace, draftId: 'builder-draft', operation: 'validate',
      payload: draft, expectedRevision: 1, idempotencyKey: crypto.randomUUID()
    })
    if (!outcome.ok) setNotice(outcome.message)
  }

  async function command(operation: string, payload = ''): Promise<void> {
    if (!api || !workspace) { setNotice('Сначала выбери рабочую папку и подключи ядро.'); return }
    const outcome = await api.invoke('workflowBuilder.command', { requestId: crypto.randomUUID(), ownerScope: workspace, draftId: 'builder-draft', operation, payload, expectedRevision: revision, idempotencyKey: crypto.randomUUID() })
    if (!outcome.ok) setNotice(outcome.message)
  }

  return <section className="settings-info workflow-builder" aria-label="Визуальный конструктор workflow">
    <h3>Визуальный конструктор</h3>
    <p>Core проверяет typed workflow draft. Редактор не выполняет граф и не получает его полномочия.</p>
    <textarea aria-label="Workflow draft JSON" value={draft} onChange={(event) => setDraft(event.target.value)} rows={8} />
    <div className="workflow-builder__canvas" aria-label="Canvas typed workflow">
      {nodes.length === 0 ? <p>Добавь узлы в typed draft JSON — они появятся на canvas.</p> : nodes.map((node) => <article className="workflow-builder__node" key={node.id}><strong>{node.id}</strong><small>{typeof node.node_type === 'string' ? node.node_type : 'typed block'}</small></article>)}
    </div>
    <div>
      <button type="button" disabled={!api || !['connected', 'replaying', 'resyncing'].includes(connection)} onClick={() => void validate()}>Проверить draft</button>{' '}
      <button type="button" disabled={!api || !workspace} onClick={() => void command('save', draft)}>Сохранить draft</button>{' '}
      <button type="button" disabled={!api || !workspace} onClick={() => void command('issue_handoff')}>Передать Composer</button>{' '}
      <button type="button" disabled={!api || !workspace || !lastResult.handoff_handle} onClick={() => void command('publish', lastResult.handoff_handle ?? '')}>Опубликовать</button>{' '}
      <button type="button" disabled={!api || !workspace} onClick={() => void command('recover')}>Восстановить</button>
      {' '}<button type="button" disabled={!api || !workspace} onClick={() => void command('catalog')}>Каталог блоков</button>
      {' '}<button type="button" disabled={!api || !runId} onClick={() => void command('inspect', runId)}>Инспекция запуска</button>
    </div>
    <label>Run ID для inspection <input value={runId} onChange={(event) => setRunId(event.target.value)} /></label>
    {notice ? <p role="alert">{notice}</p> : null}
    {last ? <pre aria-label="Результат проверки">{last.payload}</pre> : null}
  </section>
}
