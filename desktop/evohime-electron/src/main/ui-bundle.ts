import { existsSync, readFileSync } from 'node:fs'
import { isAbsolute, join, relative, resolve } from 'node:path'

export interface UiBundleOptions {
  readonly root: string
  readonly fallback: string
}

/** Selects an active version only when the pointer is bounded and complete. */
export function resolveUiEntry(options: UiBundleOptions): string {
  const root = resolve(options.root)
  const fallback = resolve(options.fallback)
  const pointer = join(root, 'ui-active.json')
  if (existsSync(pointer)) {
    try {
      const value = JSON.parse(readFileSync(pointer, 'utf8')) as { version?: unknown }
      if (typeof value.version === 'string' && /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(value.version)) {
        const candidate = resolve(root, 'ui-bundles', value.version, 'index.html')
        const rel = relative(root, candidate)
        if (!isAbsolute(rel) && !rel.startsWith('..') && existsSync(candidate)) return candidate
      }
    } catch {
      // A corrupt pointer must fall back to the last bundled UI.
    }
  }
  return existsSync(fallback) ? fallback : join(root, 'index.html')
}
