import type { CoreEvent } from '@shared/api'

export interface ContextUsageProps {
  readonly events: readonly CoreEvent[]
}

interface ContextSnapshot {
  readonly used: number
  readonly limit: number
}

/** Компактный индикатор заполнения окна контекста текущей задачи. */
export function ContextUsage({ events }: ContextUsageProps): React.JSX.Element | null {
  const event = events
    .filter((item) => item.eventType === 'model.context')
    .reduce<CoreEvent | null>((latest, item) => (
      latest === null || item.sequenceId > latest.sequenceId ? item : latest
    ), null)
  const snapshot = event === null ? null : parseSnapshot(event.payload)
  const ratio = snapshot === null ? 0 : Math.min(snapshot.used / snapshot.limit, 1)
  const percent = Math.round(ratio * 100)
  const label = snapshot === null
    ? 'Текущий контекст: пока не рассчитан'
    : `Текущий контекст: ${percent}% (${formatTokens(snapshot.used)} из ${formatTokens(snapshot.limit)} токенов)`

  return (
    <span
      className="context-usage"
      role="status"
      aria-label={label}
      title={label}
      style={{ '--context-ratio': `${ratio * 360}deg` } as React.CSSProperties}
    >
      <span className="context-usage__ring" aria-hidden="true">
        <span className="context-usage__value">{percent}</span>
      </span>
    </span>
  )
}

function parseSnapshot(payload: string): ContextSnapshot | null {
  try {
    const value: unknown = JSON.parse(payload)
    if (typeof value !== 'object' || value === null) return null
    const record = value as Record<string, unknown>
    const used = numberValue(record['estimated_tokens'])
    const limit = numberValue(record['context_limit_tokens'])
    return used !== null && limit !== null && limit > 0 ? { used, limit } : null
  } catch {
    return null
  }
}

function numberValue(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null
}

function formatTokens(value: number): string {
  return new Intl.NumberFormat('ru-RU').format(value)
}
