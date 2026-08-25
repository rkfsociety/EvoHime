import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'

import type { PermissionMode } from '@shared/api'

import { isAbsolutePath, normalizePath } from './path-utils'

/**
 * Persisted workspace preference of the shell (plan 0, stage 3, slice 1).
 *
 * This is a UI preference, not agent state: it records which workspace the user
 * last picked and a bounded list of recent ones. Core stays the authority for
 * workspace scope and re-validates the path on every command that uses it.
 *
 * The file is separate from the WinUI shell's `settings.json` so the two shells
 * never fight over one identifier while the fallback is supported.
 */

export const MAX_RECENT_WORKSPACES = 10
export const MAX_PATH_CHARS = 1_024
export const STORE_VERSION = 1

export interface WorkspaceEntry {
  readonly path: string
  readonly lastUsedMs: number
  readonly permissionMode?: PermissionMode
}

export interface WorkspaceState {
  readonly selected: string | null
  readonly recent: readonly WorkspaceEntry[]
}

const EMPTY: WorkspaceState = { selected: null, recent: [] }

/**
 * Accepts an absolute local path only. A relative path, a path with a NUL byte
 * or an unbounded one is refused before it reaches the store or Core.
 */
export function normalizeWorkspacePath(value: unknown): string | null {
  if (typeof value !== 'string') {
    return null
  }
  const trimmed = value.trim()
  if (
    trimmed.length === 0 ||
    trimmed.length > MAX_PATH_CHARS ||
    trimmed.includes('\0') ||
    !isAbsolutePath(trimmed)
  ) {
    return null
  }
  const normalized = normalizePath(trimmed)
  return normalized.length <= MAX_PATH_CHARS ? normalized : null
}

function samePath(left: string, right: string): boolean {
  // Windows paths are case-insensitive; comparing raw strings would let one
  // workspace appear twice in the recent list.
  return left.toLowerCase() === right.toLowerCase()
}

export class WorkspaceStore {
  constructor(
    private readonly filePath: string,
    private readonly now: () => number = Date.now
  ) {}

  static defaultPath(dataDirectory: string): string {
    return join(dataDirectory, 'shell', 'workspaces.json')
  }

  read(): WorkspaceState {
    let raw: string
    try {
      raw = readFileSync(this.filePath, 'utf8')
    } catch {
      return EMPTY
    }
    return this.parse(raw)
  }

  /** Records `path` as the current selection and moves it to the front. */
  select(path: string): WorkspaceState {
    const normalized = normalizeWorkspacePath(path)
    if (normalized === null) {
      return this.read()
    }
    const current = this.read()
    const others = current.recent.filter((entry) => !samePath(entry.path, normalized))
    const next: WorkspaceState = {
      selected: normalized,
      recent: [{ path: normalized, lastUsedMs: this.now() }, ...others].slice(
        0,
        MAX_RECENT_WORKSPACES
      )
    }
    this.write(next)
    return next
  }

  getPermissionMode(path: string): PermissionMode {
    const normalized = normalizeWorkspacePath(path)
    if (normalized === null) return 'ask'
    return this.read().recent.find((entry) => samePath(entry.path, normalized))?.permissionMode ?? 'ask'
  }

  setPermissionMode(path: string, mode: PermissionMode): WorkspaceState {
    const normalized = normalizeWorkspacePath(path)
    const current = this.read()
    if (normalized === null || !['ask', 'read_only', 'full'].includes(mode)) return current
    const recent = current.recent.map((entry) =>
      samePath(entry.path, normalized) ? { ...entry, permissionMode: mode } : entry
    )
    const next: WorkspaceState = { selected: current.selected, recent }
    this.write(next)
    return next
  }

  /** Drops one workspace from the list, clearing the selection if it was it. */
  forget(path: string): WorkspaceState {
    const normalized = normalizeWorkspacePath(path)
    const current = this.read()
    if (normalized === null) {
      return current
    }
    const recent = current.recent.filter((entry) => !samePath(entry.path, normalized))
    const selected =
      current.selected && samePath(current.selected, normalized) ? null : current.selected
    const next: WorkspaceState = { selected, recent }
    this.write(next)
    return next
  }

  private parse(raw: string): WorkspaceState {
    let document: unknown
    try {
      document = JSON.parse(raw)
    } catch {
      // A corrupt preference file must not take the shell down; the user simply
      // starts from an empty list.
      return EMPTY
    }
    if (typeof document !== 'object' || document === null) {
      return EMPTY
    }
    const record = document as Record<string, unknown>
    const recent: WorkspaceEntry[] = []
    for (const item of Array.isArray(record['recent']) ? record['recent'] : []) {
      if (typeof item !== 'object' || item === null) {
        continue
      }
      const entry = item as Record<string, unknown>
      const path = normalizeWorkspacePath(entry['path'])
      if (path === null || recent.some((known) => samePath(known.path, path))) {
        continue
      }
      const lastUsedMs = typeof entry['lastUsedMs'] === 'number' ? entry['lastUsedMs'] : 0
      const permissionMode = entry['permissionMode']
      recent.push({
        path,
        lastUsedMs: Number.isFinite(lastUsedMs) ? lastUsedMs : 0,
        ...(permissionMode === 'ask' || permissionMode === 'read_only' || permissionMode === 'full'
          ? { permissionMode }
          : {})
      })
      if (recent.length >= MAX_RECENT_WORKSPACES) {
        break
      }
    }
    const selected = normalizeWorkspacePath(record['selected'])
    return {
      selected: selected && recent.some((entry) => samePath(entry.path, selected)) ? selected : null,
      recent
    }
  }

  private write(state: WorkspaceState): void {
    const document = { version: STORE_VERSION, selected: state.selected, recent: state.recent }
    const temporary = `${this.filePath}.${this.now()}.tmp`
    try {
      mkdirSync(dirname(this.filePath), { recursive: true })
      writeFileSync(temporary, JSON.stringify(document), 'utf8')
      // Rename is atomic on the same volume, so a crash never leaves a half
      // written preference file behind.
      renameSync(temporary, this.filePath)
    } catch {
      rmSync(temporary, { force: true })
    }
  }
}
