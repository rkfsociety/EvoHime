import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

export function AgentMiddlewarePipelinePanel(): React.JSX.Element {
  const api = useShellApi()
  const [status, setStatus] = useState('Загрузка middleware pipeline…')
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (!api) { setStatus('Core недоступен'); return }
    void api.invoke('agentMiddleware.list', { requestId: crypto.randomUUID(), ownerScope: 'middleware' }).then((outcome) => {
      setStatus(outcome.ok && outcome.value.accepted ? 'Pipeline доступен в Core' : 'Pipeline недоступен')
    })
  }, [api])
  async function start(): Promise<void> {
    if (!api || busy) return
    setBusy(true)
    const outcome = await api.invoke('agentMiddleware.start', { requestId: crypto.randomUUID(), ownerScope: 'middleware', runId: crypto.randomUUID(), idempotencyKey: crypto.randomUUID() })
    setStatus(outcome.ok && outcome.value.accepted ? 'Запуск отправлен в Core' : 'Запуск недоступен')
    setBusy(false)
  }
  return <section className="settings-info" aria-label="Agent Middleware Pipeline">
    <h3>Agent Middleware Pipeline</h3>
    <p>{status}. Core возвращает только bounded metadata; authority, ordering и policy принадлежат Core.</p>
    <button type="button" onClick={() => void start()} disabled={!api || busy}>{busy ? 'Запуск…' : 'Проверить pipeline'}</button>
  </section>
}
