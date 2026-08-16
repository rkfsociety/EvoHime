/**
 * Pure security policy decisions, kept free of Electron imports so they can be
 * unit-tested and reused by both the main process and the packaging checks.
 */

/** Debug switches that must never take effect in a packaged build. */
export const FORBIDDEN_SWITCHES = [
  'remote-debugging-port',
  'remote-debugging-pipe',
  'inspect',
  'inspect-brk',
  'inspect-port',
  'js-flags',
  'no-sandbox',
  'disable-web-security',
  'allow-file-access-from-files',
  'enable-logging'
]

export function isForbiddenSwitch(argument: string): boolean {
  const normalized = argument.replace(/^--?/, '').split('=')[0]?.toLowerCase() ?? ''
  return FORBIDDEN_SWITCHES.includes(normalized)
}

export function findForbiddenSwitches(argv: readonly string[]): string[] {
  return argv.filter((argument) => argument.startsWith('-') && isForbiddenSwitch(argument))
}

/** Only bounded https documentation/provider links may leave the shell. */
export const EXTERNAL_URL_ALLOW_LIST = ['https://github.com/', 'https://docs.evohime.dev/']

export function isAllowedExternalUrl(rawUrl: string): boolean {
  let url: URL
  try {
    url = new URL(rawUrl)
  } catch {
    return false
  }
  if (url.protocol !== 'https:') {
    return false
  }
  // Compare against the parsed origin so a crafted `https://github.com.evil.tld`
  // or `https://evil.tld/?x=https://github.com/` never matches.
  return EXTERNAL_URL_ALLOW_LIST.some((prefix) => {
    const allowed = new URL(prefix)
    return url.origin === allowed.origin && url.pathname.startsWith(allowed.pathname)
  })
}

/**
 * Production CSP: own package only, no eval, no remote content, no inline
 * script, no navigation target outside the packaged renderer.
 */
export const CONTENT_SECURITY_POLICY = [
  "default-src 'self'",
  "script-src 'self'",
  "style-src 'self'",
  "img-src 'self' data:",
  "font-src 'self'",
  "connect-src 'self'",
  "object-src 'none'",
  "frame-src 'none'",
  "worker-src 'self'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'"
].join('; ')

/** Dev-only policy for the local Vite renderer and its HMR websocket. */
export const DEV_CONTENT_SECURITY_POLICY = [
  "default-src 'self' http://localhost:5173",
  "script-src 'self' http://localhost:5173",
  "style-src 'self' 'unsafe-inline' http://localhost:5173",
  "img-src 'self' data: http://localhost:5173",
  "font-src 'self' http://localhost:5173",
  "connect-src 'self' http://localhost:5173 ws://localhost:5173",
  "object-src 'none'",
  "frame-src 'none'",
  "worker-src 'self' blob:",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'"
].join('; ')
