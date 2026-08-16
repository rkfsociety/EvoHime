import { contextBridge, ipcRenderer, webUtils } from 'electron'

import {
  API_NAMESPACE,
  API_VERSION,
  type CommandOutcome,
  type CommandPayloads,
  type EvoHimeApiV1,
  type RendererCommand,
  type ShellEvent
} from '@shared/api'
import {
  CLIPBOARD_CHANNEL,
  EVENT_CHANNEL,
  INVOKE_CHANNEL,
  OPEN_EXTERNAL_CHANNEL
} from '@shared/channels'

/**
 * The whole renderer-visible surface (plan 0, stage 2).
 *
 * Nothing Electron-shaped crosses the bridge: no `ipcRenderer`, no emitter, no
 * MessagePort, no `fs`/`shell`/`child_process`/environment access. The renderer
 * only gets plain data and functions returning plain data.
 */

const api: EvoHimeApiV1 = {
  apiVersion: API_VERSION,

  invoke<C extends RendererCommand>(
    command: C,
    payload: CommandPayloads[C]
  ): Promise<CommandOutcome<C>> {
    return ipcRenderer.invoke(INVOKE_CHANNEL, command, payload) as Promise<CommandOutcome<C>>
  },

  subscribe(listener: (event: ShellEvent) => void): () => void {
    const handler = (_event: Electron.IpcRendererEvent, payload: ShellEvent): void => {
      listener(payload)
    }
    ipcRenderer.on(EVENT_CHANNEL, handler)
    return () => {
      ipcRenderer.removeListener(EVENT_CHANNEL, handler)
    }
  },

  writeClipboardText(text: string): Promise<boolean> {
    return ipcRenderer.invoke(CLIPBOARD_CHANNEL, text) as Promise<boolean>
  },

  openExternal(url: string): Promise<boolean> {
    return ipcRenderer.invoke(OPEN_EXTERNAL_CHANNEL, url) as Promise<boolean>
  },

  // Единственное место, где renderer вообще узнаёт путь: имя папки нужно, чтобы
  // диалог выбора открывался там же, куда пользователь уже ходил. Ничего
  // Electron-образного при этом не пересекает мост — наружу уходит строка.
  pathForFile(file: File): string {
    try {
      return webUtils.getPathForFile(file)
    } catch {
      return ''
    }
  }
}

contextBridge.exposeInMainWorld(API_NAMESPACE, Object.freeze({ v1: Object.freeze(api) }))
