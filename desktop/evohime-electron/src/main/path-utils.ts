import { dirname, isAbsolute, join, resolve, win32 } from 'node:path'

/** Path helpers used by code that is tested on Linux but runs on Windows. */
export function isAbsolutePath(value: string): boolean {
  return isAbsolute(value) || /^[A-Za-z]:[\\/]/.test(value) || value.startsWith('\\\\')
}

export function joinPath(base: string, ...parts: string[]): string {
  return isWindowsPath(base) ? win32.join(base, ...parts) : join(base, ...parts)
}

export function dirnamePath(value: string): string {
  return isWindowsPath(value) ? win32.dirname(value) : dirname(value)
}

export function normalizePath(value: string): string {
  if (!isWindowsPath(value)) return resolve(value)
  const normalized = win32.normalize(value)
  const root = win32.parse(normalized).root
  return normalized.length > root.length ? normalized.replace(/[\\/]$/, '') : normalized
}

function isWindowsPath(value: string): boolean {
  return /^[A-Za-z]:[\\/]/.test(value) || value.startsWith('\\\\')
}
