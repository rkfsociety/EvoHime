import { statSync } from 'node:fs'

import { dialog, type BrowserWindow } from 'electron'

import type { WorkspaceOption, WorkspaceSelection } from '@shared/api'

import { normalizeWorkspacePath, WorkspaceStore } from './workspace-store'

/**
 * Workspace picker service (plan 0, stage 3, slice 1).
 *
 * The renderer never touches the filesystem: it asks the main process to open
 * the native folder dialog and to remember the choice. Availability of a
 * remembered directory is reported so the UI can show an explicit failure state
 * instead of silently pointing at a workspace that no longer exists.
 */

export interface WorkspaceServiceOptions {
  readonly store: WorkspaceStore
  /** Injected in tests; production opens the OS folder dialog. */
  readonly chooseDirectory?: () => Promise<string | null>
  readonly isDirectory?: (path: string) => boolean
}

export class WorkspaceService {
  private readonly store: WorkspaceStore
  private readonly chooseDirectory: () => Promise<string | null>
  private readonly isDirectory: (path: string) => boolean

  constructor(options: WorkspaceServiceOptions) {
    this.store = options.store
    this.chooseDirectory = options.chooseDirectory ?? defaultChooseDirectory
    this.isDirectory = options.isDirectory ?? defaultIsDirectory
  }

  list(): WorkspaceSelection {
    return this.decorate()
  }

  /**
   * Opens the native folder dialog. A cancelled dialog leaves the persisted
   * selection untouched.
   */
  async pick(): Promise<{ cancelled: boolean; selection: WorkspaceSelection }> {
    const chosen = await this.chooseDirectory()
    const normalized = normalizeWorkspacePath(chosen)
    if (normalized === null) {
      return { cancelled: true, selection: this.decorate() }
    }
    this.store.select(normalized)
    return { cancelled: false, selection: this.decorate() }
  }

  /** Selects a workspace the shell already remembers. */
  select(path: string): WorkspaceSelection | 'unknown-workspace' {
    const normalized = normalizeWorkspacePath(path)
    if (normalized === null) {
      return 'unknown-workspace'
    }
    const known = this.store
      .read()
      .recent.some((entry) => entry.path.toLowerCase() === normalized.toLowerCase())
    if (!known) {
      // A path the user never picked cannot be selected through this command;
      // adding one always goes through the native dialog.
      return 'unknown-workspace'
    }
    this.store.select(normalized)
    return this.decorate()
  }

  forget(path: string): WorkspaceSelection {
    this.store.forget(path)
    return this.decorate()
  }

  private decorate(): WorkspaceSelection {
    const state = this.store.read()
    const options: WorkspaceOption[] = state.recent.map((entry) => ({
      path: entry.path,
      available: this.isDirectory(entry.path),
      lastUsedMs: entry.lastUsedMs
    }))
    return { selected: state.selected, options }
  }
}

async function defaultChooseDirectory(): Promise<string | null> {
  const result = await dialog.showOpenDialog({
    title: 'Выбери рабочую папку',
    properties: ['openDirectory', 'createDirectory']
  })
  return result.canceled ? null : (result.filePaths[0] ?? null)
}

function defaultIsDirectory(path: string): boolean {
  try {
    return statSync(path).isDirectory()
  } catch {
    return false
  }
}

/** Binds the dialog to the shell window so it opens modal to it. */
export function windowChooser(window: BrowserWindow): () => Promise<string | null> {
  return async () => {
    const result = await dialog.showOpenDialog(window, {
      title: 'Выбери рабочую папку',
      properties: ['openDirectory', 'createDirectory']
    })
    return result.canceled ? null : (result.filePaths[0] ?? null)
  }
}
