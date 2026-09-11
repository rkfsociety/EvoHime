import { useEffect, useMemo, useState } from 'react'

import {
  OLLAMA_DEFAULT_BASE_URL,
  type ConnectionState,
  type CoreEvent
} from '@shared/api'

import { useShellApi } from './shell-api'

interface DeviceProfile {
  readonly cpu_threads: number
  readonly ram_bytes: number
  readonly disk_free_bytes: number
  readonly accelerator_bytes?: number | null
}

interface Recommendation {
  readonly id: string
  readonly description: string
  readonly size_bytes: number
  readonly required_ram_bytes: number
  readonly fits_device: boolean
  readonly installed: boolean
  readonly reason: string
}

interface CatalogPayload {
  readonly device?: DeviceProfile
  readonly recommendations?: readonly Recommendation[]
  readonly installed?: readonly string[]
  readonly error?: string | null
}

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

export interface OllamaModelDownloadPanelProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly baseUrl: string
}

export function OllamaModelDownloadPanel({ connection, events, baseUrl }: OllamaModelDownloadPanelProps): React.JSX.Element {
  const api = useShellApi()
  const [message, setMessage] = useState('')
  const [pending, setPending] = useState<string | null>(null)

  const catalogEvent = useMemo(() => events.find((event) => event.eventType === 'model.catalog') ?? null, [events])
  const catalog = useMemo(() => parseCatalog(catalogEvent?.payload), [catalogEvent])
  const managerEvent = useMemo(
    () => events.find((event) => event.localModelRuntimeManager?.operation === 'ollama_pull') ?? null,
    [events]
  )
  const managerProjection = managerEvent?.localModelRuntimeManager?.projection

  useEffect(() => {
    if (!api || !CONNECTED_STATES.includes(connection)) return
    void api.invoke('core.listModelCatalog', { mode: 'free' })
  }, [api, connection])

  useEffect(() => {
    if (!managerEvent || !managerProjection || typeof managerProjection !== 'object') return
    const projection = managerProjection as { status?: unknown; model?: unknown }
    if (projection.status === 'pulled') {
      setPending(null)
      setMessage(`Модель ${typeof projection.model === 'string' ? projection.model : ''} скачана.`)
      if (api && CONNECTED_STATES.includes(connection)) void api.invoke('core.listModelCatalog', { mode: 'free' })
    } else if (pending !== null && managerProjection && Object.keys(managerProjection).length === 0) {
      setPending(null)
      setMessage('Не удалось скачать модель. Проверь, что Ollama запущена.')
    }
  }, [api, connection, managerEvent, managerProjection, pending])

  const download = async (model: Recommendation): Promise<void> => {
    if (!api || !CONNECTED_STATES.includes(connection)) {
      setMessage('Нет подключения к Core.')
      return
    }
    setPending(model.id)
    setMessage(`Скачивание ${model.id} запущено…`)
    const outcome = await api.invoke('core.localModelRuntimeManager', {
      operation: 'ollama_pull',
      payload: JSON.stringify({ base_url: baseUrl.trim() || OLLAMA_DEFAULT_BASE_URL, model_id: model.id }),
      idempotencyKey: crypto.randomUUID()
    })
    if (!outcome.ok) {
      setPending(null)
      setMessage(outcome.message)
    }
  }

  const recommendations = catalog.recommendations ?? []
  const installed = new Set(catalog.installed ?? [])

  return (
    <section className="ollama-models" aria-label="Модели Ollama">
      <div className="ollama-models__heading">
        <div>
          <h3>Модели для Ollama</h3>
          <p className="shell__empty">Список рассчитан по потокам CPU, ОЗУ и свободному месту этого устройства.</p>
        </div>
        {catalog.device ? <span className="ollama-models__device">{formatDevice(catalog.device)}</span> : null}
      </div>
      {catalog.error ? <p className="shell__reason" role="status">{catalog.error} Запусти Ollama и обнови настройки.</p> : null}
      {recommendations.length > 0 ? (
        <div className="ollama-models__list">
          {recommendations.map((model) => {
            const isInstalled = model.installed || installed.has(model.id)
            return (
              <div className={`ollama-models__item${model.fits_device ? '' : ' ollama-models__item--limited'}`} key={model.id}>
                <div>
                  <strong>{model.id}</strong>
                  <span>{model.description} · {formatBytes(model.size_bytes)} · {model.reason}</span>
                </div>
                <button
                  type="button"
                  disabled={isInstalled || !model.fits_device || pending !== null}
                  onClick={() => void download(model)}
                >
                  {isInstalled ? 'Установлена' : pending === model.id ? 'Скачивается…' : 'Скачать'}
                </button>
              </div>
            )
          })}
        </div>
      ) : (
        <p className="shell__empty">Рекомендации пока не получены.</p>
      )}
      {message ? <p className="ollama-models__message" role="status">{message}</p> : null}
    </section>
  )
}

function parseCatalog(payload: string | undefined): CatalogPayload {
  if (!payload) return {}
  try {
    const value = JSON.parse(payload) as { ollama?: CatalogPayload }
    return value.ollama ?? {}
  } catch {
    return {}
  }
}

function formatDevice(device: DeviceProfile): string {
  const vram = device.accelerator_bytes && device.accelerator_bytes > 0
    ? ` · ${formatBytes(device.accelerator_bytes)} VRAM`
    : ' · VRAM не обнаружена'
  return `${device.cpu_threads} CPU · ${formatBytes(device.ram_bytes)} ОЗУ${vram} · ${formatBytes(device.disk_free_bytes)} свободно`
}

function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) return '—'
  const units = ['Б', 'МБ', 'ГБ', 'ТБ']
  let amount = value
  let unit = 0
  while (amount >= 1024 && unit < units.length - 1) {
    amount /= 1024
    unit += 1
  }
  return `${amount >= 10 || unit === 0 ? Math.round(amount) : amount.toFixed(1)} ${units[unit]}`
}
