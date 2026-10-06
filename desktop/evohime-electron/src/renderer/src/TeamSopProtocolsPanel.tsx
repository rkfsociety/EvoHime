import { translate } from './i18n'
import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

type Projection = { protocol_count?: number; session_count?: number; protocol_id?: string; current_phase?: string; state?: string; error_code?: string; participant_count?: number; phase_count?: number; handoff_count?: number }
/** Metadata-only Team SOP projection; all transitions remain Core-owned. */
export function TeamSopProtocolsPanel(): React.JSX.Element {
  const api=useShellApi(); const [p,setP]=useState<Projection|null>(null)
  useEffect(()=>{if(!api)return; const off=api.subscribe(e=>{if(e.kind!=='core-event'||e.event.eventType!=='team_sop_protocols.result')return;try{setP(JSON.parse(e.event.payload).projection_json as Projection)}catch{setP({error_code:'invalid_projection'})}});void api.invoke('teamSopProtocols.list',{requestId:crypto.randomUUID(),ownerScope:'team-sop',idempotencyKey:crypto.randomUUID()});return off},[api])
  return <section className="panel" aria-label="Team SOP Protocols"><h2>Team SOP Protocols</h2><p role="status">{translate("Протоколов:")}{translate(p?.protocol_count??0)} {translate("· сессий:")}{translate(p?.session_count??0)}</p>{p?.protocol_id?<p>Protocol {translate(p.protocol_id)}{translate(": фаз")}{translate(p.phase_count??0)}{translate(", участников")}{translate(p.participant_count??0)}, handoffs {translate(p.handoff_count??0)}</p>:null}<p>{translate("Версия контракта v1 · storage v49 · IPC 195–196/48.")}</p><p>{translate("Состояние, grants, approvals и переходы проверяет Core; raw prompts, transcripts и credentials не передаются.")}</p>{p?.error_code?<p role="alert">{translate("Ошибка:")}{translate(p.error_code)}</p>:null}</section>
}
