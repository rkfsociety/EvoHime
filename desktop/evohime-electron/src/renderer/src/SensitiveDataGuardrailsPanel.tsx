import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

/** Metadata-only projection of the Core guardrail policy. */
export function SensitiveDataGuardrailsPanel(): React.JSX.Element {
  const api = useShellApi()
  const [status, setStatus] = useState('Загрузка политики защиты данных…')

  useEffect(() => {
    if (!api) { setStatus('Core недоступен'); return }
    void api.invoke('sensitiveDataGuardrails.status', {
      requestId: crypto.randomUUID(), ownerScope: 'sensitive-data-guardrails', idempotencyKey: crypto.randomUUID()
    }).then((outcome) => setStatus(outcome.ok ? 'Guardrails активны' : 'Не удалось получить состояние'))
  }, [api])

  return <section className="panel" aria-label="Sensitive Data Guardrails">
    <h2>Защита чувствительных данных</h2>
    <p>{status}</p>
    <p>Границы: model · tool · stream · trace</p>
    <p>Действия: redact · mask · hash · block</p>
    <p>Сырые prompt/output, credentials и тела правил не передаются в интерфейс.</p>
  </section>
}
