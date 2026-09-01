import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
type Message={message_id?:string;kind?:string;sequence?:number;payload_hash?:string;delivery?:string}
type Projection={messages?:Message[];count?:number;error_code?:string}
/** Metadata-only projection; the Core remains the sole owner of routing and delivery. */
export function CausalCollaborationBusPanel(): React.JSX.Element {
  const api=useShellApi(); const [p,setP]=useState<Projection|null>(null)
  useEffect(()=>{if(!api)return; const off=api.subscribe(e=>{if(e.kind!=='core-event'||e.event.eventType!=='causal_collaboration_bus.result')return;try{const x=JSON.parse(e.event.payload) as {projection_json?:Projection;error_code?:string};setP(x.projection_json??(x.error_code?{error_code:x.error_code}:{error_code:'unknown'}))}catch{setP({error_code:'invalid_projection'})}});void api.invoke('causalCollaborationBus.list',{requestId:crypto.randomUUID(),ownerScope:'team-sop',idempotencyKey:crypto.randomUUID(),correlationId:crypto.randomUUID()});return off},[api])
  return <section className="panel" aria-label="Causal Collaboration Bus"><h2>Causal Collaboration Bus</h2><p role="status">Сообщений: {p?.count??p?.messages?.length??0}</p>{p?.messages?.map(m=><div key={m.message_id??m.sequence}><strong>{m.kind??'message'}</strong> · seq {m.sequence??'—'} · {m.delivery??'queued'} · hash {m.payload_hash?.slice(0,12)??'—'}</div>)}<p>Core-owned routing, bounded inbox v1; payload, prompts, credentials и grants в renderer не передаются.</p>{p?.error_code?<p role="alert">Ошибка: {p.error_code}</p>:null}</section>
}
