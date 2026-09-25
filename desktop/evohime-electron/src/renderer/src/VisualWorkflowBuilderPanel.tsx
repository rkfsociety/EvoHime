import { useEffect, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'
import { useShellApi } from './shell-api'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly workspace: string | null
  readonly draftId: string
  readonly ownerScope: string | null
}

interface BuilderCommandResult {
  readonly draft_id?: string
  readonly status?: string
  readonly revision?: number
  readonly execution_hash?: string
  readonly layout_hash?: string
  readonly error_code?: string
  readonly draft_json?: string
  readonly handoff_handle?: string
}

export function VisualWorkflowBuilderPanel({ connection, events, workspace, draftId, ownerScope }: Props): React.JSX.Element {
  const api = useShellApi()
  const draftOwnerScope = ownerScope ?? workspace
  const [draft, setDraft] = useState('')
  const [revision, setRevision] = useState(0)
  const [notice, setNotice] = useState<string | null>(null)
  const [runId, setRunId] = useState('')
  const last = events.find((event) => event.eventType === 'workflow_builder.result')
  const lastResult: BuilderCommandResult = last ? (() => { try { return JSON.parse(last.payload) as BuilderCommandResult } catch { return {} } })() : {}
  const matchingResult: BuilderCommandResult = lastResult.draft_id === draftId ? lastResult : {}
  useEffect(() => {
    if (!last || lastResult.draft_id !== draftId) return
    if (typeof lastResult.revision === 'number') setRevision(lastResult.revision)
    if (typeof lastResult.draft_json === 'string') setDraft(lastResult.draft_json)
    if (lastResult.error_code) setNotice(`Core: ${lastResult.error_code}`)
    else if (lastResult.status === 'recovered') setNotice('Draft загружен из Core.')
  }, [last?.payload, draftId])

  useEffect(() => {
    setDraft('')
    setRevision(0)
    setNotice(null)
  }, [draftId, ownerScope])

  useEffect(() => {
    if (!ownerScope || !api || !draftOwnerScope || !['connected', 'replaying', 'resyncing'].includes(connection)) return
    void api.invoke('workflowBuilder.command', {
      requestId: crypto.randomUUID(),
      ownerScope: draftOwnerScope,
      draftId,
      operation: 'recover',
      payload: '',
      expectedRevision: 0,
      idempotencyKey: crypto.randomUUID()
    })
  }, [api, connection, draftId, draftOwnerScope, ownerScope])
  const nodes = (() => {
    try {
      const parsed = JSON.parse(draft) as { graph?: { nodes?: Array<{ id?: string; node_type?: unknown }> } }
      return parsed.graph?.nodes?.filter((node): node is { id: string; node_type?: unknown } => typeof node.id === 'string') ?? []
    } catch { return [] }
  })()

  async function validate(): Promise<void> {
    if (!api || !draftOwnerScope) { setNotice('Сначала выбери рабочую папку и подключи ядро.'); return }
    const outcome = await api.invoke('workflowBuilder.command', {
      requestId: crypto.randomUUID(), ownerScope: draftOwnerScope, draftId, operation: 'validate',
      payload: draft, expectedRevision: revision, idempotencyKey: crypto.randomUUID()
    })
    if (!outcome.ok) setNotice(outcome.message)
  }

  async function command(operation: string, payload = ''): Promise<void> {
    if (!api || !draftOwnerScope) { setNotice('Сначала выбери рабочую папку и подключи ядро.'); return }
    const outcome = await api.invoke('workflowBuilder.command', { requestId: crypto.randomUUID(), ownerScope: draftOwnerScope, draftId, operation, payload, expectedRevision: revision, idempotencyKey: crypto.randomUUID() })
    if (!outcome.ok) setNotice(outcome.message)
  }

  return <section className="settings-info workflow-builder" aria-label="Визуальный конструктор workflow">
    <h3>Визуальный конструктор</h3>
    <p>Core проверяет typed workflow draft. Редактор не выполняет граф и не получает его полномочия.</p>
    <p>Draft: <code>{draftId}</code> · revision {revision}</p>
    <textarea aria-label="Workflow draft JSON" value={draft} onChange={(event) => setDraft(event.target.value)} rows={8} />
    <div className="workflow-builder__canvas" aria-label="Canvas typed workflow">
      {nodes.length === 0 ? <p>Добавь узлы в typed draft JSON — они появятся на canvas.</p> : nodes.map((node) => <article className="workflow-builder__node" key={node.id}><strong>{node.id}</strong><small>{typeof node.node_type === 'string' ? node.node_type : 'typed block'}</small></article>)}
    </div>
    <div>
      <button type="button" disabled={!api || !['connected', 'replaying', 'resyncing'].includes(connection)} onClick={() => void validate()}>Проверить draft</button>{' '}
      <button type="button" disabled={!api || !draftOwnerScope} onClick={() => void command('save', draft)}>Сохранить draft</button>{' '}
      <button type="button" disabled={!api || !draftOwnerScope} onClick={() => void command('issue_handoff')}>Передать Composer</button>{' '}
      <button type="button" disabled={!api || !draftOwnerScope || !matchingResult.handoff_handle} onClick={() => void command('publish', matchingResult.handoff_handle ?? '')}>Опубликовать</button>{' '}
      <button type="button" disabled={!api || !draftOwnerScope} onClick={() => void command('recover')}>Восстановить</button>
      {' '}<button type="button" disabled={!api || !draftOwnerScope} onClick={() => void command('catalog')}>Каталог блоков</button>
      {' '}<button type="button" disabled={!api || !runId} onClick={() => void command('inspect', runId)}>Инспекция запуска</button>
    </div>
    <label>Run ID для inspection <input value={runId} onChange={(event) => setRunId(event.target.value)} /></label>
    {notice ? <p role="alert">{notice}</p> : null}
    {last && matchingResult.status ? <p role="status">Core: {matchingResult.status}{matchingResult.error_code ? ` · ${matchingResult.error_code}` : ''}{matchingResult.execution_hash ? ` · graph ${matchingResult.execution_hash}` : ''}{matchingResult.layout_hash ? ` · layout ${matchingResult.layout_hash}` : ''}</p> : null}
  </section>
}
