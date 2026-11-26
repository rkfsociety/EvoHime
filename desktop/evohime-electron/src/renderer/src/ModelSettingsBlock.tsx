import type { ModelCapabilitiesSettings } from '@shared/api'

import { translate } from './i18n'

/** Fixed integer UTC hour offsets from UTC -12 to UTC +14. */
export const UTC_OFFSET_HOURS: readonly number[] = Array.from({ length: 27 }, (_, index) => index - 12)

/** Formats a UTC hour offset as `UTC +2`, `UTC -3`, or `UTC +0`. */
export function formatUtcOffset(hours: number): string {
  return hours >= 0 ? `UTC +${hours}` : `UTC ${hours}`
}

/** Converts minute-of-day (`0..1439`) to `HH:MM`. */
export function formatResetTime(minutes: number): string {
  const clamped = Math.max(0, Math.min(1_439, Math.trunc(minutes)))
  const hh = String(Math.floor(clamped / 60)).padStart(2, '0')
  const mm = String(clamped % 60).padStart(2, '0')
  return `${hh}:${mm}`
}

/** Parses `HH:MM` into minute-of-day (`0..1439`), returning `null` if invalid. */
export function parseResetTime(value: string): number | null {
  const match = /^(\d{2}):(\d{2})$/.exec(value.trim())
  if (!match) return null
  const hh = Number(match[1])
  const mm = Number(match[2])
  if (!Number.isInteger(hh) || !Number.isInteger(mm) || hh < 0 || hh > 23 || mm < 0 || mm > 59) return null
  return hh * 60 + mm
}

export interface LimitResetScheduleFieldsProps {
  readonly idPrefix: string
  readonly enabled: boolean
  readonly resetTime: string
  readonly utcOffsetHours: number
  readonly disabled: boolean
  readonly onEnabledChange: (enabled: boolean) => void
  readonly onResetTimeChange: (value: string) => void
  readonly onUtcOffsetChange: (hours: number) => void
}

/** Shared reset schedule controls (rolling 24h vs daily HH:MM in provider UTC offset). */
export function LimitResetScheduleFields({
  idPrefix,
  enabled,
  resetTime,
  utcOffsetHours,
  disabled,
  onEnabledChange,
  onResetTimeChange,
  onUtcOffsetChange
}: LimitResetScheduleFieldsProps): React.JSX.Element {
  return (
    <div className="provider-form__grid">
      <label htmlFor={`${idPrefix}-reset-mode`}>
        {translate('Сброс лимитов')}
        <select
          id={`${idPrefix}-reset-mode`}
          value={enabled ? 'scheduled' : 'rolling'}
          onChange={(event) => onEnabledChange(event.target.value === 'scheduled')}
          disabled={disabled}
        >
          <option value="rolling">{translate('Скользящее окно (24 ч)')}</option>
          <option value="scheduled">{translate('По расписанию')}</option>
        </select>
      </label>
      <label htmlFor={`${idPrefix}-reset-time`}>
        {translate('Время сброса')}
        <input
          id={`${idPrefix}-reset-time`}
          type="time"
          value={resetTime}
          onChange={(event) => onResetTimeChange(event.target.value)}
          disabled={disabled || !enabled}
        />
      </label>
      <label htmlFor={`${idPrefix}-reset-tz`}>
        {translate('Часовой пояс')}
        <select
          id={`${idPrefix}-reset-tz`}
          value={String(utcOffsetHours)}
          onChange={(event) => onUtcOffsetChange(Number(event.target.value))}
          disabled={disabled || !enabled}
        >
          {UTC_OFFSET_HOURS.map((offset) => (
            <option key={offset} value={String(offset)}>
              {formatUtcOffset(offset)}
            </option>
          ))}
        </select>
      </label>
    </div>
  )
}

const MODEL_CAPABILITY_FIELDS: ReadonlyArray<{
  readonly key: keyof ModelCapabilitiesSettings
  readonly label: string
}> = [
  { key: 'jsonOutput', label: 'JSON Output' },
  { key: 'toolCalls', label: 'Tool Calls' },
  { key: 'responsesApi', label: 'Responses API' },
  { key: 'anthropicApi', label: 'Anthropic API' },
  { key: 'chatPrefixCompletion', label: 'Chat Prefix Completion' },
  { key: 'fimCompletion', label: 'FIM Completion' },
  { key: 'vision', label: 'Vision' }
]

