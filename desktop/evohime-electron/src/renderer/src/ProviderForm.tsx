import { useCallback, useEffect, useState } from 'react'

import {
  PROVIDER_KINDS,
  type ModelTier,
  type ProviderKind,
  type ProviderSummary
} from '@shared/api'

import { useShellApi } from './shell-api'

/**
 * Credentials surface.
 *
 * Everyday use needs two decisions: paste a key and pick free or paid models.
 * Provider and endpoint stay behind a disclosure so the common path is not
 * buried under them. The key itself is write-only from here: it goes to the
 * main process, which encrypts it with the OS key store and hands it to Core.
 */

const PROVIDER_LABELS: Record<ProviderKind, string> = {
  literouter: 'LiteRouter',
  openai_compatible: 'OpenAI API (Chat Completions)',
  openai_responses: 'OpenAI Responses / Codex'
}

const TIERS: readonly { readonly id: ModelTier; readonly label: string; readonly hint: string }[] = [
  { id: 'free', label: 'Бесплатные', hint: 'модели с суффиксом :free' },
  { id: 'paid', label: 'Платные', hint: 'списываются с баланса провайдера' }
]

type Status =
  | { readonly kind: 'idle' }
  | { readonly kind: 'saving' }
  | { readonly kind: 'saved'; readonly restarted: boolean }
  | { readonly kind: 'failed'; readonly message: string }

export function ProviderForm(): React.JSX.Element {
  const api = useShellApi()
  const [summary, setSummary] = useState<ProviderSummary | null>(null)
  const [provider, setProvider] = useState<ProviderKind>('literouter')
  const [apiKey, setApiKey] = useState('')
  const [tier, setTier] = useState<ModelTier>('free')
  const [baseUrl, setBaseUrl] = useState('')
  const [status, setStatus] = useState<Status>({ kind: 'idle' })

  // Fields stay controlled even if a summary arrives with a missing member.
  const apply = useCallback((value: ProviderSummary) => {
    setSummary(value)
    setProvider(PROVIDER_KINDS.includes(value.provider) ? value.provider : 'literouter')
    setTier(value.tier === 'paid' ? 'paid' : 'free')
    setBaseUrl(value.baseUrl ?? '')
  }, [])

  useEffect(() => {
    if (!api) return
    void api.invoke('provider.get', {}).then((outcome) => {
      if (outcome.ok) apply(outcome.value)
    })
  }, [api, apply])

  const save = useCallback(async () => {
    if (!api) return
    setStatus({ kind: 'saving' })
    const outcome = await api.invoke('provider.save', {
      provider,
      apiKey,
      // The model is chosen per task in the composer, so it is not edited here.
      model: summary?.model ?? '',
      baseUrl,
      tier
    })
    if (!outcome.ok) {
      setStatus({ kind: 'failed', message: outcome.message })
      return
    }
    apply(outcome.value.summary)
    setApiKey('')
    setStatus({ kind: 'saved', restarted: outcome.value.restarted })
  }, [api, apiKey, apply, baseUrl, provider, summary?.model, tier])

  const clearKey = useCallback(async () => {
    if (!api) return
    setStatus({ kind: 'saving' })
    const outcome = await api.invoke('provider.clearKey', {})
    if (!outcome.ok) {
      setStatus({ kind: 'failed', message: outcome.message })
      return
    }
    apply(outcome.value.summary)
    setApiKey('')
    setStatus({ kind: 'saved', restarted: outcome.value.restarted })
  }, [api, apply])

  const busy = status.kind === 'saving'
  const canSave = !busy && (apiKey.trim().length > 0 || summary?.configured === true)

  return (
    <section className="shell__panel provider-form" aria-label="Ключ провайдера">
      <div className="settings-panel__heading">
        <div>
          <h2>Доступ к моделям</h2>
          <p className="shell__empty">
            Ключ шифруется средствами Windows и хранится локально. Модель выбирается в чате.
          </p>
        </div>
        <span
          className={`settings-panel__state settings-panel__state--${summary?.configured ? 'ready' : 'offline'}`}
        >
          {summary?.configured ? 'Ключ сохранён' : 'Ключ не задан'}
        </span>
      </div>

      <div className="provider-form__grid">
        <label htmlFor="provider-kind">
          Провайдер
          <select
            id="provider-kind"
            value={provider}
            onChange={(event) => setProvider(event.target.value as ProviderKind)}
            disabled={busy}
          >
            {PROVIDER_KINDS.map((kind) => (
              <option key={kind} value={kind}>{PROVIDER_LABELS[kind]}</option>
            ))}
          </select>
        </label>

        <label className="provider-form__key" htmlFor="provider-key">
          Ключ API
          <input
            id="provider-key"
            type="password"
            value={apiKey}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setApiKey(event.target.value)}
            placeholder={summary?.configured ? 'сохранён — введи новый, чтобы заменить' : 'sk-…'}
            disabled={busy}
          />
        </label>

        <label htmlFor="provider-url">
          Адрес API
          <input
            id="provider-url"
            value={baseUrl}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setBaseUrl(event.target.value)}
            placeholder="по умолчанию провайдера"
            disabled={busy}
          />
        </label>
      </div>

      <fieldset className="provider-form__tier">
        <legend>Какие модели показывать</legend>
        {TIERS.map((item) => (
          <label key={item.id}>
            <input
              type="radio"
              name="model-tier"
              value={item.id}
              checked={tier === item.id}
              onChange={() => setTier(item.id)}
              disabled={busy}
            />
            <span>{item.label}</span>
            <span className="provider-form__hint">{item.hint}</span>
          </label>
        ))}
      </fieldset>

      <div className="provider-form__actions">
        <button type="button" onClick={() => void save()} disabled={!canSave}>
          Сохранить и перезапустить
        </button>
        {summary?.configured ? (
          <button type="button" onClick={() => void clearKey()} disabled={busy}>
            Удалить ключ
          </button>
        ) : null}
      </div>

      {status.kind === 'saved' ? (
        <p className={status.restarted ? 'provider-form__ok' : 'shell__reason'}>
          {status.restarted
            ? 'Сохранено, Core перезапущен — подключение восстановится за пару секунд.'
            : 'Сохранено, но Core не перезапустился. Перезапусти приложение вручную.'}
        </p>
      ) : null}
      {status.kind === 'failed' ? (
        <p role="alert" className="shell__reason">{status.message}</p>
      ) : null}
    </section>
  )
}
