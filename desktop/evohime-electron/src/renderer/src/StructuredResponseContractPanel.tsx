import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

export function StructuredResponseContractPanel(): React.JSX.Element {
  const api = useShellApi()
  const [status, setStatus] = useState('Загрузка structured response…')
  useEffect(() => {
    if (!api) { setStatus('Core недоступен'); return }
    void api.invoke('structuredResponse.list', { requestId: crypto.randomUUID(), ownerScope: 'structured-response', idempotencyKey: crypto.randomUUID() }).then((outcome) => {
      setStatus(outcome.ok && outcome.value.accepted ? 'Контракт доступен в Core' : 'Контракт недоступен')
    })
  }, [api])
  return <section className="settings-info" aria-label="Structured Response Contract">
    <h3>Structured Response Contract</h3>
    <p>{status}. Core валидирует schema, strategy и provenance; raw output не попадает в UI.</p>
  </section>
}
