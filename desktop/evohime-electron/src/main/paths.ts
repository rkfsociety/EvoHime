import { join } from 'node:path'

import { app } from 'electron'

/**
 * Filesystem locations owned by the shell.
 *
 * Executable content is only ever loaded from the packaged, signed resources;
 * writable directories (`%LOCALAPPDATA%`, workspaces) hold data and diagnostics
 * only (plan 0, stage 4).
 */

export function resourcePath(name: string): string {
  return app.isPackaged
    ? join(process.resourcesPath, name)
    : join(app.getAppPath(), 'resources', name)
}

/** `%LOCALAPPDATA%\EvoHime` unless the launcher overrode the data directory. */
export function dataDirectory(environment: NodeJS.ProcessEnv = process.env): string {
  const override = environment['EVOHIME_DATA_DIR']?.trim()
  if (override) {
    return override
  }
  const localAppData = environment['LOCALAPPDATA']?.trim()
  return localAppData ? join(localAppData, 'EvoHime') : app.getPath('userData')
}

export function logDirectory(environment: NodeJS.ProcessEnv = process.env): string {
  return join(dataDirectory(environment), 'logs')
}
