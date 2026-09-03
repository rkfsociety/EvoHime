import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
import type { ConnectionState, ModelPurposeRoutingProjection, ShellEvent } from '@shared/api'

export function ModelPurposeRoutingPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi(); const [projection, setProjection] = useState<ModelPurposeRoutingProjection | null>(null); const [policy, setPolicy] = useState(''); const [message, setMessage] = useState('')
  useEffect(() => api?.subscribe((event: ShellEvent) => { if (event.kind === 'core-event' && event.event.modelPurposeRouting) setProjection(event.event.modelPurposeRouting) }), [api])
  const request = async (operation: 'get' | 'put') => { if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }; const result = await api.invoke('core.modelPurposeRouting', { operation, payload: policy, expectedVersion: projection?.version ?? 0, idempotencyKey: crypto.randomUUID() }); setMessage(result.ok ? 'Запрос принят Core.' : result.message) }
  return <section aria-label="Model Purpose Routing"><h3>Model Purpose Routing</h3><p>Core выбирает зарегистрированный profile ref по purpose; retry/fallback остаются в Model Resilience Policy.</p><textarea aria-label="Policy JSON" value={policy} onChange={event => setPolicy(event.target.value)} maxLength={256 * 1024} /><div><button type="button" onClick={() => void request('get')}>Загрузить policy</button><button type="button" onClick={() => void request('put')}>Сохранить policy</button></div>{projection?.projection ? <pre>{JSON.stringify(projection.projection, null, 2)}</pre> : null}{message ? <p role="status">{message}</p> : null}</section>
}
