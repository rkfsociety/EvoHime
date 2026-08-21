import { describe, expect, it } from 'vitest'

import { supervisorEnvironment } from '../src/main/supervisor-env'

describe('supervisor environment', () => {
  it('passes the listener the same tools directory used by runtime installation', () => {
    expect(
      supervisorEnvironment(
        { PATH: 'C:\\tools', EVOHIME_DATA_DIR: 'C:\\old-data' },
        'C:\\EvoHime\\evohime-core.exe',
        'C:\\Users\\roman\\AppData\\Local\\EvoHime'
      )
    ).toMatchObject({
      PATH: 'C:\\tools',
      EVOHIME_CORE_EXE: 'C:\\EvoHime\\evohime-core.exe',
      EVOHIME_DATA_DIR: 'C:\\Users\\roman\\AppData\\Local\\EvoHime',
      EVOHIME_LISTENER_TOOLS_DIR: 'C:\\Users\\roman\\AppData\\Local\\EvoHime\\tools\\listener'
    })
  })

  it('preserves the inherited core override when no explicit executable is supplied', () => {
    expect(
      supervisorEnvironment(
        { EVOHIME_CORE_EXE: 'C:\\existing\\evohime-core.exe' },
        null,
        'C:\\data'
      ).EVOHIME_CORE_EXE
    ).toBe('C:\\existing\\evohime-core.exe')
  })
})
