import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'

type Item = { id?: string; revision?: number; title?: string; state?: string; expires_at_ms?: number }
type Projection = { count?: number; items?: Item[]; error_code?: string }

/** Inbox projection only: Core owns transitions, validation and persistence. */
export function HumanWorkItemsPanel(): React.JSX.Element {
  const api = useShellApi(); const [projection, setProjection] = useState<Projection | null>(null)
  useEffect(() => { if (!api) return; const off = api.subscribe((event) => { if (event.kind !== 'core-event' || event.event.eventType !== 'human_work_items.result') return; try { const value = JSON.parse(event.event.payload) as { projection_json?: Projection; error_code?: string }; setProjection(value.projection_json ?? { error_code: value.error_code ?? 'invalid_projection' }) } catch { setProjection({ error_code: 'invalid_projection' }) } }); void api.invoke('humanWorkItems.list', { requestId: crypto.randomUUID(), ownerScope: 'human-work-items', idempotencyKey: crypto.randomUUID() }); return off }, [api])
  return <section className="panel" aria-label="Human Work Items"><h2>Задачи для человека</h2><p role="status">В Inbox: {projection?.count ?? projection?.items?.length ?? 0}</p>{projection?.items?.map((item) => <div key={item.id}><strong>{item.title ?? item.id}</strong> · {item.state ?? '—'} · rev {item.revision ?? '—'}</div>)}<p>Ответ пользователя — только typed data, а не approval или capability grant. Инструкции и ответы валидирует Core; credentials, raw model prompts и скрытое reasoning не передаются.</p>{projection?.error_code ? <p role="alert">Ошибка: {projection.error_code}</p> : null}</section>
}
