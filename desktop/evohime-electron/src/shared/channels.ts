/**
 * Internal Electron IPC channel names.
 *
 * These are an implementation detail of the preload bridge: the renderer never
 * sees `ipcRenderer` and cannot address a channel that is not listed here.
 */
export const INVOKE_CHANNEL = 'evohime.v1.invoke'
export const EVENT_CHANNEL = 'evohime.v1.event'
export const CLIPBOARD_CHANNEL = 'evohime.v1.clipboard.writeText'
export const OPEN_EXTERNAL_CHANNEL = 'evohime.v1.openExternal'