export interface ModelLimitDraft {
  readonly key: string
  readonly modelId: string
  readonly enabled: boolean
  readonly rpm: string
  readonly rph: string
  readonly rp4h: string
  readonly rpd: string
  readonly concurrencyLimit: string
  readonly contextLengthTokens: string
  readonly maxOutputTokens: string
  readonly capabilities: Required<ModelCapabilitiesSettings>
  readonly dailyTokenQuota: string
  readonly resetScheduleEnabled: boolean
  readonly resetTime: string
  readonly resetUtcOffsetHours: number
  readonly pricingEnabled: boolean
  readonly inputUsdPer1M: string
  readonly cachedInputUsdPer1M: string
  readonly outputUsdPer1M: string
  readonly peakInputUsdPer1M: string
  readonly peakCachedInputUsdPer1M: string
  readonly peakOutputUsdPer1M: string
}

export interface ModelSettingsBlockProps {
  readonly entries: readonly ModelLimitDraft[]
  readonly catalogModels: readonly string[]
  readonly busy: boolean
  readonly hideRateLimits?: boolean
  readonly supportsExtendedModelSpecs?: boolean
  readonly onUpdateEntry: (key: string, patch: Partial<Omit<ModelLimitDraft, 'key'>>) => void
  readonly onAddEntry: () => void
  readonly onRemoveEntry: (key: string) => void
}

/**
 * Renders the provider-wide `*` card (request limits plus daily token quota)
 * together with per-model rate limit cards and allows adding multiple models.
 * Designed as a dedicated block so new per-model settings can be added per card.
 */
