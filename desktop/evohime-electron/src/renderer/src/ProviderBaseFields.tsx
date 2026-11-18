import { translate } from './i18n'
import type { ProviderUISchema } from './provider-schema'

export interface ProviderChoiceOption<T extends string = string> {
  readonly value: T
  readonly label: string
}

export interface ProviderBaseFieldsProps<T extends string = string> {
  readonly schema: ProviderUISchema
  readonly selection: T
  readonly choices: readonly ProviderChoiceOption<T>[]
  readonly onSelectChoice: (choice: T) => void
  readonly apiKey: string
  readonly onApiKeyChange: (value: string) => void
  readonly displayBaseUrl: string
  readonly onBaseUrlChange: (value: string) => void
  readonly configured: boolean
  readonly busy: boolean
}

/**
 * Renders the core provider credentials and endpoint fields driven by `ProviderUISchema`.
 */
export function ProviderBaseFields<T extends string>({
  schema,
  selection,
  choices,
  onSelectChoice,
  apiKey,
  onApiKeyChange,
  displayBaseUrl,
  onBaseUrlChange,
  configured,
  busy
}: ProviderBaseFieldsProps<T>): React.JSX.Element {
  return (
    <div className="provider-form__grid">
      <label htmlFor="provider-kind">
        {translate('Провайдер')}
        <select
          id="provider-kind"
          value={selection}
          onChange={(event) => onSelectChoice(event.target.value as T)}
          disabled={busy}
        >
          {choices.map((choice) => (
            <option key={choice.value} value={choice.value}>
              {translate(choice.label)}
            </option>
          ))}
        </select>
      </label>

      {schema.requiresApiKey ? (
        <label className="provider-form__key" htmlFor="provider-key">
          {translate('Ключ API')}
          <input
            id="provider-key"
            type="password"
            value={apiKey}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => onApiKeyChange(event.target.value)}
            placeholder={configured ? translate('сохранён — введи новый, чтобы заменить') : 'sk-…'}
            disabled={busy}
          />
        </label>
      ) : null}

      <label htmlFor="provider-url">
        {translate('Адрес API')}
        <input
          id="provider-url"
          value={displayBaseUrl}
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => onBaseUrlChange(event.target.value)}
          placeholder={translate('по умолчанию провайдера')}
          disabled={busy || !schema.editableBaseUrl}
        />
      </label>
    </div>
  )
}
