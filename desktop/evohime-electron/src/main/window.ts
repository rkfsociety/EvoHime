import { join } from 'node:path'

import { BrowserWindow } from 'electron'

import { hardenWebContents, isProduction, type HardeningOptions } from './security'
import { resourcePath } from './paths'
import { resolveUiEntry } from './ui-bundle'

/**
 * The single application window. It never gets Node integration, always runs a
 * sandboxed renderer, and only loads the packaged renderer entry point. The
 * renderer is a separate build output so release tooling can version and
 * replace it without rebuilding native code.
 */

export interface WindowOptions extends HardeningOptions {
  readonly onRendererFailure: (reason: string) => void
  /** The updater owns the first visible frame during a launch gate. */
  readonly showOnReady?: () => boolean
}

export function createMainWindow(options: WindowOptions): BrowserWindow {
  const window = new BrowserWindow({
    width: 1_280,
    height: 800,
    minWidth: 960,
    minHeight: 640,
    show: false,
    backgroundColor: '#101014',
    icon: resourcePath('evohime-agent.ico'),
    autoHideMenuBar: true,
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
      nodeIntegrationInWorker: false,
      nodeIntegrationInSubFrames: false,
      webviewTag: false,
      webSecurity: true,
      allowRunningInsecureContent: false,
      spellcheck: false,
      devTools: !isProduction()
    }
  })

  hardenWebContents(window.webContents, options)

  window.once('ready-to-show', () => {
    if (options.showOnReady?.() ?? true) window.show()
  })

  window.webContents.on('render-process-gone', (_event, details) => {
    options.onRendererFailure(details.reason)
  })
  window.webContents.on('preload-error', (_event, _preloadPath, error) => {
    options.log('error', 'shell.preload_error', { error })
    options.onRendererFailure('preload-error')
  })
  window.webContents.on('unresponsive', () => {
    options.log('warn', 'shell.renderer_unresponsive', {})
  })

  void loadRenderer(window)
  return window
}

export function loadRenderer(window: BrowserWindow): Promise<void> {
  const devServerUrl = process.env['ELECTRON_RENDERER_URL']
  if (!isProduction() && devServerUrl) {
    return window.loadURL(devServerUrl)
  }
  const bundledRoot = join(__dirname, '..')
  const installRoot = isProduction() ? join(process.resourcesPath, '..') : bundledRoot
  return window.loadFile(resolveUiEntry({
    root: installRoot,
    fallback: join(bundledRoot, 'ui-bundle/index.html')
  }))
}

/** Focuses the existing window for the single-instance handoff. */
export function focusWindow(window: BrowserWindow): void {
  if (window.isMinimized()) {
    window.restore()
  }
  if (!window.isVisible()) {
    window.show()
  }
  window.focus()
}
