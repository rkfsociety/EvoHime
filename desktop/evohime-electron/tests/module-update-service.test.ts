import { describe, expect, it, vi } from 'vitest'

const { spawnMock } = vi.hoisted(() => ({
  spawnMock: vi.fn(() => ({ once: vi.fn(), unref: vi.fn() }))
}))

vi.mock('node:child_process', () => ({ spawn: spawnMock }))

import { ModuleUpdateService } from '../src/main/update/module-update-service'

describe('ModuleUpdateService', () => {
  it('starts the Windows worker through the shell with quoted paths', async () => {
    const service = new ModuleUpdateService({
      dataDirectory: 'C:\\data\\EvoHime',
      branch: 'main',
      enabled: true,
      updaterPath: 'C:\\Program Files\\EvoHime\\evohime-updater.exe',
      installDirectory: 'C:\\Program Files\\EvoHime',
      emit: () => {},
      intervalMs: 60_000
    })

    await service.prepareComponents(['listener-runtime'])

    const windows = process.platform === 'win32'
    expect(spawnMock).toHaveBeenCalledWith(
      windows ? '"C:\\Program Files\\EvoHime\\evohime-updater.exe"' : 'C:\\Program Files\\EvoHime\\evohime-updater.exe',
      windows ? ['--apply', '--install-dir', '"C:\\Program Files\\EvoHime"'] : ['--apply', '--install-dir', 'C:\\Program Files\\EvoHime'],
      expect.objectContaining({
        detached: true,
        windowsHide: true,
        shell: windows,
        stdio: 'ignore'
      })
    )
  })
})
