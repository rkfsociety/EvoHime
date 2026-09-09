import { useState } from 'react'
import type { ConnectionState } from '@shared/api'
import { useShellApi } from './shell-api'

export function AgentProgramOptimizerPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api=useShellApi(); const [programId,setProgramId]=useState('program'); const [payload,setPayload]=useState('{}'); const [message,setMessage]=useState('')
  const send=async(operation:'save'|'get'|'optimize'|'pin'|'activate'|'supersede')=>{if(!api||connection!=='connected'){setMessage('Нет подключения к Core.');return} const result=await api.invoke('core.agentProgramOptimizer',{operation,programId,payload,expectedRevision:0,idempotencyKey:crypto.randomUUID()});setMessage(result.ok?'Запрос принят Core; optimizer остаётся deterministic metadata-only projection.':result.message)}
  return <section className="panel" aria-label="Agent Program Optimizer"><h2>Agent Program Optimizer</h2><p>Core владеет revision, policy и pin; шаги программы не исполняются этим слоем.</p><input value={programId} onChange={event=>setProgramId(event.target.value)} maxLength={128}/><textarea value={payload} onChange={event=>setPayload(event.target.value)} maxLength={512*1024}/><div>{(['save','get','optimize','pin','activate','supersede'] as const).map(operation=><button key={operation} type="button" onClick={()=>void send(operation)}>{operation}</button>)}</div>{message?<p role="status">{message}</p>:null}</section>
}
