import { translate } from './i18n'

export interface ModelLimitDraft {
  readonly key: string
  readonly modelId: string
  readonly enabled: boolean
  readonly rpm: string
  readonly rph: string
  readonly rpd: string
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
 * Renders per-model rate limit cards (RPM/RPH/RPD) and allows adding multiple models.
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
      <legend>{translate('Лимиты запросов модели')}</legend>
      <p className="provider-form__hint">
        {translate('Ноль означает без ограничения. Лимиты независимы для каждой модели выбранного провайдера; выключены по умолчанию.')}
      </p>
      <div className="provider-form__model-cards">
        {entries.map((entry, index) => {
          const trimmedModel = entry.modelId.trim()
          const hasCustomOption = trimmedModel.length > 0 && !catalogModels.includes(trimmedModel)
          const hasOptions = catalogModels.length > 0 || hasCustomOption
          const allowManualInput = catalogModels.length === 0 && index > 0
          const modelInputId = `provider-limit-model-${entry.key}`
          const rpmId = `provider-model-rpm-${entry.key}`
          const rphId = `provider-model-rph-${entry.key}`
          const rpdId = `provider-model-rpd-${entry.key}`

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
                {translate('Включить лимиты для этой модели')}
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
