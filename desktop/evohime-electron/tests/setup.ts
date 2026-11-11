/**
 * Test-environment storage shim.
 *
 * jsdom exposes `localStorage` only for non-opaque origins, and Node ≥ 22
 * additionally shadows it with its own experimental global (undefined without
 * `--localstorage-file`) that vitest's jsdom environment filter keeps instead
 * of the jsdom copy. The result is `window.localStorage === undefined` in
 * every DOM test. Panels under test persist real preferences there, so install
 * a small in-memory Storage when the environment provides none.
 */

class MemoryStorage {
  private readonly values = new Map<string, string>()

  get length(): number {
    return this.values.size
  }

  clear(): void {
    this.values.clear()
  }

  getItem(key: string): string | null {
    return this.values.has(key) ? (this.values.get(key) ?? null) : null
  }

  key(index: number): string | null {
    return Array.from(this.values.keys())[index] ?? null
  }

  removeItem(key: string): void {
    this.values.delete(key)
  }

  setItem(key: string, value: string): void {
    this.values.set(String(key), String(value))
  }
}

interface BrowserLikeGlobal {
  window?: {
    localStorage?: unknown
  } & Record<string, unknown>
}

const browserGlobal = globalThis as BrowserLikeGlobal

if (browserGlobal.window && !browserGlobal.window.localStorage) {
  Object.defineProperty(browserGlobal.window, 'localStorage', {
    value: new MemoryStorage(),
    configurable: true,
    writable: true
  })
}
