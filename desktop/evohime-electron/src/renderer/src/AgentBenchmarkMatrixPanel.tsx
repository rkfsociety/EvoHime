import { useEffect, useState } from 'react'

import { useShellApi } from './shell-api'

export function AgentBenchmarkMatrixPanel(): React.JSX.Element {
  const api = useShellApi()
  const [status, setStatus] = useState('Загрузка benchmark matrix…')
  const [starting, setStarting] = useState(false)
  const [approval, setApproval] = useState({
    runId: '', challengeId: '', modelProfileId: '', agentProfileId: '', reportSha256: '', expectedVersion: ''
  })
  const [approvalIdempotencyKey, setApprovalIdempotencyKey] = useState('')

  useEffect(() => {
    if (!api) { setStatus('Core недоступен'); return }
    void api.invoke('benchmarkMatrix.list', {
      requestId: crypto.randomUUID(), ownerScope: 'benchmark'
    }).then((outcome) => {
      setStatus(outcome.ok && outcome.value.accepted ? 'Matrix доступна в Core' : 'Matrix недоступна')
    })
  }, [api])

  async function start(): Promise<void> {
    if (!api || starting) return
    setStarting(true)
    const outcome = await api.invoke('benchmarkMatrix.start', {
      requestId: crypto.randomUUID(), ownerScope: 'benchmark', suiteId: 'core',
      mode: 'deterministic', attempts: 3, idempotencyKey: crypto.randomUUID()
    })
    setStatus(outcome.ok && outcome.value.accepted ? 'Запуск отправлен в Core' : 'Запуск недоступен')
    setStarting(false)
  }

  async function approveBaseline(): Promise<void> {
    if (!api || starting) return
    const expectedVersion = Number(approval.expectedVersion)
    if (!Number.isSafeInteger(expectedVersion) || expectedVersion <= 0) {
      setStatus('Для approval укажите текущую revision завершённой adaptation job.')
      return
    }
    const idempotencyKey = approvalIdempotencyKey || crypto.randomUUID()
    setApprovalIdempotencyKey(idempotencyKey)
    setStarting(true)
    try {
      const outcome = await api.invoke('benchmarkMatrix.approveBaseline', {
        requestId: crypto.randomUUID(), ownerScope: 'benchmark',
        runId: approval.runId, challengeId: approval.challengeId,
        modelProfileId: approval.modelProfileId, agentProfileId: approval.agentProfileId,
        reportSha256: approval.reportSha256, expectedVersion,
        idempotencyKey
      })
      setStatus(outcome.ok && outcome.value.accepted
        ? 'Запрос явного approval отправлен Core; baseline появится только после проверки отчёта и revision.'
        : 'Core отклонил запрос approval.')
    } catch {
      setStatus('Не удалось отправить запрос approval; повторите его с тем же содержимым.')
    } finally {
      setStarting(false)
    }
  }

  return <section className="settings-info" aria-label="Agent Benchmark Matrix">
    <h3>Agent Benchmark Matrix</h3>
    <p>{status}. Отображаются только bounded redacted metadata; verdict и baseline принадлежат Core.</p>
    <button type="button" onClick={() => void start()} disabled={!api || starting}>
      {starting ? 'Запуск…' : 'Запустить deterministic matrix'}
    </button>
    <fieldset>
      <legend>Явно утвердить baseline завершённого real run</legend>
      {([
        ['runId', 'ID adaptation job / run'], ['challengeId', 'Challenge ID'],
        ['modelProfileId', 'Model profile ID'], ['agentProfileId', 'Agent profile ID'],
        ['reportSha256', 'SHA-256 отчёта']
      ] as const).map(([key, label]) => <label key={key}>
        {label}
        <input value={approval[key]} onChange={event => {
          setApproval(current => ({ ...current, [key]: event.target.value }))
          setApprovalIdempotencyKey('')
        }} />
      </label>)}
      <label>
        Revision завершённой adaptation job
        <input inputMode="numeric" value={approval.expectedVersion} onChange={event => {
          setApproval(current => ({ ...current, expectedVersion: event.target.value }))
          setApprovalIdempotencyKey('')
        }} />
      </label>
      <button type="button" onClick={() => void approveBaseline()} disabled={!api || starting
        || !approval.runId || !approval.challengeId || !approval.modelProfileId
        || !approval.agentProfileId || !/^[\da-f]{64}$/i.test(approval.reportSha256)}>
        Явно утвердить baseline
      </button>
    </fieldset>
  </section>
}
