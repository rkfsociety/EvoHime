import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

type Projection = { status?: string; policy_id?: string; policy_hash?: string; attempts?: number; retries?: number; fallbacks?: number; terminal_outcome?: string; error_code?: string }

/** Metadata-only projection of Core-owned model retry/fallback policy. */
export function ModelResiliencePolicyPanel(): React.JSX.Element {
  const api = useShellApi()
  const [projection, setProjection] = useState<Projection | null>(null)
  useEffect(() => {
    if (!api) return
    const unsubscribe = api.subscribe((event) => {
      if (event.kind !== 'core-event' || event.event.eventType !== 'model_resilience_policy.result') return
      try { setProjection(JSON.parse(event.event.payload) as Projection) } catch { setProjection({ status: 'invalid_projection' }) }
    })
    void api.invoke('modelResiliencePolicy.status', {
      requestId: crypto.randomUUID(), ownerScope: 'model-resilience-policy', idempotencyKey: crypto.randomUUID()
    })
    return unsubscribe
  }, [api])
  return <section className="panel" aria-label="Model Resilience Policy">
    <h2>Надёжность модели</h2>
    {!projection && <p>Ожидание состояния Core…</p>}
    {projection?.status === 'ok' && <>
      <p>Политика: {projection.policy_id}</p>
      <p>Попытки: {projection.attempts} · retry: {projection.retries} · fallback: {projection.fallbacks}</p>
      <p>Терминальное правило: {projection.terminal_outcome}</p>
      <p>Hash: {projection.policy_hash}</p>
    </>}
    {projection && projection.status !== 'ok' && <p>Состояние: {projection.status} ({projection.error_code || 'unknown'})</p>}
    <p>Профили проверяются Core; prompt, output, credentials и provider payload в UI не передаются.</p>
  </section>
}
