import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

type Projection = { status?: string; error_code?: string; projection_json?: { profile_count?: number; profile_id?: string; revision?: number; content_hash?: string; execution_mode?: string; raw_prompt?: boolean; credentials?: boolean } }

/** Metadata-only projection; Core remains the sole profile and authority owner. */
export function AgentRoleProfilesPanel(): React.JSX.Element {
  const api = useShellApi(); const [projection, setProjection] = useState<Projection | null>(null)
  useEffect(() => {
    if (!api) return
    const unsubscribe = api.subscribe((event) => { if (event.kind !== 'core-event' || event.event.eventType !== 'agent_role_profiles.result') return; try { setProjection(JSON.parse(event.event.payload) as Projection) } catch { setProjection({ status: 'invalid_projection' }) } })
    void api.invoke('agentRoleProfiles.list', { requestId: crypto.randomUUID(), ownerScope: 'agent-role-profiles', idempotencyKey: crypto.randomUUID() })
    return unsubscribe
  }, [api])
  const meta = projection?.projection_json
  return <section className="panel" aria-label="Agent Role Profiles"><h2>Профили ролей агентов</h2><p role="status">Профилей: {meta?.profile_count ?? 0} · состояние: {projection?.status ?? 'ожидание Core'}</p><p>Версия контракта: v1 · profile revision/hash фиксируются на run.</p><p>Режимы human/AI проверяются Core; requested grants не расширяют parent/policy/registry intersection.</p><p>Raw prompts: запрещены · credentials: не передаются.</p>{projection && projection.status !== 'ok' && <p>Ошибка: {projection.error_code || projection.status}</p>}</section>
}
