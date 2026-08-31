import { useEffect, useState } from 'react'

import { useShellApi } from './shell-api'

export function AgentBenchmarkMatrixPanel(): React.JSX.Element {
  const api = useShellApi()
  const [status, setStatus] = useState('Загрузка benchmark matrix…')
  const [starting, setStarting] = useState(false)

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

  return <section className="settings-info" aria-label="Agent Benchmark Matrix">
    <h3>Agent Benchmark Matrix</h3>
    <p>{status}. Отображаются только bounded redacted metadata; verdict и baseline принадлежат Core.</p>
    <button type="button" onClick={() => void start()} disabled={!api || starting}>
      {starting ? 'Запуск…' : 'Запустить deterministic matrix'}
    </button>
  </section>
}
