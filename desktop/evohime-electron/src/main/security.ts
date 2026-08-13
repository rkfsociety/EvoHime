import { app, shell, Menu, session, type WebContents } from 'electron'

import type { ShellLog } from './diagnostics/logger'
import {
  CONTENT_SECURITY_POLICY,
  findForbiddenSwitches,
  isAllowedExternalUrl
} from './security-policy'

/**
 * Process- and session-wide hardening for the Electron shell (plan 0, stage 2).
 *
 * Everything here is deny-by-default: navigation, new windows, Chromium
 * permissions and external links are refused unless they match an explicit
 * allow-list. The renderer is untrusted, and the shell is a transport layer,
 * not a security authority.
 */

export const isProduction = (): boolean => app.isPackaged

export {
  CONTENT_SECURITY_POLICY,
  EXTERNAL_URL_ALLOW_LIST,
  FORBIDDEN_SWITCHES,
  findForbiddenSwitches,
  isAllowedExternalUrl,
  isForbiddenSwitch
} from './security-policy'

export interface HardeningOptions {
  /** Origin the packaged renderer is served from; navigation stays inside it. */
  readonly rendererOrigin: string
  readonly log: ShellLog
}

export function hardenProcess(options: HardeningOptions): void {
  // A packaged build refuses to start with debug switches rather than quietly
  // dropping them, so a "working" launch never means a weakened one.
  const forbidden = findForbiddenSwitches(process.argv.slice(1))
  if (forbidden.length > 0) {
    options.log('error', 'shell.forbidden_switch', { count: forbidden.length })
    if (isProduction()) {
      app.exit(2)
      return
    }
  }

  app.enableSandbox()
  if (isProduction()) {
    Menu.setApplicationMenu(null)
  }
}

export function hardenSession(options: HardeningOptions): void {
  const defaultSession = session.defaultSession

  defaultSession.setPermissionRequestHandler((_contents, permission, callback) => {
    options.log('warn', 'shell.permission_denied', { permission })
    callback(false)
  })
  defaultSession.setPermissionCheckHandler((_contents, permission) => {
    options.log('warn', 'shell.permission_check_denied', { permission })
    return false
  })
  defaultSession.setDevicePermissionHandler(() => false)
  defaultSession.setDisplayMediaRequestHandler((_request, callback) => {
    callback({})
  })

  // No remote content: everything the renderer needs ships in the package.
  defaultSession.webRequest.onBeforeRequest((details, callback) => {
    const allowed =
      details.url.startsWith(options.rendererOrigin) ||
      details.url.startsWith('devtools://') ||
      details.url.startsWith('blob:') ||
      details.url.startsWith('data:')
    if (!allowed) {
      options.log('warn', 'shell.request_blocked', { resourceType: details.resourceType })
    }
    callback({ cancel: !allowed })
  })

  defaultSession.webRequest.onHeadersReceived((details, callback) => {
    callback({
      responseHeaders: {
        ...details.responseHeaders,
        'Content-Security-Policy': [CONTENT_SECURITY_POLICY]
      }
    })
  })
}

export function hardenWebContents(contents: WebContents, options: HardeningOptions): void {
  contents.setWindowOpenHandler(({ url }) => {
    if (isAllowedExternalUrl(url)) {
      void shell.openExternal(url)
    } else {
      options.log('warn', 'shell.window_open_denied', {})
    }
    return { action: 'deny' }
  })

  contents.on('will-navigate', (event, url) => {
    if (!url.startsWith(options.rendererOrigin)) {
      event.preventDefault()
      options.log('warn', 'shell.navigation_denied', {})
    }
  })

  contents.on('will-attach-webview', (event) => {
    event.preventDefault()
    options.log('warn', 'shell.webview_denied', {})
  })

  contents.on('before-input-event', (event, input) => {
    if (!isProduction()) {
      return
    }
    const key = input.key.toLowerCase()
    const devToolsShortcut =
      key === 'f12' || (input.control && input.shift && (key === 'i' || key === 'c' || key === 'j'))
    if (devToolsShortcut) {
      event.preventDefault()
    }
  })
}
