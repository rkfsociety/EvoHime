import { joinPath } from './path-utils'

/**
 * Environment shared by the supervisor and its listener child.
 *
 * The Electron runtime installer and the listener must resolve one identical
 * writable tools directory. Passing it explicitly avoids relying on a
 * process-specific LOCALAPPDATA value in packaged or developer launches.
 */
export function supervisorEnvironment(
  base: NodeJS.ProcessEnv,
  coreExecutable: string | null,
  dataDirectory: string
): NodeJS.ProcessEnv {
  return {
    ...base,
    EVOHIME_CORE_EXE: coreExecutable ?? base['EVOHIME_CORE_EXE'],
    EVOHIME_DATA_DIR: dataDirectory,
    EVOHIME_LISTENER_TOOLS_DIR: joinPath(dataDirectory, 'tools', 'listener')
  }
}
