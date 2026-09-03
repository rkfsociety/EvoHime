import { useShellApi } from './shell-api'
import type { ConnectionState, PolicyAwareToolResultCacheProjection, ShellEvent } from '@shared/api'
import { useEffect, useState } from 'react'

export function PolicyAwareToolResultCachePanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi(); const [projection, setProjection] = useState<PolicyAwareToolResultCacheProjection | null>(null); const [cacheKey, setCacheKey] = useState(''); const [message, setMessage] = useState('')
  useEffect(() => api?.subscribe((event: ShellEvent) => { if (event.kind === 'core-event' && event.event.policyAwareToolResultCache) setProjection(event.event.policyAwareToolResultCache) }), [api])
  const inspect = async (): Promise<void> => { if (!api || connection !== 'connected' || !cacheKey.trim()) { setMessage('Нужны подключение и cache key.'); return }; const result = await api.invoke('core.policyAwareToolResultCache', { operation: 'inspect', cacheKey: cacheKey.trim(), payload: '', idempotencyKey: crypto.randomUUID() }); setMessage(result.ok ? 'Запрос принят Core.' : result.message) }
  return <section aria-label="Policy-Aware Tool Result Cache"><h3>Policy-Aware Tool Result Cache</h3><p>По умолчанию Never; только Core решает, допустимы ли read-only reuse, TTL и provenance.</p><label>Cache key<input value={cacheKey} onChange={event => setCacheKey(event.target.value)} maxLength={128} /></label><button type="button" onClick={() => void inspect()}>Проверить в Core</button>{projection?.projection ? <pre>{JSON.stringify(projection.projection, null, 2)}</pre> : null}{message ? <p role="status">{message}</p> : null}</section>
}
