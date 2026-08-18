import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import {
  MAX_RECENT_WORKSPACES,
  normalizeWorkspacePath,
  WorkspaceStore
} from '../src/main/workspace-store'
import { WorkspaceService } from '../src/main/workspace-service'

const directories: string[] = []

afterEach(() => {
  for (const directory of directories.splice(0, directories.length)) {
    rmSync(directory, { recursive: true, force: true })
  }
})

function newStore(): { store: WorkspaceStore; file: string } {
  const directory = mkdtempSync(join(tmpdir(), 'evohime-workspaces-'))
  directories.push(directory)
  const file = join(directory, 'shell', 'workspaces.json')
  let tick = 1_000
  return { store: new WorkspaceStore(file, () => (tick += 1)), file }
}

describe('workspace path validation', () => {
  it('accepts an absolute path and normalizes it', () => {
    expect(normalizeWorkspacePath('C:\\work\\repo\\')).toBe('C:\\work\\repo')
    expect(normalizeWorkspacePath('  C:\\work\\repo  ')).toBe('C:\\work\\repo')
  })

  it('refuses relative, empty, unbounded and NUL-bearing paths', () => {
    expect(normalizeWorkspacePath('relative\\path')).toBeNull()
    expect(normalizeWorkspacePath('')).toBeNull()
    expect(normalizeWorkspacePath(`C:\\${'x'.repeat(2_000)}`)).toBeNull()
    expect(normalizeWorkspacePath('C:\\work\0\\repo')).toBeNull()
    expect(normalizeWorkspacePath(42)).toBeNull()
    expect(normalizeWorkspacePath(null)).toBeNull()
  })
})

describe('workspace store', () => {
  it('starts empty and persists a selection', () => {
    const { store, file } = newStore()
    expect(store.read()).toEqual({ selected: null, recent: [] })

    const state = store.select('C:\\work\\repo')
    expect(state.selected).toBe('C:\\work\\repo')
    expect(state.recent.map((entry) => entry.path)).toEqual(['C:\\work\\repo'])
    expect(JSON.parse(readFileSync(file, 'utf8')).version).toBe(1)

    // A fresh instance sees the same persisted state.
    expect(new WorkspaceStore(file).read().selected).toBe('C:\\work\\repo')
  })

  it('keeps the recent list unique, ordered and bounded', () => {
    const { store } = newStore()
    for (let index = 0; index < MAX_RECENT_WORKSPACES + 5; index += 1) {
      store.select(`C:\\work\\repo-${index}`)
    }
    store.select('C:\\WORK\\REPO-3')

    const state = store.read()
    expect(state.recent).toHaveLength(MAX_RECENT_WORKSPACES)
    expect(state.recent[0]?.path).toBe('C:\\WORK\\REPO-3')
    // Case-insensitive on Windows: the same folder must not appear twice.
    const lowered = state.recent.map((entry) => entry.path.toLowerCase())
    expect(new Set(lowered).size).toBe(lowered.length)
  })

  it('persists a permission mode independently for each workspace', () => {
    const { store } = newStore()
    store.select('C:\\work\\ask')
    store.select('C:\\work\\full')
    store.setPermissionMode('C:\\work\\full', 'full')
    store.setPermissionMode('C:\\work\\ask', 'read_only')

    expect(store.getPermissionMode('C:\\work\\full')).toBe('full')
    expect(store.getPermissionMode('C:\\work\\ask')).toBe('read_only')
    expect(store.getPermissionMode('C:\\work\\unknown')).toBe('ask')
  })

  it('forgets a workspace and clears the selection when it was selected', () => {
    const { store } = newStore()
    store.select('C:\\work\\a')
    store.select('C:\\work\\b')

    const afterForget = store.forget('C:\\work\\b')
    expect(afterForget.selected).toBeNull()
    expect(afterForget.recent.map((entry) => entry.path)).toEqual(['C:\\work\\a'])
  })

  it('recovers from a corrupt or hostile preference file', () => {
    const { store, file } = newStore()
    store.select('C:\\work\\repo')

    writeFileSync(file, 'not json at all', 'utf8')
    expect(store.read()).toEqual({ selected: null, recent: [] })

    writeFileSync(
      file,
      JSON.stringify({
        selected: 'relative\\path',
        recent: [{ path: 'relative\\path' }, { path: 'C:\\work\\ok' }, 'garbage', null]
      }),
      'utf8'
    )
    const state = store.read()
    expect(state.recent.map((entry) => entry.path)).toEqual(['C:\\work\\ok'])
    // A selection that is not in the list is dropped instead of trusted.
    expect(state.selected).toBeNull()
  })
})

describe('workspace service', () => {
  function newService(chosen: string | null, existing: readonly string[] = []) {
    const { store } = newStore()
    return new WorkspaceService({
      store,
      chooseDirectory: async () => chosen,
      isDirectory: (path) => existing.some((item) => item.toLowerCase() === path.toLowerCase())
    })
  }

  it('records the picked folder and reports it as available', async () => {
    const service = newService('C:\\work\\repo', ['C:\\work\\repo'])
    const result = await service.pick()

    expect(result.cancelled).toBe(false)
    expect(result.selection.selected).toBe('C:\\work\\repo')
    expect(result.selection.options[0]).toMatchObject({ path: 'C:\\work\\repo', available: true })
  })

  it('leaves the selection untouched when the dialog is cancelled', async () => {
    const service = newService(null)
    const result = await service.pick()

    expect(result.cancelled).toBe(true)
    expect(result.selection.selected).toBeNull()
    expect(result.selection.options).toHaveLength(0)
  })

  it('marks a remembered folder that disappeared as unavailable', async () => {
    const service = newService('C:\\work\\gone', [])
    const result = await service.pick()

    expect(result.selection.options[0]).toMatchObject({ path: 'C:\\work\\gone', available: false })
  })

  it('refuses to select a folder the user never picked', async () => {
    const service = newService('C:\\work\\repo', ['C:\\work\\repo'])
    await service.pick()

    expect(service.select('C:\\somewhere\\else')).toBe('unknown-workspace')
    expect(service.select('relative')).toBe('unknown-workspace')
    expect(service.list().selected).toBe('C:\\work\\repo')
  })
})
