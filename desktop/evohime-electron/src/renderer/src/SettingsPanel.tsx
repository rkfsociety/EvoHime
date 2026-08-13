import { useCallback, useEffect, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'
import { ProviderForm } from './ProviderForm'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

interface SettingsPanelProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

export function SettingsPanel({ connection, events }: SettingsPanelProps): React.JSX.Element {
  const api = useShellApi()
  const connected = CONNECTED_STATES.includes(connection)
  const [catalogMode, setCatalogMode] = useState<'free' | 'paid'>('free')
  const [config, setConfig] = useState<Record<string, unknown> | null>(null)
  const [models, setModels] = useState<string[]>([])
  const [catalogError, setCatalogError] = useState<string | null>(null)

  useEffect(() => {
    if (!api || !connected) return
    void api.invoke('core.getModelConfig', {})
    void api.invoke('core.listModelCatalog', { mode: catalogMode })
  }, [api, catalogMode, connected])

  useEffect(() => {
    const configEvent = latestEvent(events, 'model.config')
    if (configEvent) {
      const parsed = parseJson(configEvent.payload)
      setConfig(parsed)
    }
    const catalogEvent = latestEvent(events, 'model.catalog')
    if (catalogEvent) {
      const parsed = parseJson(catalogEvent.payload)
      setModels(Array.isArray(parsed.models) ? parsed.models.filter((model): model is string => typeof model === 'string') : [])
      setCatalogError(typeof parsed.error === 'string' ? parsed.error : null)
    }
  }, [events])

  const refreshCatalog = useCallback((mode: 'free' | 'paid') => {
    setCatalogMode(mode)
  }, [])

  return (
    <>
    <ProviderForm />

    <section className="shell__panel settings-panel" aria-label="Настройки и provider references">
      <div className="settings-panel__heading">
        <div>
          <h2>Настройки и providers</h2>
          <p className="shell__empty">Показываются только references и наличие конфигурации, без secret values.</p>
        </div>
        <span className={`settings-panel__state settings-panel__state--${connected ? 'ready' : 'offline'}`}>
          {connected ? 'Core подключён' : 'Core недоступен'}
        </span>
      </div>

      <dl className="settings-panel__config">
        <dt>Provider</dt><dd>{stringValue(config, 'provider')}</dd>
        <dt>Route</dt><dd>{stringValue(config, 'route')}</dd>
        <dt>Model</dt><dd>{stringValue(config, 'model')}</dd>
        <dt>Credentials</dt><dd>{config?.configured === true ? 'настроены' : 'не настроены'}</dd>
      </dl>

      <div className="settings-panel__catalog">
        <div className="settings-panel__catalog-heading">
          <h3>Каталог моделей: {catalogMode === 'free' ? 'free' : 'paid'}</h3>
          <div>
            <button type="button" onClick={() => refreshCatalog('free')} disabled={!connected || catalogMode === 'free'}>Free</button>
            <button type="button" onClick={() => refreshCatalog('paid')} disabled={!connected || catalogMode === 'paid'}>Paid</button>
          </div>
        </div>
        {catalogError ? <p role="alert" className="shell__reason">{catalogError}</p> : null}
        {models.length > 0 ? <ul>{models.map((model) => <li key={model}>{model}</li>)}</ul> : <p className="shell__empty">Модели не получены.</p>}
      </div>
    </section>
    </>
  )
}

function latestEvent(events: readonly CoreEvent[], eventType: string): CoreEvent | null {
  return [...events].reverse().find((event) => event.eventType === eventType) ?? null
}

function parseJson(payload: string): Record<string, unknown> {
  try {
    const value: unknown = JSON.parse(payload)
    return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
  } catch {
    return {}
  }
}

function stringValue(value: Record<string, unknown> | null, key: string): string {
  return typeof value?.[key] === 'string' && value[key] ? String(value[key]) : '—'
}
