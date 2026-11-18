import {
  REQUEST_PACING_MAX_INTERVAL_MS,
  type ConnectionState,
  type CoreEvent,
  type FreeAccessProbePolicy,
  type FreeAccessRoutingMode,
  type ModelTier
} from '@shared/api'

import { OllamaModelDownloadPanel } from './OllamaModelDownloadPanel'
import { translate } from './i18n'
import type { ProviderUISchema } from './provider-schema'

const TIERS: readonly { readonly id: ModelTier; readonly label: string; readonly hint: string }[] = [
  { id: 'free', label: 'Бесплатные', hint: 'модели с суффиксом :free' },
  { id: 'paid', label: 'Платные', hint: 'списываются с баланса провайдера' }
]

export interface ProviderSpecificSettingsProps {
  readonly schema: ProviderUISchema
  readonly busy: boolean
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly baseUrl: string
  readonly accountId: string
  readonly onAccountIdChange: (value: string) => void
  readonly tier: ModelTier
  readonly onTierChange: (tier: ModelTier) => void
  readonly probePolicy: FreeAccessProbePolicy
  readonly onProbePolicyChange: (policy: FreeAccessProbePolicy) => void
  readonly acknowledgeProbePossibleCost: boolean
  readonly onAcknowledgeProbePossibleCostChange: (value: boolean) => void
  readonly routingMode: FreeAccessRoutingMode
  readonly onRoutingModeChange: (mode: FreeAccessRoutingMode) => void
  readonly allowPaidFallback: boolean
  readonly onAllowPaidFallbackChange: (value: boolean) => void
  readonly pacingEnabled: boolean
  readonly onPacingEnabledChange: (value: boolean) => void
  readonly pacingInterval: string
  readonly onPacingIntervalChange: (value: string) => void
  readonly pacingFreeInterval: string
  readonly onPacingFreeIntervalChange: (value: string) => void
  readonly pacingIsDefault: boolean
}

/**
 * Renders provider-specific configuration sections driven by `ProviderUISchema`.
 */
