import {
  PROVIDER_PROFILE_IDS,
  type FreeAccessProbePolicy,
  type ModelTier,
  type ProviderKind,
  type ProviderProfileId
} from '@shared/api'

/**
 * Declarative UI schema for a provider transport or vendor profile.
 * Drives field visibility, editability, and provider-specific sections
 * without hardcoding `provider === ...` branches in form components.
 */
export interface ProviderUISchema {
  readonly requiresApiKey: boolean
  readonly requiresBaseUrl: boolean
  readonly editableBaseUrl: boolean
  readonly requiresAccountId: boolean
  readonly showTierSelector: boolean
  readonly supportsRequestPacing: boolean
  readonly supportsFreeAccessProbe: boolean
  readonly supportsModelLimits: boolean
  readonly supportsEvidenceRouting: boolean
  readonly supportsAutoProbe: boolean
  readonly supportsLocalModelDownload: boolean
  readonly defaultTier: ModelTier
  readonly defaultProbePolicy: FreeAccessProbePolicy
  readonly description: string
  readonly unconfiguredBadge?: string
  readonly configuredBadge?: string
  readonly saveButtonLabel: string
  readonly accountIdLabel?: string
  readonly accountIdPlaceholder?: string
  readonly accountIdHint?: string
  readonly pacingHint?: string
}

const DEFAULT_API_KEY_DESCRIPTION =
  'Ключ шифруется средствами Windows и хранится локально. Модель выбирается в чате.'
const DEFAULT_SAVE_BUTTON_LABEL = 'Сохранить ключ и применить'

const PAID_OPENAI_COMPATIBLE_PROFILES: ReadonlySet<ProviderProfileId> = new Set([
  'openai',
  'deepseek',
  'xai',
  'together_ai',
  'fireworks_ai',
  'moonshot',
  'zai',
  'nebius',
  'anymodel',
  'custom'
])

function buildOpenAICompatibleProfileSchema(profileId: ProviderProfileId): ProviderUISchema {
  const isCustom = profileId === 'custom'
  const isCloudflare = profileId === 'cloudflare_workers_ai'
  const isOpenRouter = profileId === 'openrouter'
  const defaultTier: ModelTier = PAID_OPENAI_COMPATIBLE_PROFILES.has(profileId) ? 'paid' : 'free'

  return {
    requiresApiKey: true,
    requiresBaseUrl: isCustom,
    editableBaseUrl: isCustom,
    requiresAccountId: isCloudflare,
    showTierSelector: false,
    supportsRequestPacing: false,
    supportsFreeAccessProbe: isOpenRouter || isCloudflare,
    supportsModelLimits: true,
    supportsEvidenceRouting: isOpenRouter,
    supportsAutoProbe: isOpenRouter,
    supportsLocalModelDownload: false,
    defaultTier,
    defaultProbePolicy: isOpenRouter ? 'on_first_use' : 'disabled',
    description: DEFAULT_API_KEY_DESCRIPTION,
    saveButtonLabel: DEFAULT_SAVE_BUTTON_LABEL,
    ...(isCloudflare
      ? {
          accountIdLabel: 'Cloudflare Account ID',
          accountIdPlaceholder: '32 шестнадцатеричных символа',
          accountIdHint:
            'Нужен API token Cloudflare с разрешениями Workers AI Read и Workers AI Edit. Host и API path задаются профилем.'
        }
      : {})
  }
}

const OPENAI_COMPATIBLE_SCHEMAS: Record<ProviderProfileId, ProviderUISchema> = Object.fromEntries(
  PROVIDER_PROFILE_IDS.map((profileId) => [profileId, buildOpenAICompatibleProfileSchema(profileId)])
) as Record<ProviderProfileId, ProviderUISchema>

const LITEROUTER_SCHEMA: ProviderUISchema = {
  requiresApiKey: true,
  requiresBaseUrl: false,
  editableBaseUrl: true,
  requiresAccountId: false,
  showTierSelector: true,
  supportsRequestPacing: false,
  supportsFreeAccessProbe: false,
  supportsModelLimits: true,
  supportsEvidenceRouting: false,
  supportsAutoProbe: false,
  supportsLocalModelDownload: false,
  defaultTier: 'free',
  defaultProbePolicy: 'disabled',
  description: DEFAULT_API_KEY_DESCRIPTION,
  saveButtonLabel: DEFAULT_SAVE_BUTTON_LABEL,
  pacingHint:
    'Пауза — дополнительное локальное ограничение EvoHime. Она выключена по умолчанию и не отражает лимиты аккаунта провайдера.'
}

export const PROVIDER_SCHEMAS: Record<string, ProviderUISchema> = {
  literouter: LITEROUTER_SCHEMA,
  ...OPENAI_COMPATIBLE_SCHEMAS,
  openai_compatible: OPENAI_COMPATIBLE_SCHEMAS.custom,
  openai_responses: {
    requiresApiKey: true,
    requiresBaseUrl: false,
    editableBaseUrl: true,
    requiresAccountId: false,
    showTierSelector: false,
    supportsRequestPacing: false,
    supportsFreeAccessProbe: false,
    supportsModelLimits: true,
    supportsEvidenceRouting: false,
    supportsAutoProbe: false,
    supportsLocalModelDownload: false,
    defaultTier: 'paid',
    defaultProbePolicy: 'disabled',
    description: DEFAULT_API_KEY_DESCRIPTION,
    saveButtonLabel: DEFAULT_SAVE_BUTTON_LABEL
  },
  ollama: {
    requiresApiKey: false,
    requiresBaseUrl: true,
    editableBaseUrl: true,
    requiresAccountId: false,
    showTierSelector: false,
    supportsRequestPacing: false,
    supportsFreeAccessProbe: false,
    supportsModelLimits: true,
    supportsEvidenceRouting: false,
    supportsAutoProbe: false,
    supportsLocalModelDownload: true,
    defaultTier: 'free',
    defaultProbePolicy: 'disabled',
    description: 'Ollama работает локально. Ключ не нужен, модель можно скачать ниже.',
    unconfiguredBadge: 'Локальный провайдер',
    configuredBadge: 'Локальный провайдер',
    saveButtonLabel: 'Сохранить параметры и применить'
  }
}

/** Resolves the UI schema for a transport provider and optional profile id. */
export function getProviderSchema(
  provider: ProviderKind,
  profileId?: ProviderProfileId
): ProviderUISchema {
  if (provider === 'openai_compatible') {
    const key = profileId ?? 'custom'
    return PROVIDER_SCHEMAS[key] ?? OPENAI_COMPATIBLE_SCHEMAS.custom
  }
  return PROVIDER_SCHEMAS[provider] ?? LITEROUTER_SCHEMA
}
