import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import type { ChatProviderMode, ConnectionState, CoreEvent, CodexModel, CodexRateLimit, ModelTier } from '@shared/api'

import { useShellApi } from './shell-api'
import { CodexRateLimits } from './CodexRateLimits'
import { capabilityForModel, sortModelsForUse, type ModelUse } from '@shared/model-capabilities'

/**
 * Model selection for the next task, shown in the composer.
 *
 * The list is the provider's own catalogue, filtered by the tier chosen in
 * settings — the shell never hardcodes model names. Selecting one sends a
 * bounded command to Core, which resolves the model per request, so a change
 * applies to the next task without restarting anything.
 */

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

export interface ModelPickerProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly provider: ChatProviderMode
  readonly use?: ModelUse
}

export function ModelPicker({ connection, events, provider = 'literouter', use = 'agent' }: ModelPickerProps & { readonly provider?: ChatProviderMode }): React.JSX.Element | null {
  const api = useShellApi()
  const connected = CONNECTED_STATES.includes(connection)
  const [tier, setTier] = useState<ModelTier | null>(null)
  const [models, setModels] = useState<readonly string[]>([])
  const [current, setCurrent] = useState('')
  const [codexModels, setCodexModels] = useState<readonly CodexModel[]>([])
  const [codexRateLimits, setCodexRateLimits] = useState<readonly CodexRateLimit[]>([])
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!api || provider === 'codex_cli') return
    void api.invoke('provider.get', {}).then((outcome) => {
      if (outcome.ok) setTier(outcome.value.tier === 'paid' ? 'paid' : 'free')
    })
  }, [api, provider])

  useEffect(() => {
    if (!api || !connected || provider === 'codex_cli' || tier === null) return
    void api.invoke('core.listModelCatalog', { mode: tier })
    void api.invoke('core.getModelConfig', {})
  }, [api, connected, provider, tier])

  useEffect(() => {
    if (!api || !connected || provider !== 'codex_cli') return
    void api.invoke('codex.getStatus', {}).then((outcome) => {
      if (outcome.ok) {
        setCodexModels(outcome.value.models)
        setCodexRateLimits(outcome.value.rateLimits)
        setCurrent(outcome.value.selectedModel)
        setError(outcome.value.error)
      }
    })
  }, [api, connected, provider])

  const catalog = useMemo(() => latest(events, 'model.catalog'), [events])
  const config = useMemo(() => latest(events, 'model.config'), [events])

  useEffect(() => {
    if (provider === 'codex_cli') return
    if (!catalog) return
    const parsed = parseJson(catalog.payload)
    const catalogModels = Array.isArray(parsed['models'])
      ? parsed['models'].filter((model): model is string => typeof model === 'string' && model.trim().length > 0)
      : []
    setModels(sortModelsForUse(provider, catalogModels, use))
    setError(typeof parsed['error'] === 'string' ? parsed['error'] : null)
  }, [catalog, provider, use])

  useEffect(() => {
    if (provider === 'codex_cli') return
    if (!config) return
    const parsed = parseJson(config.payload)
    if (typeof parsed['model'] === 'string') setCurrent(parsed['model'])
  }, [config, provider])

  const select = useCallback(
    async (model: string) => {
      if (!api) return
      setCurrent(model)
      const outcome = provider === 'codex_cli'
        ? await api.invoke('codex.selectModel', { model })
        : await api.invoke('core.selectModel', { model })
      if (!outcome.ok) setError(outcome.message)
    },
    [api, provider]
  )

  // A dropdown whose value is not in its own list still renders the first
  // option, which would show one model while Core used another — the route
  // default, which need not even exist in this tier. Commit to what is shown.
  useEffect(() => {
    const available = provider === 'codex_cli' ? codexModels.map((model) => model.id) : models
    if (available.length === 0) return
    if (current !== '' && available.includes(current)) return
    const first = available[0]
    if (first !== undefined) void select(first)
  }, [codexModels, current, models, provider, select])

  if (!connected) {
    return null
  }

  if (error !== null) {
    // A catalogue failure is almost always a missing or rejected key; say so
    // where the user is, instead of leaving an empty dropdown.
    return (
      <span className="model-picker model-picker--error" role="status">
        Модели недоступны — проверь ключ в настройках
      </span>
    )
  }

  const visibleModels = provider === 'codex_cli'
    ? codexModels.map((model) => ({ value: model.id, label: model.displayName || model.id }))
    : models.map((model) => ({ value: model, label: model }))
  const known = visibleModels.some((model) => model.value === current)

  return (
    <>
      <ModelDropdown
        models={visibleModels}
        current={known ? current : ''}
        onSelect={(model) => void select(model)}
      />
      {provider !== 'codex_cli' && use === 'agent' && models.length > 0 ? (
        <span className="model-picker__hint" title={capabilityForModel(provider, current).reason}>агентские модели</span>
      ) : null}
      {provider === 'codex_cli' ? <CodexRateLimits rateLimits={codexRateLimits} compact /> : null}
    </>
  )
}

interface ModelDropdownProps {
  readonly models: readonly ModelOption[]
  readonly current: string
  readonly onSelect: (model: string) => void
}

interface ModelOption {
  readonly value: string
  readonly label: string
}

/**
 * Own dropdown instead of a native `select`.
 *
 * Windows draws a native option list with its own colours, which came out as
 * grey text on white inside the dark composer. This one is styled by the app,
 * and a catalogue of dozens of models needs a filter anyway.
 */
function ModelDropdown({ models, current, onSelect }: ModelDropdownProps): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const root = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (!open) return
    const onPointerDown = (event: MouseEvent): void => {
      if (!root.current?.contains(event.target as Node)) setOpen(false)
    }
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') setOpen(false)
    }
    document.addEventListener('mousedown', onPointerDown)
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('mousedown', onPointerDown)
      document.removeEventListener('keydown', onKeyDown)
    }
  }, [open])

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase()
    return needle.length === 0
      ? models
      : models.filter((model) => `${model.label} ${model.value}`.toLowerCase().includes(needle))
  }, [models, query])

  return (
    <div className="model-picker" ref={root}>
      <button
        type="button"
        className="model-picker__button"
        aria-label="Модель"
        aria-expanded={open}
        disabled={models.length === 0}
        onClick={() => {
          setQuery('')
          setOpen((value) => !value)
        }}
      >
        <span className="model-picker__value">{current || 'загрузка моделей…'}</span>
        <span className="model-picker__chevron" aria-hidden="true">▾</span>
      </button>

      {open ? (
        <div className="model-picker__menu" role="listbox" aria-label="Список моделей">
          <input
            className="model-picker__search"
            value={query}
            autoFocus
            placeholder="Поиск модели…"
            aria-label="Поиск модели"
            onChange={(event) => setQuery(event.target.value)}
          />
          <ul>
            {visible.length === 0 ? (
              <li className="model-picker__none">Ничего не найдено</li>
            ) : (
              visible.map((model) => (
                <li key={model.value}>
                  <button
                    type="button"
                    role="option"
                    aria-selected={model.value === current}
                    onClick={() => {
                      onSelect(model.value)
                      setOpen(false)
                    }}
                  >
                    {model.label}
                  </button>
                </li>
              ))
            )}
          </ul>
        </div>
      ) : null}
    </div>
  )
}

function latest(events: readonly CoreEvent[], eventType: string): CoreEvent | null {
  return events.find((event) => event.eventType === eventType) ?? null
}

function parseJson(payload: string): Record<string, unknown> {
  try {
    const value: unknown = JSON.parse(payload)
    return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {}
  } catch {
    return {}
  }
}
