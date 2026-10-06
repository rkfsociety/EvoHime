import { translate } from './i18n'
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
      <div><h2>{translate("Браузерная сессия")}</h2><span>{translate("Дополнительная панель")}</span></div>
      <button type="button" onClick={onClose}>{translate("Скрыть")}</button>
    </div>
    <p role="status">{translate(projection ? `${projection.state ?? 'unknown'} · rev ${projection.revision ?? 0}` : translate("Ожидание состояния Core…"))}</p>
    {projection?.session_id && <p>{translate("Сессия:")}{translate(projection.session_id)}</p>}
    {projection?.profile_policy && <p>{translate("Профиль:")}{translate(projection.profile_policy)} {translate("· сеть:")}{translate(projection.network_policy)}</p>}
    {projection?.error_code && <p role="alert">{translate("Ошибка:")}{translate(projection.error_code)}</p>}
    {!projection?.session_id && <button type="button" onClick={() => void create()}>{translate("Создать сессию")}</button>}
    <p>CDP: {translate(projection?.cdp_endpoint ? translate("запрещён к показу") : translate("не передаётся"))} · credentials: {translate(projection?.credentials ? translate("запрещены") : translate("не передаются"))}</p>
  </section>
}
