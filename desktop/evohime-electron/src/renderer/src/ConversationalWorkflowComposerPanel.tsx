import { translate } from './i18n'
import { useState } from 'react'
import type { ConnectionState, CoreEvent } from '@shared/api'
import { useShellApi } from './shell-api'

interface Props { readonly connection: ConnectionState; readonly events: readonly CoreEvent[]; readonly workspace: string | null }

export function ConversationalWorkflowComposerPanel({ connection, events, workspace }: Props): React.JSX.Element {
  const api = useShellApi()
  const [request, setRequest] = useState('')
  const last = events.find((event) => event.eventType === 'workflow_composer.result')
  async function generate(): Promise<void> {
    if (!api || !workspace || !request.trim()) return
    await api.invoke('workflowComposer.command', {
      requestId: crypto.randomUUID(), ownerScope: workspace, draftId: 'composer-draft', operation: 'generate',
      payload: request, expectedRevision: 0, idempotencyKey: crypto.randomUUID()
    })
  }
  return <section className="settings-info workflow-composer" aria-label="Conversational Workflow Composer">
    <h3>Composer workflow</h3>
    <p>{translate("Модель предлагает draft, а Core отдельно проверяет права, binding и риск. Запуск выполняется только явным действием.")}</p>
    <textarea aria-label={translate("Описание workflow")} value={request} onChange={(event) => setRequest(event.target.value)} rows={4} placeholder={translate("Опиши желаемый workflow обычным языком")} />
    <button type="button" disabled={!api || !workspace || !['connected', 'replaying', 'resyncing'].includes(connection) || !request.trim()} onClick={() => void generate()}>{translate("Создать draft")}</button>
    {last ? <pre aria-label={translate("Результат Composer")}>{translate(last.payload)}</pre> : <p>{translate("Результат и blockers появятся после ответа Core.")}</p>}
  </section>
}