export function ModelSettingsBlock({
  entries,
  catalogModels,
  busy,
  hideRateLimits = false,
  supportsExtendedModelSpecs = false,
  onUpdateEntry,
  onAddEntry,
  onRemoveEntry
}: ModelSettingsBlockProps): React.JSX.Element {
  return (
    <fieldset className="provider-form__tier" aria-label={translate(hideRateLimits ? 'Параметры и стоимость моделей' : 'Лимиты запросов модели')}>
      <legend>{translate(hideRateLimits ? 'Параметры и стоимость моделей' : 'Лимиты запросов провайдера и моделей')}</legend>
      <p className="provider-form__hint">
        {translate(
          hideRateLimits
            ? 'Укажи характеристики модели, лимит параллельных запросов (Concurrency) и стоимость токенов для расчёта и маршрутизации.'
            : 'Ноль означает без ограничения. Задай общие лимиты для всех моделей или отдельные лимиты для конкретных моделей; по умолчанию ограничения выключены.'
        )}
      </p>
      <div className="provider-form__model-cards">
        {entries.map((entry, index) => {
          const trimmedModel = entry.modelId.trim()
          if (hideRateLimits && trimmedModel === '*') return null
          const hasCustomOption = trimmedModel.length > 0 && trimmedModel !== '*' && !catalogModels.includes(trimmedModel)
          const hasOptions = true
          const allowManualInput = catalogModels.length === 0 && index > 0
          const modelInputId = `provider-limit-model-${entry.key}`
          const rpmId = `provider-model-rpm-${entry.key}`
          const rphId = `provider-model-rph-${entry.key}`
          const rp4hId = `provider-model-rp4h-${entry.key}`
          const rpdId = `provider-model-rpd-${entry.key}`
          const priceInputId = `provider-model-price-input-${entry.key}`
          const priceCachedId = `provider-model-price-cached-${entry.key}`
          const priceOutputId = `provider-model-price-output-${entry.key}`

          return (
            <div key={entry.key} className="provider-form__model-card">
              <div className="provider-form__model-card-header">
                <label htmlFor={modelInputId}>
                  {translate('ID модели')}
                  {allowManualInput ? (
                    <input
                      id={modelInputId}
                      type="text"
                      value={entry.modelId}
                      onChange={(event) => onUpdateEntry(entry.key, { modelId: event.target.value })}
                      placeholder={translate('например, gpt-4o-mini')}
                      disabled={busy}
                    />
                  ) : (
                    <select
                      id={modelInputId}
                      value={entry.modelId}
                      onChange={(event) => onUpdateEntry(entry.key, { modelId: event.target.value })}
                      disabled={busy || !hasOptions}
                    >
                      {!hasOptions ? (
                        <option value="">{translate('Каталог моделей ещё не загружен')}</option>
                      ) : null}
                      {hasCustomOption ? (
                        <option value={trimmedModel}>
                          {trimmedModel} ({translate('текущая, не найдена в каталоге')})
                        </option>
                      ) : null}
                      {trimmedModel === '*' ? (
                        <option value="*">{translate('Все модели провайдера')}</option>
                      ) : null}
                      {catalogModels.map((modelId) => (
                        <option key={modelId} value={modelId}>
                          {modelId}
                        </option>
                      ))}
                    </select>
                  )}
                </label>
                {entries.length > 1 ? (
                  <button
                    type="button"
                    className="provider-form__model-remove"
                    onClick={() => onRemoveEntry(entry.key)}
                    disabled={busy}
                  >
                    {translate('Удалить')}
                  </button>
                ) : null}
              </div>

              {!hideRateLimits ? (
                <>
                  <label>
                    <input
                      type="checkbox"
                      checked={entry.enabled}
                      onChange={(event) => onUpdateEntry(entry.key, { enabled: event.target.checked })}
                      disabled={busy || trimmedModel.length === 0}
                    />
                    {translate(trimmedModel === '*' ? 'Включить общие лимиты провайдера' : 'Включить лимиты для этой модели')}
                  </label>

                  <div className="provider-form__grid">
                    <label htmlFor={rpmId}>
                      RPM
                      <input
                        id={rpmId}
                        type="number"
                        min={0}
                        max={4_294_967_295}
                        step={1}
                        value={entry.rpm}
                        onChange={(event) => onUpdateEntry(entry.key, { rpm: event.target.value })}
                        disabled={busy || !entry.enabled}
                      />
                    </label>
                    <label htmlFor={rphId}>
                      RPH
                      <input
                        id={rphId}
                        type="number"
                        min={0}
                        max={4_294_967_295}
                        step={1}
                        value={entry.rph}
                        onChange={(event) => onUpdateEntry(entry.key, { rph: event.target.value })}
                        disabled={busy || !entry.enabled}
                      />
                    </label>
                    <label htmlFor={rp4hId}>
                      RP4H
                      <input
                        id={rp4hId}
                        type="number"
                        min={0}
                        max={4_294_967_295}
                        step={1}
                        value={entry.rp4h}
                        onChange={(event) => onUpdateEntry(entry.key, { rp4h: event.target.value })}
                        disabled={busy || !entry.enabled}
                      />
                    </label>
                    <label htmlFor={rpdId}>
                      RPD
                      <input
                        id={rpdId}
                        type="number"
                        min={0}
                        max={4_294_967_295}
                        step={1}
                        value={entry.rpd}
                        onChange={(event) => onUpdateEntry(entry.key, { rpd: event.target.value })}
                        disabled={busy || !entry.enabled}
                      />
                    </label>
                  </div>
                </>
              ) : null}

              {supportsExtendedModelSpecs && trimmedModel !== '*' ? (
                <>
                  <div className="provider-form__grid">
                    <label htmlFor={`provider-concurrency-${entry.key}`}>
                      {translate('Лимит параллельных запросов (Concurrency)')}
                      <input
                        id={`provider-concurrency-${entry.key}`}
                        type="number"
                        min={0}
                        max={4_294_967_295}
                        step={1}
                        value={entry.concurrencyLimit}
                        onChange={(event) => onUpdateEntry(entry.key, { concurrencyLimit: event.target.value })}
                        disabled={busy}
                      />
                    </label>
                    <label htmlFor={`provider-context-len-${entry.key}`}>
                      {translate('Длина контекста (токены)')}
                      <input
                        id={`provider-context-len-${entry.key}`}
                        type="number"
                        min={0}
                        max={1_000_000_000}
                        step={1}
                        value={entry.contextLengthTokens}
                        onChange={(event) => onUpdateEntry(entry.key, { contextLengthTokens: event.target.value })}
                        disabled={busy}
                      />
                    </label>
                    <label htmlFor={`provider-max-output-${entry.key}`}>
                      {translate('Макс. токенов ответа (Max Output)')}
                      <input
                        id={`provider-max-output-${entry.key}`}
                        type="number"
                        min={0}
                        max={1_000_000_000}
                        step={1}
                        value={entry.maxOutputTokens}
                        onChange={(event) => onUpdateEntry(entry.key, { maxOutputTokens: event.target.value })}
                        disabled={busy}
                      />
                    </label>
                  </div>
                  <div className="provider-form__grid">
                    {MODEL_CAPABILITY_FIELDS.map((cap) => (
                      <label key={cap.key}>
                        <input
                          type="checkbox"
                          checked={entry.capabilities[cap.key]}
                          onChange={(event) =>
                            onUpdateEntry(entry.key, {
                              capabilities: { ...entry.capabilities, [cap.key]: event.target.checked }
                            })
                          }
                          disabled={busy}
                        />
                        {cap.label}
                      </label>
                    ))}
                  </div>
                </>
              ) : null}

              {trimmedModel !== '*' ? <label>
                <input
                  type="checkbox"
                  checked={entry.pricingEnabled}
                  onChange={(event) => onUpdateEntry(entry.key, { pricingEnabled: event.target.checked })}
                  disabled={busy || trimmedModel.length === 0}
                />
                {translate('Указать стоимость токенов (USD за 1 млн)')}
              </label> : null}

              {trimmedModel === '*' ? (
                <>
                  <label htmlFor={`provider-daily-token-quota-${entry.key}`}>
                    {translate('Дневная квота токенов (0 — без лимита)')}
                    <input
                      id={`provider-daily-token-quota-${entry.key}`}
                      type="number"
                      min={0}
                      max={1_000_000_000_000}
                      step={1}
                      value={entry.dailyTokenQuota}
                      onChange={(event) => onUpdateEntry(entry.key, { dailyTokenQuota: event.target.value })}
                      disabled={busy || !entry.enabled}
                    />
                  </label>
                  <LimitResetScheduleFields
                    idPrefix={`provider-limit-${entry.key}`}
                    enabled={entry.resetScheduleEnabled}
                    resetTime={entry.resetTime}
                    utcOffsetHours={entry.resetUtcOffsetHours}
                    disabled={busy || !entry.enabled}
                    onEnabledChange={(enabled) => onUpdateEntry(entry.key, { resetScheduleEnabled: enabled })}
                    onResetTimeChange={(resetTime) => onUpdateEntry(entry.key, { resetTime })}
                    onUtcOffsetChange={(resetUtcOffsetHours) => onUpdateEntry(entry.key, { resetUtcOffsetHours })}
                  />
                </>
              ) : null}

              {trimmedModel !== '*' ? <div className="provider-form__grid">
                <label htmlFor={priceInputId}>
                  {translate(supportsExtendedModelSpecs ? 'Вход Cache Miss (Off-Peak, $/1M)' : 'Вход ($ / 1M)')}
                  <input
                    id={priceInputId}
                    type="number"
                    min={0}
                    max={10_000}
                    step="0.001"
                    value={entry.inputUsdPer1M}
                    onChange={(event) => onUpdateEntry(entry.key, { inputUsdPer1M: event.target.value })}
                    disabled={busy || !entry.pricingEnabled}
                  />
                </label>
                <label htmlFor={priceCachedId}>
                  {translate(supportsExtendedModelSpecs ? 'Вход Cache Hit (Off-Peak, $/1M)' : 'Кэш-вход ($ / 1M)')}
                  <input
                    id={priceCachedId}
                    type="number"
                    min={0}
                    max={10_000}
                    step="0.001"
                    value={entry.cachedInputUsdPer1M}
                    onChange={(event) => onUpdateEntry(entry.key, { cachedInputUsdPer1M: event.target.value })}
                    disabled={busy || !entry.pricingEnabled}
                  />
                </label>
                <label htmlFor={priceOutputId}>
                  {translate(supportsExtendedModelSpecs ? 'Выход (Off-Peak, $/1M)' : 'Выход ($ / 1M)')}
                  <input
                    id={priceOutputId}
                    type="number"
                    min={0}
                    max={10_000}
                    step="0.001"
                    value={entry.outputUsdPer1M}
                    onChange={(event) => onUpdateEntry(entry.key, { outputUsdPer1M: event.target.value })}
                    disabled={busy || !entry.pricingEnabled}
                  />
                </label>
                {supportsExtendedModelSpecs ? (
                  <>
                    <label htmlFor={`provider-peak-input-${entry.key}`}>
                      {translate('Вход Cache Miss (Peak, $/1M)')}
                      <input
                        id={`provider-peak-input-${entry.key}`}
                        type="number"
                        min={0}
                        max={10_000}
                        step="0.001"
                        value={entry.peakInputUsdPer1M}
                        onChange={(event) => onUpdateEntry(entry.key, { peakInputUsdPer1M: event.target.value })}
                        disabled={busy || !entry.pricingEnabled}
                      />
                    </label>
                    <label htmlFor={`provider-peak-cached-${entry.key}`}>
                      {translate('Вход Cache Hit (Peak, $/1M)')}
                      <input
                        id={`provider-peak-cached-${entry.key}`}
                        type="number"
                        min={0}
                        max={10_000}
                        step="0.001"
                        value={entry.peakCachedInputUsdPer1M}
                        onChange={(event) => onUpdateEntry(entry.key, { peakCachedInputUsdPer1M: event.target.value })}
                        disabled={busy || !entry.pricingEnabled}
                      />
                    </label>
                    <label htmlFor={`provider-peak-output-${entry.key}`}>
                      {translate('Выход (Peak, $/1M)')}
                      <input
                        id={`provider-peak-output-${entry.key}`}
                        type="number"
                        min={0}
                        max={10_000}
                        step="0.001"
                        value={entry.peakOutputUsdPer1M}
                        onChange={(event) => onUpdateEntry(entry.key, { peakOutputUsdPer1M: event.target.value })}
                        disabled={busy || !entry.pricingEnabled}
                      />
                    </label>
                  </>
                ) : null}
              </div> : null}
            </div>
          )
        })}
      </div>

      <div className="provider-form__actions">
        <button type="button" onClick={onAddEntry} disabled={busy}>
          {translate('Добавить модель')}
        </button>
      </div>
    </fieldset>
  )
}
