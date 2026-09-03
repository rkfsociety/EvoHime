import { useEffect, useState } from 'react'
import type { ConnectionState, PlanArtifactProjection } from '@shared/api'
import { useShellApi } from './shell-api'

export function PlanArtifactPanel({ connection, events }: { readonly connection: ConnectionState; readonly events: readonly { readonly eventType: string; readonly payload: string }[] }): React.JSX.Element {
  const api = useShellApi()
  const [id, setId] = useState('')
  const [projection, setProjection] = useState<PlanArtifactProjection | null>(null)
  const [message, setMessage] = useState('')
  useEffect(() => { const event = events.find((item) => item.eventType === 'plan_artifact.result'); if (!event) return; try { const value = JSON.parse(event.payload) as PlanArtifactProjection; if (value && typeof value.id === 'string') setProjection(value) } catch { setMessage('Core вернул некорректную bounded projection.') } }, [events])
  const read = async (): Promise<void> => { if (!api || !id.trim() || connection !== 'connected') { setMessage('Нужны подключение к Core и идентификатор.'); return }; await api.invoke('core.planArtifactRead', { artifactId: id.trim() }) }
  const action = async (operation: 'transition' | 'execute', status = ''): Promise<void> => { if (!api || !projection) return; await api.invoke('core.planArtifactAction', { operation, artifactId: projection.id, expectedVersion: projection.version, status, policySnapshotHash: 'ui-policy-request', correlationId: crypto.randomUUID(), idempotencyKey: crypto.randomUUID() }) }
  return <section aria-label="Plan Artifact" className="plan-artifact-panel">
    <h3>План выполнения (Plan Artifact)</h3>
    <p className="plan-artifact-panel__intro">Здесь хранится утверждённый план работы агента: последовательность шагов, риски и условия, по которым можно считать задачу выполненной.</p>
    <div className="plan-artifact-panel__guide">
      <h4>Для чего это нужно</h4>
      <p>Plan Artifact сохраняет план в Core с версией и контрольной суммой. Поэтому во время выполнения нельзя незаметно подменить шаги или выдать незавершённую работу за готовую.</p>
      <h4>Как пользоваться</h4>
      <ol>
        <li>Введите идентификатор сохранённого плана и нажмите «Прочитать».</li>
        <li>Проверьте шаги, риски и критерии готовности.</li>
        <li>Для черновика нажмите «Принять план», затем для принятого плана — «Запустить план».</li>
      </ol>
      <p className="plan-artifact-panel__hint">Сейчас планы создаются другими сценариями Core, поэтому эта вкладка умеет только прочитать, принять и запустить уже существующий план.</p>
    </div>
    <div className="plan-artifact-panel__lookup">
      <label htmlFor="plan-artifact-id">Идентификатор плана</label>
      <div className="plan-artifact-panel__lookup-row">
        <input id="plan-artifact-id" value={id} onChange={(event) => setId(event.target.value)} maxLength={256} placeholder="Например, plan-…" />
        <button type="button" onClick={() => void read()}>Прочитать план</button>
      </div>
    </div>
    {projection ? <>
      <p><strong>Состояние: {projection.status}</strong> · revision {projection.revision} · version {projection.version}</p>
      <h4>Шаги плана</h4>
      <ul>{projection.steps.map((step) => <li key={step.id}>{step.description} · риск: {step.risk}</li>)}</ul>
      <h4>Критерии готовности</h4>
      <ul>{projection.acceptanceCriteria.map((criterion) => <li key={criterion.id}>{criterion.description} · проверка: {criterion.evidenceKind}</li>)}</ul>
      {projection.status === 'draft' ? <button type="button" onClick={() => void action('transition', 'accepted')}>Принять план</button> : null}
      {projection.status === 'accepted' ? <button type="button" onClick={() => void action('execute')}>Запустить план</button> : null}
    </> : null}
    {message ? <p role="status">{message}</p> : null}
  </section>
}
