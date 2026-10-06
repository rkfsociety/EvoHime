import { translate } from './i18n'
import { useCallback, useEffect, useState } from 'react'

import type { CodexStatus } from '@shared/api'

import { useShellApi } from './shell-api'
import { CodexRateLimits } from './CodexRateLimits'

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
          <h3>{translate("Codex CLI для Евы")}</h3>
        </div>
        <button type="button" className="button button--secondary" disabled={busy} onClick={() => void load(true)}>
          {translate(busy ? translate("Обновление…") : translate("Обновить"))}
        </button>
      </div>
      <p className="settings-info__text">
        {translate("Данные берутся из локального Codex CLI, авторизованного через ChatGPT. Ключ API для этого не нужен.")}</p>
      {status?.available ? (
        <>
          <label className="field">
            <span className="field__label">{translate("Модель Codex")}</span>
            <select value={status.selectedModel} disabled={busy || status.models.length === 0} onChange={(event) => void selectModel(event.target.value)}>
              {status.models.map((model) => <option key={model.id} value={model.id}>{translate(model.displayName)} ({translate(model.id)})</option>)}
            </select>
          </label>
          <div className="settings-info__details">
            <strong>{translate("Остаток лимита")}</strong>
            <CodexRateLimits rateLimits={status.rateLimits} />
          </div>
        </>
      ) : (
        <>
          <p className="settings-info__badge">{translate(status?.error ?? translate("Проверяем вход в Codex…"))}</p>
          {status && !status.installed ? (
            <button type="button" onClick={() => void install()} disabled={busy}>
              {translate(status.installing ? translate("Установка Codex CLI…") : translate("Установить Codex CLI"))}
            </button>
          ) : null}
          {status?.installed ? (
            <button type="button" onClick={() => void login()} disabled={busy || status.loggingIn}>
              {translate(status.loggingIn ? translate("Заверши вход в окне Codex CLI…") : translate("Войти через ChatGPT"))}
            </button>
          ) : null}
        </>
      )}
      {message && message !== status?.error ? <p className="form-status">{translate(message)}</p> : null}
    </section>
  )
}
