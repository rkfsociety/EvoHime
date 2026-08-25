/**
 * Shell-side redaction layer.
 *
 * Every diagnostic written by the Electron main process goes through this
 * module before it reaches a file, a renderer channel or an exported bundle
 * (plan 0, stage 2). It mirrors `evohime_core::observability::redact_text` and
 * `is_sensitive_name`, and additionally strips filesystem paths, command-line
 * arguments and stack frames, which only the shell produces.
 *
 * Core keeps redacting its own journal: this layer never replaces it.
 */

export const REDACTED = '[REDACTED]'
export const REDACTED_PATH = '[PATH]'

export const MAX_TEXT_CHARS = 2_000
export const MAX_OBJECT_KEYS = 64
export const MAX_DEPTH = 6

const SENSITIVE_NAME_MARKERS = [
  'secret',
  'token',
  'password',
  'passphrase',
  'api_key',
  'apikey',
  'authorization',
  'credential',
  'cookie',
  'session_key',
  'private_key'
]

const SECRET_TOKEN_PREFIXES = ['bearer', 'sk-', 'ghp_', 'gho_', 'github_pat_', 'aiza', 'xoxb-']

/** Windows drive paths, UNC paths, pipe paths and POSIX-looking absolute paths. */
const PATH_PATTERN = /(?:[A-Za-z]:\\|\\\\[.?]?\\|\\\\)[^\s"'<>|]*|(?:\/[A-Za-z0-9._-]+){2,}/g

export function isSensitiveName(name: string): boolean {
  const lower = name.toLowerCase()
  return SENSITIVE_NAME_MARKERS.some((marker) => lower.includes(marker))
}

export function redactText(input: string): string {
  const bounded = input.length > MAX_TEXT_CHARS ? `${input.slice(0, MAX_TEXT_CHARS)}…` : input
  const credentialSafe = bounded.replace(/(?<![A-Za-z0-9])(?:bearer\s+|sk-|ghp_|gho_|github_pat_|aiza|xoxb-)[A-Za-z0-9._+\-/=]+/gi, REDACTED)
  return credentialSafe
    .split(/(\s+)/)
    .map((token) => (isSecretToken(token) ? REDACTED : redactPaths(token)))
    .join('')
}

export function redactPaths(input: string): string {
  return input.replace(PATH_PATTERN, REDACTED_PATH)
}

/** Keeps the error class and a redacted message; stack frames are dropped. */
export function redactError(error: unknown): string {
  if (error instanceof Error) {
    return redactText(`${error.name}: ${error.message}`)
  }
  if (typeof error === 'string') {
    return redactText(error)
  }
  return 'unknown error'
}

export type RedactedValue =
  | string
  | number
  | boolean
  | null
  | RedactedValue[]
  | { [key: string]: RedactedValue }

export function redactValue(value: unknown, depth = 0): RedactedValue {
  if (value === null || value === undefined) {
    return null
  }
  if (typeof value === 'string') {
    return redactText(value)
  }
  if (typeof value === 'number') {
    return Number.isFinite(value) ? value : null
  }
  if (typeof value === 'boolean') {
    return value
  }
  if (value instanceof Error) {
    return redactError(value)
  }
  if (depth >= MAX_DEPTH) {
    return REDACTED
  }
  if (Array.isArray(value)) {
    return value.slice(0, MAX_OBJECT_KEYS).map((item) => redactValue(item, depth + 1))
  }
  if (typeof value === 'object') {
    const result: Record<string, RedactedValue> = {}
    for (const [key, item] of Object.entries(value as Record<string, unknown>).slice(
      0,
      MAX_OBJECT_KEYS
    )) {
      result[key] = isSensitiveName(key) ? REDACTED : redactValue(item, depth + 1)
    }
    return result
  }
  return REDACTED
}

/**
 * Command-line arguments frequently carry pipe names, challenges and paths, so
 * only the flag name survives; every value becomes a placeholder.
 */
export function redactArgv(argv: readonly string[]): string[] {
  return argv.map((argument) => {
    if (!argument.startsWith('-')) {
      return REDACTED
    }
    const separator = argument.indexOf('=')
    return separator === -1 ? argument : `${argument.slice(0, separator)}=${REDACTED}`
  })
}

function isSecretToken(token: string): boolean {
  const trimmed = token.trim().replace(/^["'([{]+|["')\]},.]+$/g, '')
  if (trimmed.length === 0) {
    return false
  }
  const lower = trimmed.toLowerCase()
  if (SECRET_TOKEN_PREFIXES.some((prefix) => lower.startsWith(prefix))) {
    return true
  }
  const at = trimmed.indexOf('@')
  if (at > 0 && trimmed.indexOf('.', at) > at) {
    return true
  }
  // Long opaque blobs are treated as credentials even without a known prefix.
  return trimmed.length >= 40 && /^[A-Za-z0-9+/=_-]+$/.test(trimmed)
}
