import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

type Projection = {
  session_id?: string; state?: string; revision?: number; profile_policy?: string
  network_policy?: string; control_owner?: string; error_code?: string
  cdp_endpoint?: boolean; credentials?: boolean; raw_payload?: boolean
}

/** Metadata-only projection. Browser authority remains in Core. */
export function AgenticBrowserSessionPanel({ onClose }: { readonly onClose: () => void }) {
  const api = useShellApi()
  const [projection, setProjection] = useState<Projection | null>(null)
  useEffect(() => api?.subscribe((event) => {
    if (event.kind !== 'core-event' || event.event.eventType !== 'agentic_browser_session.result') return
    try {
      const payload = JSON.parse(event.event.payload) as { projection_json?: Projection }
      setProjection(payload.projection_json ?? { error_code: 'invalid_projection' })
    } catch { setProjection({ error_code: 'invalid_projection' }) }
  }), [api])
  const create = () => api?.invoke('agenticBrowserSession.create', {
    requestId: crypto.randomUUID(), ownerScope: 'conversation', idempotencyKey: crypto.randomUUID()
  })
  return <section className="panel browser-session-panel" aria-label="Agentic Browser Session">
    <div className="panel__header">
      <div><h2>Браузерная сессия</h2><span>Дополнительная панель</span></div>
      <button type="button" onClick={onClose}>Скрыть</button>
    </div>
    <p role="status">{projection ? `${projection.state ?? 'unknown'} · rev ${projection.revision ?? 0}` : 'Ожидание состояния Core…'}</p>
    {projection?.session_id && <p>Сессия: {projection.session_id}</p>}
    {projection?.profile_policy && <p>Профиль: {projection.profile_policy} · сеть: {projection.network_policy}</p>}
    {projection?.error_code && <p role="alert">Ошибка: {projection.error_code}</p>}
    {!projection?.session_id && <button type="button" onClick={() => void create()}>Создать сессию</button>}
    <p>CDP: {projection?.cdp_endpoint ? 'запрещён к показу' : 'не передаётся'} · credentials: {projection?.credentials ? 'запрещены' : 'не передаются'}</p>
  </section>
}
