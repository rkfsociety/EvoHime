import type { ProviderProfileId } from '@shared/api'

/** User-facing names for fixed vendor profiles shared by settings and the composer. */
export const PROVIDER_PROFILE_LABELS: Record<ProviderProfileId, string> = {
  openai: 'OpenAI',
  openrouter: 'OpenRouter',
  groq: 'Groq',
  gemini: 'Google Gemini',
  mistral: 'Mistral',
  cloudflare_workers_ai: 'Cloudflare Workers AI',
  nvidia_nim: 'NVIDIA NIM',
  cerebras: 'Cerebras',
  hugging_face: 'Hugging Face Inference Providers',
  deepseek: 'DeepSeek',
  xai: 'xAI (Grok)',
  together_ai: 'Together AI',
  fireworks_ai: 'Fireworks AI',
  moonshot: 'Moonshot (Kimi)',
  zai: 'Z.AI (GLM)',
  sambanova: 'SambaNova',
  nebius: 'Nebius Token Factory',
  custom: 'Другой OpenAI-compatible API'
}
