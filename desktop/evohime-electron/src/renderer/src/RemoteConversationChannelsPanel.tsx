import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
import type { ConnectionState } from '@shared/api'

export function RemoteConversationChannelsPanel({ connection, events }: { readonly connection: ConnectionState; readonly events: readonly { readonly eventType: string; readonly payload: string }[] }): React.JSX.Element {
  const api=useShellApi(); const [id,setId]=useState(''); const [payload,setPayload]=useState(''); const [result,setResult]=useState(''); const [message,setMessage]=useState('')
  useEffect(()=>{const event=events.find(item=>item.eventType==='remote_conversation_channels.result'); if(event)setResult(event.payload)},[events])
  const send=async(operation:'save'|'inspect'|'pair'|'admit'|'revoke'):Promise<void>=>{if(!api||connection!=='connected'||!id.trim()||(operation!=='inspect'&&!payload.trim())){setMessage('Нужны подключение, connection ID и bounded JSON.');return};const response=await api.invoke('core.remoteConversationChannels',{operation,connectionId:id.trim(),payload:payload.trim(),expectedVersion:0,idempotencyKey:crypto.randomUUID()});if(!response.ok)setMessage(response.message)}
  return <section aria-label="Remote Conversation Channels"><h3>Remote Conversation Channels</h3><p>Pairing single-use, identity binding, queue/rate/attachment limits и revoke принадлежат Core; high-risk approval остаётся desktop-only.</p><label>Connection ID <input value={id} onChange={event=>setId(event.target.value)} maxLength={128}/></label><label>Channel JSON <textarea value={payload} onChange={event=>setPayload(event.target.value)} maxLength={512*1024}/></label><div>{(['save','inspect','pair','admit','revoke'] as const).map(operation=><button key={operation} type="button" onClick={()=>void send(operation)}>{operation}</button>)}</div>{result?<pre>{result}</pre>:null}{message?<p role="status">{message}</p>:null}</section>
}
