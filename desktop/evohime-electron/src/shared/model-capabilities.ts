export type ModelUse = 'agent' | 'text'

export type CoreCapabilityState = 'supported' | 'unsupported' | 'unknown'

export interface CoreModelCapability {
  readonly capability: string
  readonly state: CoreCapabilityState
  readonly provenance: string
}

/** Safe model metadata projected by Core inside the authenticated catalog event. */
export interface CoreModelDescriptor {
  readonly id: string
  readonly contextTokens: number | null
  readonly maxOutputTokens: number | null
  readonly capabilities: readonly CoreModelCapability[]
  readonly privacy: string
  readonly lifecycle: string
}

/**
 * Parses only the bounded Core descriptor. Provider responses and model-name
 * policy never enter this helper, so new models remain visible until Core
 * supplies an explicit capability state.
 */
export function parseCoreModelDescriptor(value: unknown): CoreModelDescriptor | null {
  const record = asRecord(value)
  const id = stringField(record?.['id'])
  if (id === null) return null

  const limits = asRecord(record?.['limits'])
  const capabilities = Array.isArray(record?.['capabilities'])
    ? record['capabilities'].flatMap(parseCoreCapability)
    : []
  return {
    id,
    contextTokens: numberField(limits?.['context_tokens']),
    maxOutputTokens: numberField(limits?.['max_output_tokens']),
    capabilities,
    privacy: stringField(record?.['privacy']) ?? 'unknown',
    lifecycle: stringField(record?.['lifecycle']) ?? 'unknown'
  }
}

export function modelMetadataHint(model: CoreModelDescriptor | undefined, use: ModelUse): string {
  const requiredCapability = use === 'agent' ? 'tool_calls' : 'chat'
  if (!model) return `Core: ${requiredCapability} не подтверждён Core; лимиты и privacy неизвестны`

  const capability = model.capabilities.find((item) => item.capability === requiredCapability)
  const capabilityLabel = capability?.state === 'supported'
    ? `${requiredCapability} подтверждён Core`
    : capability?.state === 'unsupported'
      ? `${requiredCapability} не поддерживается`
      : `${requiredCapability} не подтверждён Core`
  const limits = [
    model.contextTokens === null ? null : `контекст ${formatTokens(model.contextTokens)}`,
    model.maxOutputTokens === null ? null : `вывод ${formatTokens(model.maxOutputTokens)}`
  ].filter((value): value is string => value !== null)
  const privacy = model.privacy === 'local_only'
    ? 'данные локально'
    : model.privacy === 'provider_retained'
      ? 'провайдер может хранить данные'
      : model.privacy === 'provider_controlled'
        ? 'хранение по политике провайдера'
        : 'privacy неизвестен'
  return [`Core: ${capabilityLabel}`, ...limits, privacy].join(' · ')
}

function parseCoreCapability(value: unknown): CoreModelCapability[] {
  const record = asRecord(value)
  const capability = stringField(record?.['capability'])
  if (capability === null) return []
  const state = record?.['state']
  return [{
    capability,
    state: state === 'supported' || state === 'unsupported' ? state : 'unknown',
    provenance: stringField(record?.['provenance']) ?? 'unknown'
  }]
}

function formatTokens(value: number): string {
  return value >= 1_000_000
    ? `${Math.round(value / 100_000) / 10}M`
    : value >= 1_000
      ? `${Math.round(value / 100) / 10}k`
      : String(value)
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null ? value as Record<string, unknown> : null
}

function stringField(value: unknown): string | null {
  return typeof value === 'string' && value.trim().length > 0 ? value.trim() : null
}

/**
 * Measured 100-point integer ratings (0..100) from EvoHime CLI benchmark runs
 * evaluating tool-calling accuracy, factual precision, and response latency.
 */
const MODEL_BENCHMARK_RATINGS: Readonly<Record<string, number>> = {
  'glm-5.3-cheap:free': 98,
  'glm-5.1-cheap:free': 98,
  'minimax-m3.1-flash:free': 98,
  'deepseek-v4-pro-stable:free': 98,
  'deepseek-v4-pro-cheap:free': 98,
  'qwen3.8-flash:free': 98,
  'glm-4.7-flash-heretic:free': 98,
  'qwen3-30b-a3b:free': 98,
  'glm-5.2-cheap:free': 98,
  'minimax-m2.7:free': 98,
  'qwen3.8-flash-cheap:free': 98,
  'glm-5.3-flash-cheap:free': 98,
  'deepseek-v3.1:free': 95,
  'minimax-m2.1:free': 95,
  'deepseek-v3.2:free': 95,
  'qwen3-32b:free': 95,
  'deepseek-v3.1-stable:free': 95,
  'qwen3-coder-next:free': 95,
  'qwen-3.8-27b:free': 92,
  'minimax-m2:free': 92,
  'minimax-m2.5:free': 92,
  'deepseek-v4-flash-0731:free': 92,
  'qwen3.6-35b-a3b:free': 92,
  'glm-5.3-flash:free': 92,
  'deepseek-v4-flash-0731-cheap:free': 92,
  'qwen3-next-80b-a3b-instruct:free': 92,
  'openai-gpt-oss-safeguard-20b:free': 87,
  'qwen3-coder-30b-a3b-instruct:free': 86,
  'qwen3-vl-235b-a22b-instruct:free': 76,
  'deepseek-v4-flash-0731-stable:free': 0,
  'deepseek-v4.1-flash:free': 0,
  'kimi-k2.5-cheap:free': 0,
  'ministral-3b-2512:free': 0,
  'mythomax-l2-13b:free': 0,
  'deepseek-v4-flash-cheap:free': 0,
  'deepseek-v4-flash:free': 0,
  'deepseek-v4.1-flash-1017:free': 0,
  'deepseek-v4.1-flash-cheap:free': 0,
  'gpt-oss-120b:free': 0,
  'llama-3.3-70b-instruct-turbo:free': 0,
  'magistral-small:free': 0,
  'ministral-3-14b-instruct:free': 0,
  'ministral-3-8b-instruct:free': 0
}

/** Returns integer 0..100 benchmark rating for a model, or null if unrated. */
export function modelBenchmarkRating(modelId: string): number | null {
  const value = MODEL_BENCHMARK_RATINGS[modelId]
  return typeof value === 'number' ? Math.round(value) : null
}

/** Sorts models by descending benchmark rating first, then alphabetically. */
export function compareModelsByRating(left: string, right: string): number {
  const leftRating = modelBenchmarkRating(left)
  const rightRating = modelBenchmarkRating(right)
  if (leftRating !== null && rightRating !== null && leftRating !== rightRating) {
    return rightRating - leftRating
  }
  if (leftRating !== null && rightRating === null) return -1
  if (leftRating === null && rightRating !== null) return 1
  return left.localeCompare(right)
}

function numberField(value: unknown): number | null {
  return typeof value === 'number' && Number.isSafeInteger(value) && value > 0 ? value : null
}
