/** Состояние установленного локального runtime Ollama. */
export type OllamaRuntimeState =
  | 'unknown'
  | 'missing'
  | 'installed'
  | 'ready'
  | 'installing'
  | 'failed'

export interface OllamaRuntimeStatus {
  readonly state: OllamaRuntimeState
  readonly version: string | null
  readonly executablePath: string | null
  readonly downloadedBytes: number
  readonly totalBytes: number | null
  readonly message: string
}

export function initialOllamaRuntimeStatus(): OllamaRuntimeStatus {
  return {
    state: 'unknown',
    version: null,
    executablePath: null,
    downloadedBytes: 0,
    totalBytes: null,
    message: 'Проверяем Ollama…'
  }
}
