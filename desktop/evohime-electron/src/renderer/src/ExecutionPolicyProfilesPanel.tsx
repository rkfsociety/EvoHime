import { translate } from './i18n'
import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

type Projection = { status?: string; profile_id?: string; version?: number; profile_hash?: string; backend?: string; network_policy?: string; environment_policy?: string; timeout_ms?: number; max_output_bytes?: number; error_code?: string }

/** Metadata-only projection of the Core-resolved process execution profile. */
export function ExecutionPolicyProfilesPanel(): React.JSX.Element {
  const api = useShellApi()
  const [projection, setProjection] = useState<Projection | null>(null)
  useEffect(() => {
    if (!api) return
    const unsubscribe = api.subscribe((event) => {
      if (event.kind !== 'core-event' || event.event.eventType !== 'execution_policy_profiles.result') return
      try { setProjection(JSON.parse(event.event.payload) as Projection) } catch { setProjection({ status: 'invalid_projection' }) }
    })
    void api.invoke('executionPolicyProfiles.status', {
      requestId: crypto.randomUUID(), ownerScope: 'execution-policy-profiles', idempotencyKey: crypto.randomUUID()
    })
    return unsubscribe
  }, [api])
  return <section className="panel" aria-label="Execution Policy Profiles">
    <h2>{translate("Профиль выполнения процессов")}</h2>
    {!projection && <p>{translate("Ожидание состояния Core…")}</p>}
    {projection && projection.status === 'ok' && <>
      <p>{translate("Профиль:")}{translate(projection.profile_id)} {translate("· версия")}{translate(projection.version)}</p>
      <p>Backend: {translate(projection.backend)} {translate("· сеть:")}{translate(projection.network_policy)}</p>
      <p>Environment: {translate(projection.environment_policy)} · timeout: {translate(projection.timeout_ms)} ms</p>
      <p>Output limit: {translate(projection.max_output_bytes)} bytes</p>
      <p>Hash: {translate(projection.profile_hash)}</p>
    </>}
    {projection && projection.status !== 'ok' && <p>{translate("Состояние:")}{translate(projection.status)} ({translate(projection.error_code || 'unknown')})</p>}
    <p>{translate("Команда и environment не выбирают профиль и не передаются в эту проекцию.")}</p>
  </section>
}
