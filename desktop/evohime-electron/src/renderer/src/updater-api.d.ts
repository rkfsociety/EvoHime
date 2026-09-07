import type { EvoHimeUpdaterApi } from '@shared/updater'

declare global {
  interface Window {
    readonly evohimeUpdater: EvoHimeUpdaterApi
  }
}

export {}
