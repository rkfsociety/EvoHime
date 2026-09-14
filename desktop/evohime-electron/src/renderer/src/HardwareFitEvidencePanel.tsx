import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

type Projection = { sample_count?: number; observation_count?: number; confidence?: string; runtime_family?: string; error_code?: string }

/** Core-derived hardware-fit metadata only; no raw machine identity crosses the UI boundary. */
export function HardwareFitEvidencePanel(): React.JSX.Element {
  const api = useShellApi()
  const [projection, setProjection] = useState<Projection | null>(null)
  useEffect(() => {
    if (!api) return
    const off = api.subscribe((event) => {
      if (event.kind !== 'core-event' || event.event.eventType !== 'hardware_fit_evidence.result') return
      try { setProjection(JSON.parse(event.event.payload).projection as Projection) }
      catch { setProjection({ error_code: 'invalid_projection' }) }
    })
    void api.invoke('hardwareFitEvidence.list', { requestId: crypto.randomUUID(), ownerScope: 'local', idempotencyKey: crypto.randomUUID() })
    return off
  }, [api])
  return <section className="panel" aria-label="Hardware Fit Evidence">
    <h2>Hardware Fit Evidence</h2>
    <p role="status">Измерений: {projection?.sample_count ?? projection?.observation_count ?? 0} · confidence: {projection?.confidence ?? 'unknown'}</p>
    <p>Runtime: {projection?.runtime_family ?? 'нет comparable evidence'} · Core contract v1 · IPC 276/121.</p>
    <p>Показываются только Core-derived metadata; raw machine identifiers, prompts и credentials не передаются.</p>
    {projection?.error_code ? <p role="alert">Ошибка: {projection.error_code}</p> : null}
  </section>
}
