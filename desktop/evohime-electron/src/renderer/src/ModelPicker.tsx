import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import type { ConnectionState, CoreEvent, ModelTier } from '@shared/api'

import { useShellApi } from './shell-api'

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
}

export function ModelPicker({ connection, events }: ModelPickerProps): React.JSX.Element | null {
  const api = useShellApi()
  const connected = CONNECTED_STATES.includes(connection)
  const [tier, setTier] = useState<ModelTier | null>(null)
  const [models, setModels] = useState<readonly string[]>([])
  const [current, setCurrent] = useState('')
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!api) return
    void api.invoke('provider.get', {}).then((outcome) => {
      if (outcome.ok) setTier(outcome.value.tier === 'paid' ? 'paid' : 'free')
    })
  }, [api])

  useEffect(() => {
    if (!api || !connected || tier === null) return
    void api.invoke('core.listModelCatalog', { mode: tier })
    void api.invoke('core.getModelConfig', {})
  }, [api, connected, tier])

  const catalog = useMemo(() => latest(events, 'model.catalog'), [events])
  const config = useMemo(() => latest(events, 'model.config'), [events])

  useEffect(() => {
    if (!catalog) return
    const parsed = parseJson(catalog.payload)
    setModels(
      Array.isArray(parsed['models'])
        ? parsed['models'].filter((model): model is string => typeof model === 'string')
        : []
    )
    setError(typeof parsed['error'] === 'string' ? parsed['error'] : null)
  }, [catalog])

  useEffect(() => {
    if (!config) return
    const parsed = parseJson(config.payload)
    if (typeof parsed['model'] === 'string') setCurrent(parsed['model'])
  }, [config])

  const select = useCallback(
    async (model: string) => {
      if (!api) return
      setCurrent(model)
      const outcome = await api.invoke('core.selectModel', { model })
      if (!outcome.ok) setError(outcome.message)
    },
    [api]
  )

  // A dropdown whose value is not in its own list still renders the first
  // option, which would show one model while Core used another — the route
  // default, which need not even exist in this tier. Commit to what is shown.
  useEffect(() => {
    if (models.length === 0) return
    if (current !== '' && models.includes(current)) return
    const first = models[0]
    if (first !== undefined) void select(first)
  }, [current, models, select])

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

  const known = models.includes(current)

  return (
    <ModelDropdown
      models={models}
      current={known ? current : ''}
      onSelect={(model) => void select(model)}
    />
  )
}

interface ModelDropdownProps {
  readonly models: readonly string[]
  readonly current: string
  readonly onSelect: (model: string) => void
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
      : models.filter((model) => model.toLowerCase().includes(needle))
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
                <li key={model}>
                  <button
                    type="button"
                    role="option"
                    aria-selected={model === current}
                    onClick={() => {
                      onSelect(model)
                      setOpen(false)
                    }}
                  >
                    {model}
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
