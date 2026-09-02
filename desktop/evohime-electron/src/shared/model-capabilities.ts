import type { ChatProviderMode } from './api'

export type ModelUse = 'agent' | 'text'

export interface ModelCapability {
  readonly agent: boolean
  readonly text: boolean
  readonly rank: number
  readonly reason: string
}

/**
 * UI policy for model use-cases. The provider remains the source of model
 * identifiers; this layer only answers whether a returned model is suitable
 * for a use-case. Unknown models are text-only until a provider advertises
 * verified capabilities, which keeps new local/cloud providers safe by
 * default without making them disappear from review.
 */
export function capabilityForModel(provider: ChatProviderMode, model: string): ModelCapability {
  const normalized = model.trim().toLowerCase()
  if (provider === 'codex_cli') {
    return { agent: true, text: true, rank: 100, reason: 'Codex сам управляет инструментами' }
  }

  if (provider === 'literouter') {
    if (normalized === 'claude-haiku-4.5-cheap:free') {
      return { agent: true, text: true, rank: 100, reason: 'проверена для работы с инструментами' }
    }
    if (/^(?:gpt-oss-(?:20b|120b)|llama-3(?:\.1)?-8b-instruct|llama-3\.3-70b-instruct-turbo|l3-8b-lunaris|gemma-3-27b-it|ministral-3b-2512|mythomax-l2-13b):free$/.test(normalized)) {
      return { agent: false, text: true, rank: 10, reason: 'не прошла проверку агентских инструментов' }
    }
    return { agent: false, text: true, rank: 50, reason: 'возможности инструментов ещё не подтверждены' }
  }

  return { agent: false, text: true, rank: 50, reason: 'провайдер ещё не объявил capabilities' }
}

export function sortModelsForUse(provider: ChatProviderMode, models: readonly string[], use: ModelUse): string[] {
  return models
    .filter((model) => capabilityForModel(provider, model)[use])
    .sort((left, right) => {
      const rank = capabilityForModel(provider, right).rank - capabilityForModel(provider, left).rank
      return rank || left.localeCompare(right)
    })
}
