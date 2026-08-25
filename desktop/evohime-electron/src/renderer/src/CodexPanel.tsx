import { useCallback, useEffect, useState } from 'react'

import type { CodexRateLimit, CodexStatus } from '@shared/api'

import { useShellApi } from './shell-api'

export function CodexPanel(): React.JSX.Element {
  const api = useShellApi()
  const [status, setStatus] = useState<CodexStatus | null>(null)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState('')

  const load = useCallback(async (refresh: boolean) => {
    if (!api) return
    setBusy(true)
    const outcome = await api.invoke(refresh ? 'codex.refresh' : 'codex.getStatus', {})
    if (outcome.ok) {
      setStatus(outcome.value)
      setMessage(outcome.value.error ?? '')
    } else {
      setMessage(outcome.message)
    }
    setBusy(false)
  }, [api])

  useEffect(() => { void load(false) }, [load])

  const selectModel = useCallback(async (model: string) => {
    if (!api) return
    setBusy(true)
    const outcome = await api.invoke('codex.selectModel', { model })
    if (outcome.ok) {
      setStatus(outcome.value)
      setMessage('Модель Codex выбрана.')
    } else {
      setMessage(outcome.message)
    }
    setBusy(false)
  }, [api])

  return (
    <section className="shell__panel provider-form" aria-label="Codex">
      <div className="settings-panel__heading">
        <div>
          <p className="settings-modal__eyebrow">ChatGPT + Codex</p>
          <h3>Codex для Евы</h3>
        </div>
        <button type="button" className="button button--secondary" disabled={busy} onClick={() => void load(true)}>
          {busy ? 'Обновление…' : 'Обновить'}
        </button>
      </div>
      <p className="settings-info__text">
        Данные берутся из локального Codex CLI, авторизованного через ChatGPT. Ключ API для этого не нужен.
      </p>
      {status?.available ? (
        <>
          <label className="field">
            <span className="field__label">Модель Codex</span>
            <select value={status.selectedModel} disabled={busy || status.models.length === 0} onChange={(event) => void selectModel(event.target.value)}>
              {status.models.map((model) => <option key={model.id} value={model.id}>{model.displayName} ({model.id})</option>)}
            </select>
          </label>
          <div className="settings-info__details">
            <strong>Остаток лимита</strong>
            {status.rateLimits.length > 0
              ? status.rateLimits.map((limit) => <RateLimitView key={limit.limitId} limit={limit} />)
              : <p>Codex не передал данные о лимите.</p>}
          </div>
        </>
      ) : (
        <p className="settings-info__badge">{status?.error ?? 'Проверяем вход в Codex…'}</p>
      )}
      {message && message !== status?.error ? <p className="form-status">{message}</p> : null}
    </section>
  )
}

function RateLimitView({ limit }: { readonly limit: CodexRateLimit }): React.JSX.Element {
  const primary = limit.primary
  const secondary = limit.secondary
  const individual = limit.individualRemainingPercent
  const remaining = individual ?? primary?.remainingPercent ?? secondary?.remainingPercent
  return (
    <div className="settings-info__detail" data-testid={`codex-limit-${limit.limitId}`}>
      <span>{limit.limitId}{limit.planType ? ` · ${limit.planType}` : ''}</span>
      <strong>{remaining === undefined ? 'нет данных' : `осталось ${remaining}%`}</strong>
      <small>{formatReset(individual !== null ? limit.individualResetsAt : primary?.resetsAt ?? secondary?.resetsAt ?? null)}</small>
    </div>
  )
}

function formatReset(timestamp: number | null): string {
  if (timestamp === null) return 'время сброса не передано'
  const value = new Date(timestamp * 1000)
  return Number.isNaN(value.getTime()) ? 'время сброса не передано' : `сброс ${value.toLocaleString('ru-RU')}`
}
