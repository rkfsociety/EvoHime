import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react'

import type { ProviderSummary } from '@shared/api'

import { useShellApi } from './shell-api'

interface ProviderStateValue {
  /** False only for isolated component tests; the app always provides the store. */
  readonly shared: boolean
  readonly summary: ProviderSummary | null
  readonly loading: boolean
  readonly error: string | null
  readonly refresh: () => Promise<ProviderSummary | null>
  /** Apply a mutation response and invalidate older in-flight reads. */
  readonly apply: (summary: ProviderSummary) => void
}

const EMPTY_STATE: ProviderStateValue = {
  shared: false,
  summary: null,
  loading: false,
  error: null,
  refresh: async () => null,
  apply: () => undefined
}

const ProviderStateContext = createContext<ProviderStateValue>(EMPTY_STATE)

/**
 * One renderer-owned snapshot of provider profiles.
 *
 * Settings and task pickers are siblings, so each component fetching its own
 * snapshot can display different answers after a provider mutation. This
 * provider owns the read and receives every successful mutation response. The
 * generation guard prevents a slow, older `provider.get` from overwriting a
 * newer `provider.save`/`provider.select` result.
 */
export function ProviderStateProvider({ children }: { readonly children: React.ReactNode }): React.JSX.Element {
  const api = useShellApi()
  const [summary, setSummary] = useState<ProviderSummary | null>(null)
  const [loading, setLoading] = useState<boolean>(() => Boolean(api))
  const [error, setError] = useState<string | null>(null)
  const generation = useRef(0)

  const apply = useCallback((next: ProviderSummary) => {
    generation.current += 1
    setSummary(next)
    setLoading(false)
    setError(null)
  }, [])

  const refresh = useCallback(async (): Promise<ProviderSummary | null> => {
    if (!api) {
      setLoading(false)
      return null
    }
    const requestGeneration = ++generation.current
    setLoading(true)
    setError(null)
    try {
      const outcome = await api.invoke('provider.get', {})
      if (requestGeneration !== generation.current) {
        return outcome.ok ? outcome.value : null
      }
      if (!outcome.ok) {
        setError(outcome.message || 'Не удалось загрузить настройки провайдера.')
        setLoading(false)
        return null
      }
      setSummary(outcome.value)
      setLoading(false)
      setError(null)
      return outcome.value
    } catch {
      if (requestGeneration !== generation.current) return null
      setError('Не удалось загрузить настройки провайдера.')
      setLoading(false)
      return null
    }
  }, [api])

  useEffect(() => {
    generation.current += 1
    setSummary(null)
    setLoading(Boolean(api))
    setError(null)
    void refresh()
  }, [api, refresh])

  const value = useMemo<ProviderStateValue>(
    () => ({ shared: true, summary, loading, error, refresh, apply }),
    [apply, error, loading, refresh, summary]
  )
  return <ProviderStateContext.Provider value={value}>{children}</ProviderStateContext.Provider>
}

export function useProviderState(): ProviderStateValue {
  return useContext(ProviderStateContext)
}
