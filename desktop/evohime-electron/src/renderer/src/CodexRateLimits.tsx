import type { CodexRateLimit } from '@shared/api'

export function CodexRateLimits({ rateLimits, compact = false }: { readonly rateLimits: readonly CodexRateLimit[]; readonly compact?: boolean }): React.JSX.Element {
  return (
    <div className={`codex-rate-limits${compact ? ' codex-rate-limits--compact' : ''}`} data-testid="codex-composer-limits">
      {rateLimits.length === 0 ? (
        <span className="codex-rate-limits__empty">Лимиты Codex пока недоступны</span>
      ) : rateLimits.map((limit) => (
        <div key={limit.limitId} className="codex-rate-limits__group">
          {!compact ? <span>{limit.limitId}{limit.planType ? ` · ${limit.planType}` : ''}</span> : null}
          {windowsFor(limit).length === 0 ? <small>данные о лимите не переданы</small> : windowsFor(limit).map((window) => (
            <span key={window.label} className="codex-limit-window">
              <strong>{window.label}: осталось {window.remaining}%</strong>
              <small>{formatReset(window.resetsAt)}</small>
            </span>
          ))}
        </div>
      ))}
    </div>
  )
}

function windowsFor(limit: CodexRateLimit): readonly { label: string; remaining: number; resetsAt: number | null }[] {
  return [
    limit.individualRemainingPercent !== null
      ? { label: 'Индивидуальный', remaining: limit.individualRemainingPercent, resetsAt: limit.individualResetsAt }
      : null,
    limit.primary ? { label: windowLabel(limit.primary.windowDurationMins, 'Основной'), remaining: limit.primary.remainingPercent, resetsAt: limit.primary.resetsAt } : null,
    limit.secondary ? { label: windowLabel(limit.secondary.windowDurationMins, 'Дополнительный'), remaining: limit.secondary.remainingPercent, resetsAt: limit.secondary.resetsAt } : null
  ].filter((item): item is { label: string; remaining: number; resetsAt: number | null } => item !== null)
}

function windowLabel(durationMins: number | null, fallback: string): string {
  if (durationMins === 300) return '5 часов'
  if (durationMins === 10080) return 'Неделя'
  if (durationMins === null) return fallback
  if (durationMins % 1440 === 0) return `${durationMins / 1440} дн.`
  if (durationMins % 60 === 0) return `${durationMins / 60} ч.`
  return `${durationMins} мин.`
}

function formatReset(timestamp: number | null): string {
  if (timestamp === null) return 'время сброса не передано'
  const value = new Date(timestamp * 1000)
  return Number.isNaN(value.getTime()) ? 'время сброса не передано' : `сброс ${value.toLocaleString('ru-RU')}`
}
