import { translate } from './i18n'

export interface ModelLimitDraft {
  readonly key: string
  readonly modelId: string
  readonly enabled: boolean
  readonly rpm: string
  readonly rph: string
  readonly rp4h: string
  readonly rpd: string
  readonly dailyTokenQuota: string
  readonly quotaResetUtcMidnight: boolean
  readonly pricingEnabled: boolean
  readonly inputUsdPer1M: string
  readonly cachedInputUsdPer1M: string
  readonly outputUsdPer1M: string
}

export interface ModelSettingsBlockProps {
  readonly entries: readonly ModelLimitDraft[]
  readonly catalogModels: readonly string[]
  readonly busy: boolean
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
  onUpdateEntry,
  onAddEntry,
  onRemoveEntry
}: ModelSettingsBlockProps): React.JSX.Element {
  return (
    <fieldset className="provider-form__tier" aria-label={translate('Лимиты запросов модели')}>
      <legend>{translate('Лимиты запросов провайдера и моделей')}</legend>
      <p className="provider-form__hint">
        {translate('Ноль означает без ограничения. Задай общие лимиты для всех моделей или отдельные лимиты для конкретных моделей; по умолчанию ограничения выключены.')}
      </p>
      <div className="provider-form__model-cards">
        {entries.map((entry, index) => {
          const trimmedModel = entry.modelId.trim()
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
                    max={10_000}
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
                    max={10_000}
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
                    max={10_000}
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
                    max={10_000}
                    step={1}
                    value={entry.rpd}
                    onChange={(event) => onUpdateEntry(entry.key, { rpd: event.target.value })}
                    disabled={busy || !entry.enabled}
                  />
                </label>
              </div>

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
                  <label>
                    <input
                      type="checkbox"
                      checked={entry.quotaResetUtcMidnight}
                      onChange={(event) => onUpdateEntry(entry.key, { quotaResetUtcMidnight: event.target.checked })}
                      disabled={busy || !entry.enabled || !entry.dailyTokenQuota}
                    />
                    {translate('Сброс в полночь UTC')}
                  </label>
                </>
              ) : null}

              {trimmedModel !== '*' ? <div className="provider-form__grid">
                <label htmlFor={priceInputId}>
                  {translate('Вход ($ / 1M)')}
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
                  {translate('Кэш-вход ($ / 1M)')}
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
                  {translate('Выход ($ / 1M)')}
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
