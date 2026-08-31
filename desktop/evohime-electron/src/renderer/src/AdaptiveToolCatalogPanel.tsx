import { useMemo } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

/** Metadata-only projection of the Core's current model tool loadout. */
export function AdaptiveToolCatalogPanel({ connection, events }: Props): React.JSX.Element {
  const latest = useMemo(() => {
    const event = events.find((item) => item.eventType === 'model.context')
    if (!event) return null
    try {
      const payload = JSON.parse(event.payload) as { tools?: unknown; context?: { tools?: unknown } }
      const tools = Array.isArray(payload.tools) ? payload.tools : payload.context?.tools
      return Array.isArray(tools) ? tools.filter((tool): tool is string => typeof tool === 'string').slice(0, 32) : []
    } catch {
      return []
    }
  }, [events])

  return (
    <section className="settings-info" aria-label="Adaptive Tool Catalog">
      <h3>Adaptive Tool Catalog</h3>
      <p>Core передаёт модели только bounded loadout разрешённых инструментов. Полные schemas не попадают в renderer.</p>
      <small>Состояние: {connection} · последний Core snapshot: {latest ? 'получен' : 'ожидается'}</small>
      {latest && latest.length > 0 ? (
        <ul className="skill-catalog__list" aria-label="Выбранные инструменты">
          {latest.map((tool) => <li key={tool}><code>{tool}</code></li>)}
        </ul>
      ) : <span className="settings-info__badge">Выбор инструментов ещё не опубликован</span>}
    </section>
  )
}
