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

interface PullProgress {
  readonly status: 'preparing' | 'downloading'
  readonly model: string
  readonly stage: string
  readonly completedBytes: number | null
  readonly totalBytes: number | null
  readonly percent: number | null
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
  const pullProgress = readPullProgress(managerProjection)

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
    } else if (pending !== null && Object.keys(managerProjection).length === 0) {
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
  const installedOnly = [...installed].filter((model) => !recommendations.some((recommendation) => recommendation.id === model))

  return (
    <section className="ollama-models" aria-label="Модели Ollama">
      <div className="ollama-models__heading">
        <div>
          <h3>Модели для Ollama</h3>
          <p className="shell__empty">Показаны модели для скачивания, которые помещаются в CPU, ОЗУ, VRAM и свободное место, а также все уже установленные модели.</p>
        </div>
        {catalog.device ? <span className="ollama-models__device">{formatDevice(catalog.device)}</span> : null}
      </div>
      {catalog.error ? <p className="shell__reason" role="status">{catalog.error} Запусти Ollama и обнови настройки.</p> : null}
      {pullProgress ? (
        <div className="ollama-models__progress" role="status" aria-live="polite">
          <div>
            <strong>Скачивание {pullProgress.model}</strong>
            <span>{pullProgress.stage || (pullProgress.status === 'downloading' ? 'Загрузка' : 'Подготовка')}</span>
          </div>
          {pullProgress.percent !== null ? (
            <>
              <progress max={100} value={pullProgress.percent} aria-label={`Скачивание ${pullProgress.model}`} />
              <span>{pullProgress.percent}% · {formatOptionalBytes(pullProgress.completedBytes)} из {formatOptionalBytes(pullProgress.totalBytes)}</span>
            </>
          ) : (
            <span>Подготовка модели…</span>
          )}
        </div>
      ) : null}
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
      ) : null}
      {installedOnly.length > 0 ? (
        <div className="ollama-models__installed" aria-label="Установленные модели Ollama">
          <strong>Установленные модели</strong>
          <div className="ollama-models__list">
            {installedOnly.map((model) => (
              <div className="ollama-models__item" key={model}>
                <div>
                  <strong>{model}</strong>
                  <span>установлена в Ollama · доступна в композиторе</span>
                </div>
                <span className="ollama-models__installed-badge">Установлена</span>
              </div>
            ))}
          </div>
        </div>
      ) : null}
      {recommendations.length === 0 && installedOnly.length === 0 ? (
        <p className="shell__empty">Нет моделей, которые безопасно помещаются на этом устройстве.</p>
      ) : null}
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

function readPullProgress(value: unknown): PullProgress | null {
  if (!value || typeof value !== 'object') return null
  const record = value as Record<string, unknown>
  const status = record.status
  const model = record.model
  if ((status !== 'preparing' && status !== 'downloading') || typeof model !== 'string' || !model) return null
  const completedBytes = asNonNegativeNumber(record.completed_bytes)
  const totalBytes = asNonNegativeNumber(record.total_bytes)
  const rawPercent = asNonNegativeNumber(record.percent)
  return {
    status,
    model,
    stage: typeof record.stage === 'string' ? record.stage : '',
    completedBytes,
    totalBytes,
    percent: rawPercent === null ? null : Math.min(100, Math.round(rawPercent))
  }
}

function asNonNegativeNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null
}

function formatDevice(device: DeviceProfile): string {
  const vram = device.accelerator_bytes && device.accelerator_bytes > 0
    ? ` · ${formatBytes(device.accelerator_bytes)} VRAM`
    : ' · VRAM не обнаружена'
  return `${device.cpu_threads} CPU · ${formatBytes(device.ram_bytes)} ОЗУ${vram} · ${formatBytes(device.disk_free_bytes)} свободно`
}

function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) return '—'
  const units = ['Б', 'КБ', 'МБ', 'ГБ', 'ТБ']
  let amount = value
  let unit = 0
  while (amount >= 1024 && unit < units.length - 1) {
    amount /= 1024
    unit += 1
  }
  return `${amount >= 10 || unit === 0 ? Math.round(amount) : amount.toFixed(1)} ${units[unit]}`
}

function formatOptionalBytes(value: number | null): string {
  return value === null ? 'размер уточняется' : formatBytes(value)
}
