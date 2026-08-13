import type { EvoHimeApiV1 } from '@shared/api'

/**
 * Single access point to the preload bridge.
 *
 * Components never reach for `window` themselves, so a missing bridge (preload
 * failed to load) surfaces as one explicit recovery state instead of a crash in
 * every component.
 */
export function useShellApi(): EvoHimeApiV1 | null {
  return window.evohime?.v1 ?? null
}
