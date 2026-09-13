import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react'

import type { ProviderSummary } from '@shared/api'

import { useShellApi } from './shell-api'

interface ProviderStateValue {
  /** False only for isolated component tests; the app always provides the store. */
  readonly shared: boolean
  readonly summary: ProviderSummary | null
  readonly refresh: () => Promise<ProviderSummary | null>
  /** Apply a mutation response and invalidate older in-flight reads. */
  readonly apply: (summary: ProviderSummary) => void
}

const EMPTY_STATE: ProviderStateValue = {
  shared: false,
  summary: null,
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
  const generation = useRef(0)

  const apply = useCallback((next: ProviderSummary) => {
    generation.current += 1
    setSummary(next)
  }, [])

  const refresh = useCallback(async (): Promise<ProviderSummary | null> => {
    if (!api) return null
    const requestGeneration = ++generation.current
    const outcome = await api.invoke('provider.get', {})
    if (!outcome.ok) return null
    if (requestGeneration !== generation.current) return outcome.value
    setSummary(outcome.value)
    return outcome.value
  }, [api])

  useEffect(() => {
    generation.current += 1
    setSummary(null)
    void refresh()
  }, [refresh])

  const value = useMemo<ProviderStateValue>(() => ({ shared: true, summary, refresh, apply }), [apply, refresh, summary])
  return <ProviderStateContext.Provider value={value}>{children}</ProviderStateContext.Provider>
}

export function useProviderState(): ProviderStateValue {
  return useContext(ProviderStateContext)
}
