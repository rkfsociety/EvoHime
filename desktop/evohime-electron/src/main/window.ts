import { join } from 'node:path'

import { BrowserWindow, nativeTheme, type TitleBarOverlay } from 'electron'

import type { AppTheme } from '../shared/api'
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
}

const TITLEBAR_HEIGHT = 56
const DARK_BACKGROUND = '#0c1017'
const LIGHT_BACKGROUND = '#f3f5f9'

function windowBackgroundColor(dark: boolean): string {
  return dark ? DARK_BACKGROUND : LIGHT_BACKGROUND
}

function titleBarOverlayOptions(dark: boolean): TitleBarOverlay {
  return {
    color: '#00000000',
    symbolColor: dark ? '#a6b1c2' : '#516075',
    height: TITLEBAR_HEIGHT
  }
}

/** Updates the native theme source so window chrome matches the renderer theme. */
export function applyWindowTheme(theme: AppTheme): void {
  nativeTheme.themeSource = theme
}

export function createMainWindow(options: WindowOptions): BrowserWindow {
  const dark = nativeTheme.shouldUseDarkColors
  const window = new BrowserWindow({
    width: 1_360,
    height: 860,
    minWidth: 960,
    minHeight: 640,
    useContentSize: true,
    center: true,
    show: false,
    titleBarStyle: 'hidden',
    titleBarOverlay: titleBarOverlayOptions(dark),
    backgroundColor: windowBackgroundColor(dark),
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

  const syncTheme = (): void => {
    if (window.isDestroyed()) return
    const isDark = nativeTheme.shouldUseDarkColors
    window.setBackgroundColor?.(windowBackgroundColor(isDark))
    window.setTitleBarOverlay?.(titleBarOverlayOptions(isDark))
  }
  nativeTheme.on('updated', syncTheme)
  window.once('closed', () => {
    nativeTheme.off('updated', syncTheme)
  })

  window.once('ready-to-show', () => {
    // The standalone updater owns blocking progress; the shell window remains
    // visible immediately while its process-level launch gate is running.
    focusWindow(window)
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