export function ProviderSpecificSettings({
  schema,
  busy,
  connection,
  events,
  baseUrl,
  accountId,
  onAccountIdChange,
  tier,
  onTierChange,
  probePolicy,
  onProbePolicyChange,
  acknowledgeProbePossibleCost,
  onAcknowledgeProbePossibleCostChange,
  routingMode,
  onRoutingModeChange,
  allowPaidFallback,
  onAllowPaidFallbackChange,
  pacingEnabled,
  onPacingEnabledChange,
  pacingInterval,
  onPacingIntervalChange,
  pacingFreeInterval,
  onPacingFreeIntervalChange,
  pacingIsDefault
}: ProviderSpecificSettingsProps): React.JSX.Element {
  const showProbe = schema.supportsFreeAccessProbe || probePolicy !== 'disabled'
  const showRouting = schema.supportsEvidenceRouting || routingMode !== 'any'
  const showPacing = schema.supportsRequestPacing || !pacingIsDefault

  return (
    <>
      {schema.requiresAccountId ? (
        <div className="provider-form__grid">
          <label htmlFor="provider-account-id">
            {schema.accountIdLabel ?? translate('Account ID')}
            <input
              id="provider-account-id"
              value={accountId}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => onAccountIdChange(event.target.value)}
              placeholder={schema.accountIdPlaceholder ? translate(schema.accountIdPlaceholder) : ''}
              disabled={busy}
            />
          </label>
        </div>
      ) : null}

      {schema.requiresAccountId && schema.accountIdHint ? (
        <p className="shell__empty">{translate(schema.accountIdHint)}</p>
      ) : null}

      {schema.supportsLocalModelDownload ? (
        <OllamaModelDownloadPanel connection={connection} events={events} baseUrl={baseUrl} />
      ) : null}

      {schema.showTierSelector ? (
        <fieldset className="provider-form__tier">
          <legend>{translate('Какие модели показывать')}</legend>
          {TIERS.map((item) => (
            <label key={item.id}>
              <input
                type="radio"
                name="model-tier"
                value={item.id}
                checked={tier === item.id}
                onChange={() => onTierChange(item.id)}
                disabled={busy}
              />
              <span>{translate(item.label)}</span>
              <span className="provider-form__hint">{translate(item.hint)}</span>
            </label>
          ))}
        </fieldset>
      ) : null}

      {showProbe ? (
        <fieldset className="provider-form__tier">
          <legend>{translate('Проверка бесплатного доступа')}</legend>
          <label htmlFor="free-access-probe-policy">
            {translate('Как собирать данные')}
            <select
              id="free-access-probe-policy"
              value={probePolicy}
              onChange={(event) => onProbePolicyChange(event.target.value as FreeAccessProbePolicy)}
              disabled={busy}
            >
              <option value="disabled">{translate('Выключена (ручную проверку можно запустить отдельно)')}</option>
              <option value="manual_only">{translate('Только ручная проверка')}</option>
              <option value="passive_only">{translate('Только наблюдать обычные запросы')}</option>
              <option value="on_first_use" disabled={!schema.supportsAutoProbe}>
                {translate('Авто (проверить при первом использовании)')}
              </option>
              <option value="periodic_bounded" disabled={!schema.supportsAutoProbe}>
                {translate('Повторять после истечения evidence')}
              </option>
            </select>
          </label>
          {probePolicy === 'on_first_use' || probePolicy === 'periodic_bounded' ? (
            <>
              <p className="provider-form__hint">
                {translate('Автоматическая проверка отправляет короткий synthetic запрос через OpenRouter. Возможен расход кредитов или денег.')}
              </p>
              <label>
                <input
                  type="checkbox"
                  checked={acknowledgeProbePossibleCost}
                  onChange={(event) => onAcknowledgeProbePossibleCostChange(event.target.checked)}
                  disabled={busy}
                />
                {translate('Я отдельно разрешаю автоматическую проверку с возможным расходом для этого профиля и ключа')}
              </label>
            </>
          ) : null}
        </fieldset>
      ) : null}

      {showRouting ? (
        <fieldset className="provider-form__tier">
          <legend>{translate('Маршрутизация по evidence')}</legend>
          <label htmlFor="free-access-routing-mode">
            {translate('Режим')}
            <select
              id="free-access-routing-mode"
              value={routingMode}
              onChange={(event) => onRoutingModeChange(event.target.value as FreeAccessRoutingMode)}
              disabled={busy}
            >
              <option value="any">{translate('Обычная маршрутизация')}</option>
              <option value="prefer_free">{translate('Сначала подтверждённые бесплатные модели')}</option>
              <option value="free_only">{translate('Только подтверждённые бесплатные модели')}</option>
            </select>
          </label>
          <p className="provider-form__hint">
            {translate('Каталожная метка и суффикс :free не подтверждают доступ. FreeOnly блокирует неизвестные, устаревшие и платные маршруты.')}
          </p>
          {routingMode === 'prefer_free' ? (
            <label>
              <input
                type="checkbox"
                checked={allowPaidFallback}
                onChange={(event) => onAllowPaidFallbackChange(event.target.checked)}
                disabled={busy}
              />
              {translate('Разрешить платный fallback, если подтверждённая бесплатная модель недоступна')}
            </label>
          ) : null}
        </fieldset>
      ) : null}

      {showPacing ? (
        <fieldset className="provider-form__tier">
          <legend>{translate('Пауза между запросами')}</legend>
          <label>
            <input
              type="checkbox"
              checked={pacingEnabled}
              onChange={(event) => onPacingEnabledChange(event.target.checked)}
              disabled={busy}
            />
            {translate('Выдерживать минимальную паузу между запросами к этому провайдеру')}
          </label>
          {pacingEnabled ? (
            <>
              <label htmlFor="provider-pacing-interval">
                {translate('Интервал для платных моделей, мс')}
                <input
                  id="provider-pacing-interval"
                  type="number"
                  min={0}
                  max={REQUEST_PACING_MAX_INTERVAL_MS}
                  step={1}
                  value={pacingInterval}
                  onChange={(event) => onPacingIntervalChange(event.target.value)}
                  disabled={busy}
                />
              </label>
              <label htmlFor="provider-pacing-free-interval">
                {translate('Интервал для моделей :free, мс')}
                <input
                  id="provider-pacing-free-interval"
                  type="number"
                  min={0}
                  max={REQUEST_PACING_MAX_INTERVAL_MS}
                  step={1}
                  value={pacingFreeInterval}
                  onChange={(event) => onPacingFreeIntervalChange(event.target.value)}
                  disabled={busy}
                />
              </label>
            </>
          ) : null}
          {schema.pacingHint ? (
            <p className="provider-form__hint">{translate(schema.pacingHint)}</p>
          ) : null}
        </fieldset>
      ) : null}
    </>
  )
}
