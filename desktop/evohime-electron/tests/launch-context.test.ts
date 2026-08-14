import { describe, expect, it } from 'vitest'

import { hasLiveSupervisor, readLaunchContext } from '../src/main/ipc/launch-context'

/**
 * The shell starts a supervisor unless one is proven to be running.
 *
 * A `session.json` left behind by a crashed supervisor — or written by Core in
 * its console mode, which records no pid — used to be read as "a supervisor
 * already owns this session". The shell then never started one and reconnected
 * forever to a pipe nobody served.
 */

const SECRET = '8f75b0106a1459fda14d7a90e0aedc6413c87ae27dcbdd4b38bdcfe9ed695e40'

function contextFrom(file: Record<string, unknown>) {
  return readLaunchContext(
    { LOCALAPPDATA: 'C:\\Users\\eva\\AppData\\Local' },
    () => 'fixed-id',
    () => JSON.stringify(file)
  )
}

const base = {
  pipe_name: '\\\\.\\pipe\\evohime-core-cfba6739a4d785dab935b85d81a9c25f',
  secret: SECRET
}

describe('supervisor liveness', () => {
  it('trusts a context whose recorded supervisor is still running', () => {
    const context = contextFrom({ ...base, supervisor_pid: 4242 })

    expect(context.developerLaunch).toBe(false)
    expect(hasLiveSupervisor(context, (pid) => pid === 4242)).toBe(true)
  })

  it('starts a supervisor when the context records no pid', () => {
    const context = contextFrom(base)

    expect(context.supervisorPid).toBeUndefined()
    expect(hasLiveSupervisor(context, () => true)).toBe(false)
  })

  it('starts a supervisor when the recorded process is gone', () => {
    const context = contextFrom({ ...base, supervisor_pid: 4242 })

    expect(hasLiveSupervisor(context, () => false)).toBe(false)
  })

  it('starts a supervisor for a developer launch without any context', () => {
    const context = readLaunchContext(
      { LOCALAPPDATA: 'C:\\Users\\eva\\AppData\\Local' },
      () => 'fixed-id',
      () => {
        throw new Error('missing')
      }
    )

    expect(context.developerLaunch).toBe(true)
    expect(hasLiveSupervisor(context, () => true)).toBe(false)
  })
})
