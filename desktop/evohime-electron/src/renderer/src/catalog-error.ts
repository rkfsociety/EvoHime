export type CatalogErrorCode =
  | 'provider_configuration_error'
  | 'provider_not_configured'
  | 'provider_timeout'
  | 'provider_transport_error'
  | 'catalog_model_not_found'
  | 'catalog_too_many_entries'
  | 'catalog_response_too_large'
  | 'catalog_response_invalid'
  | 'catalog_stream_error'
  | 'hardware_discovery_failed'
  | 'catalog_unavailable'

export function safeCatalogErrorCode(value: unknown): CatalogErrorCode | null {
  if (typeof value !== 'string' || value.trim().length === 0) return null
  switch (value) {
    case 'provider_configuration_error':
    case 'provider_not_configured':
    case 'provider_timeout':
    case 'provider_transport_error':
    case 'catalog_model_not_found':
    case 'catalog_too_many_entries':
    case 'catalog_response_too_large':
    case 'catalog_response_invalid':
    case 'catalog_stream_error':
    case 'hardware_discovery_failed':
    case 'catalog_unavailable':
      return value
    default:
      return 'catalog_unavailable'
  }
}

export function catalogErrorMessage(code: string, provider: 'ollama' | 'provider' = 'provider'): string {
  switch (code) {
    case 'provider_configuration_error':
      return provider === 'ollama' ? 'Проверь настройки Ollama.' : 'Проверь ключ провайдера в настройках.'
    case 'provider_not_configured':
      return 'Провайдер не настроен для получения каталога моделей.'
    case 'catalog_model_not_found':
      return 'Провайдер не нашёл каталог моделей по этому адресу.'
    case 'provider_timeout':
      return provider === 'ollama' ? 'Ollama не ответила вовремя.' : 'Провайдер не ответил вовремя.'
    case 'provider_transport_error':
      return provider === 'ollama' ? 'Не удалось подключиться к Ollama.' : 'Не удалось подключиться к провайдеру.'
    case 'hardware_discovery_failed':
      return 'Не удалось определить возможности устройства.'
    case 'catalog_too_many_entries':
    case 'catalog_response_too_large':
    case 'catalog_response_invalid':
    case 'catalog_stream_error':
      return provider === 'ollama'
        ? 'Ollama вернула некорректный каталог моделей.'
        : 'Провайдер вернул некорректный каталог моделей.'
    default:
      return 'Не удалось получить каталог моделей.'
  }
}
