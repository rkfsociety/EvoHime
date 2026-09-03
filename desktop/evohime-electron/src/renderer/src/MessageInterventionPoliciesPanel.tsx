import { useEffect, useState } from 'react'
import type { ConnectionState, MessageInterventionPoliciesProjection, ShellEvent } from '@shared/api'
import { useShellApi } from './shell-api'

export function MessageInterventionPoliciesPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [payload, setPayload] = useState('')
  const [projection, setProjection] = useState<MessageInterventionPoliciesProjection | null>(null)
  const [message, setMessage] = useState('')
  useEffect(() => api?.subscribe((event: ShellEvent) => { if (event.kind === 'core-event' && event.event.messageInterventionPolicies) setProjection(event.event.messageInterventionPolicies) }), [api])
  const send = async (): Promise<void> => { if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }; const result = await api.invoke('core.messageInterventionPolicies', { operation: 'evaluate', payload }); setMessage(result.ok ? 'Evaluation передана Core.' : result.message) }
  const data = projection?.projection && typeof projection.projection === 'object' && projection.projection !== null ? projection.projection as Record<string, unknown> : null
  return <section aria-label="Message Intervention Policies"><h3>Message Intervention Policies</h3><p>Перехват выполняется Core до доставки; renderer показывает только bounded verdict и redaction metadata.</p><label>Policy и metadata context JSON<textarea aria-label="Message Intervention Policies JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={64 * 1024} /></label><button type="button" onClick={() => void send()}>Проверить доставку</button>{data ? <pre>{JSON.stringify(data, null, 2)}</pre> : null}{message ? <p role="status">{message}</p> : null}</section>
}
