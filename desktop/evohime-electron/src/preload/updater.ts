import { contextBridge, ipcRenderer } from 'electron'

import type { EvoHimeUpdaterApi, UpdaterUiStatus } from '@shared/updater'

const api: EvoHimeUpdaterApi = {
  getStatus: () => ipcRenderer.invoke('updater.get-status') as Promise<UpdaterUiStatus>,
  subscribe(listener) {
    const handler = (_event: Electron.IpcRendererEvent, status: UpdaterUiStatus): void => listener(status)
    ipcRenderer.on('updater.status', handler)
    return () => ipcRenderer.removeListener('updater.status', handler)
  },
  apply: () => ipcRenderer.invoke('updater.apply') as Promise<void>,
  launch: () => ipcRenderer.invoke('updater.launch') as Promise<void>,
  close: () => ipcRenderer.invoke('updater.close') as Promise<void>,
  minimize: () => ipcRenderer.invoke('updater.minimize') as Promise<void>
}

contextBridge.exposeInMainWorld('evohimeUpdater', Object.freeze(api))
