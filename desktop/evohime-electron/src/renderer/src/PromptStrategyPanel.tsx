import { useMemo, useState } from 'react'

import { useShellApi } from './shell-api'
import type { CoreEvent } from '@shared/api'

interface PromptStrategyPanelProps {
  readonly events: readonly CoreEvent[]
}

export function PromptStrategyPanel({ events }: PromptStrategyPanelProps): React.JSX.Element {
  const api = useShellApi()
  const [profileJson, setProfileJson] = useState('')
  const [profileId, setProfileId] = useState('')
  const [revision, setRevision] = useState('1')
  const [expectedVersion, setExpectedVersion] = useState('1')
  const [expectedState, setExpectedState] = useState('draft')
  const [nextState, setNextState] = useState('validated')
  const [advancedOperation, setAdvancedOperation] = useState<'strategyBind' | 'strategyExampleSet' | 'strategyOutputContract' | 'strategyPromote' | 'strategySelections' | 'strategyEvidence' | 'strategyCompatibility' | 'strategyCompare'>('strategyBind')
  const [advancedPayload, setAdvancedPayload] = useState('{}')
  const [status, setStatus] = useState('')

  const strategyEvents = useMemo(() => events.filter((event) =>
    event.eventType === 'benchmark_matrix.result' && event.payload.includes('strategy')
  ).slice(-8), [events])

  function send(operation: 'strategyList' | 'strategyGet' | 'strategyRegister' | 'strategyBind' | 'strategyExampleSet' | 'strategyOutputContract' | 'strategyTransition' | 'strategyPromote' | 'strategySelections' | 'strategyEvidence' | 'strategyCompatibility' | 'strategyCompare', payload: Record<string, unknown>, expected = 0): void {
    if (!api) { setStatus('Core недоступен'); return }
    if (!Number.isSafeInteger(expected) || expected < 0) { setStatus('Некорректная lifecycle revision'); return }
    const payloadJson = JSON.stringify(payload)
    if (new TextEncoder().encode(payloadJson).length > 64 * 1024) { setStatus('Payload превышает 64 KiB'); return }
    void api.invoke(`benchmarkMatrix.${operation}`, {
      requestId: crypto.randomUUID(), ownerScope: 'prompt_strategy', payload: payloadJson,
      expectedVersion: expected, idempotencyKey: crypto.randomUUID()
    }).then((outcome) => setStatus(outcome.ok && outcome.value.accepted
      ? `Команда ${operation} принята Core; результат появится в event stream.`
      : `Команда ${operation} отклонена оболочкой.`))
  }

  const parsedRevision = Number(revision)
  const parsedExpectedVersion = Number(expectedVersion)
  return <section className="settings-info prompt-strategy" aria-label="Prompt Strategy Resolver">
    <h3>Prompt Strategy Resolver</h3>
    <p>Core хранит только bounded strategy metadata и evidence references. Renderer не получает prompt text или reusable example contents.</p>
    <div className="settings-info__actions prompt-strategy__actions">
      <button type="button" disabled={!api} onClick={() => send('strategyList', {})}>Обновить registry</button>
      <button type="button" disabled={!api || !profileId || !Number.isSafeInteger(parsedRevision) || parsedRevision <= 0}
        onClick={() => send('strategyGet', { profile_id: profileId, revision: parsedRevision })}>Получить профиль</button>
    </div>
    <label>Profile ID<input value={profileId} maxLength={128} onChange={(event) => setProfileId(event.target.value)} /></label>
    <label>Profile revision<input inputMode="numeric" value={revision} onChange={(event) => setRevision(event.target.value)} /></label>
    <label>Новый immutable profile JSON<textarea value={profileJson} maxLength={64 * 1024} onChange={(event) => setProfileJson(event.target.value)} /></label>
    <button type="button" disabled={!api || !profileJson.trim()} onClick={() => {
      try { send('strategyRegister', { profile: JSON.parse(profileJson) }) }
      catch { setStatus('Profile JSON некорректен') }
    }}>Зарегистрировать draft</button>
    <fieldset>
      <legend>Lifecycle transition через Core CAS</legend>
      <label>Ожидаемое состояние<input value={expectedState} maxLength={16} onChange={(event) => setExpectedState(event.target.value)} /></label>
      <label>Новое состояние<input value={nextState} maxLength={16} onChange={(event) => setNextState(event.target.value)} /></label>
      <label>Ожидаемая state revision<input inputMode="numeric" value={expectedVersion} onChange={(event) => setExpectedVersion(event.target.value)} /></label>
      <button type="button" disabled={!api || !profileId || !Number.isSafeInteger(parsedRevision) || parsedRevision <= 0
        || !Number.isSafeInteger(parsedExpectedVersion) || parsedExpectedVersion <= 0}
        onClick={() => send('strategyTransition', {
          profile_id: profileId, revision: parsedRevision,
          expected_state: expectedState, next_state: nextState
        }, parsedExpectedVersion)}>Применить transition</button>
    </fieldset>
    <fieldset>
      <legend>Связи, evidence и replay</legend>
      <label>Операция<select value={advancedOperation} onChange={(event) => setAdvancedOperation(event.target.value as typeof advancedOperation)}>
        <option value="strategyBind">Создать immutable binding</option>
        <option value="strategyExampleSet">Добавить example-set references</option>
        <option value="strategyOutputContract">Зарегистрировать structured-output contract</option>
        <option value="strategyPromote">Явно продвинуть по holdout evidence</option>
        <option value="strategyEvidence">Проверить evidence и gate promotion</option>
        <option value="strategyCompatibility">Историческая route compatibility</option>
        <option value="strategyCompare">Сравнить с benchmark baseline</option>
        <option value="strategySelections">Посмотреть selections по provenance request</option>
      </select></label>
      <label>Bounded operation payload JSON<textarea value={advancedPayload} maxLength={64 * 1024} onChange={(event) => setAdvancedPayload(event.target.value)} /></label>
      <button type="button" disabled={!api || !Number.isSafeInteger(parsedExpectedVersion) || parsedExpectedVersion < 0}
        onClick={() => {
          try { send(advancedOperation, JSON.parse(advancedPayload), parsedExpectedVersion) }
          catch { setStatus('Operation payload JSON некорректен') }
        }}>Отправить Core operation</button>
    </fieldset>
    {status ? <p role="status">{status}</p> : null}
    <details><summary>Последние bounded Core responses</summary>
      {strategyEvents.length ? <pre>{strategyEvents.map((event) => event.payload).join('\n')}</pre> : <p>Ответов ещё нет.</p>}
    </details>
  </section>
}
