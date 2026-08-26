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

  const install = useCallback(async () => {
    if (!api) return
    setBusy(true)
    setMessage('Устанавливаем Codex CLI…')
    const outcome = await api.invoke('codex.install', {})
    if (outcome.ok) {
      setStatus(outcome.value)
      setMessage(outcome.value.error ?? (outcome.value.available ? 'Codex CLI установлен.' : 'Codex CLI установлен, но требуется вход.'))
    } else {
      setMessage(outcome.message)
    }
    setBusy(false)
  }, [api])

  const login = useCallback(async () => {
    if (!api) return
    setBusy(true)
    const outcome = await api.invoke('codex.login', {})
    if (outcome.ok) {
      setStatus(outcome.value)
      setMessage(outcome.value.error ?? 'Вход через ChatGPT запущен.')
    } else {
      setMessage(outcome.message)
    }
    setBusy(false)
  }, [api])

  return (
    <section className="shell__panel provider-form" aria-label="Codex">
      <div className="settings-panel__heading">
        <div>
          <p className="settings-modal__eyebrow">ChatGPT + Codex CLI</p>
          <h3>Codex CLI для Евы</h3>
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
        <>
          <p className="settings-info__badge">{status?.error ?? 'Проверяем вход в Codex…'}</p>
          {status && !status.installed ? (
            <button type="button" onClick={() => void install()} disabled={busy}>
              {status.installing ? 'Установка Codex CLI…' : 'Установить Codex CLI'}
            </button>
          ) : null}
          {status?.installed ? (
            <button type="button" onClick={() => void login()} disabled={busy || status.loggingIn}>
              {status.loggingIn ? 'Заверши вход в окне Codex CLI…' : 'Войти через ChatGPT'}
            </button>
          ) : null}
        </>
      )}
      {message && message !== status?.error ? <p className="form-status">{message}</p> : null}
    </section>
  )
}

function RateLimitView({ limit }: { readonly limit: CodexRateLimit }): React.JSX.Element {
  const windows = [
    limit.individualRemainingPercent !== null
      ? { label: 'Индивидуальный', remaining: limit.individualRemainingPercent, resetsAt: limit.individualResetsAt }
      : null,
    limit.primary ? { label: windowLabel(limit.primary.windowDurationMins, 'Основной'), remaining: limit.primary.remainingPercent, resetsAt: limit.primary.resetsAt } : null,
    limit.secondary ? { label: windowLabel(limit.secondary.windowDurationMins, 'Дополнительный'), remaining: limit.secondary.remainingPercent, resetsAt: limit.secondary.resetsAt } : null
  ].filter((item): item is { label: string; remaining: number; resetsAt: number | null } => item !== null)

  return (
    <div className="settings-info__detail" data-testid={`codex-limit-${limit.limitId}`}>
      <span>{limit.limitId}{limit.planType ? ` · ${limit.planType}` : ''}</span>
      {windows.length === 0 ? <small>данные о лимите не переданы</small> : windows.map((window) => (
        <span key={window.label} className="codex-limit-window">
          <strong>{window.label}: осталось {window.remaining}%</strong>
          <small>{formatReset(window.resetsAt)}</small>
        </span>
      ))}
    </div>
  )
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
