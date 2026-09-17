import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { describe, expect, it, vi } from 'vitest'

const { spawnMock } = vi.hoisted(() => ({
  spawnMock: vi.fn(() => ({ once: vi.fn(), unref: vi.fn() }))
}))

vi.mock('node:child_process', () => ({ spawn: spawnMock }))

import { ModuleUpdateService, shouldApplyBootstrap } from '../src/main/update/module-update-service'

describe('ModuleUpdateService', () => {
  it('routes the launch action into apply while a bootstrap shell is missing', () => {
    expect(shouldApplyBootstrap(false, ['shell-host'])).toBe(true)
    expect(shouldApplyBootstrap(true, ['shell-host'])).toBe(false)
    expect(shouldApplyBootstrap(false, [])).toBe(false)
  })

  it('starts the native worker directly with paths kept as separate arguments', async () => {
    let quitForApply = false
    const service = new ModuleUpdateService({
      dataDirectory: 'C:\\data\\EvoHime',
      branch: 'main',
      enabled: true,
      updaterPath: 'C:\\Program Files\\EvoHime\\evohime-updater.exe',
      installDirectory: 'C:\\Program Files\\EvoHime',
      emit: () => {},
      intervalMs: 60_000,
      quitForApply: () => { quitForApply = true }
    })

    await service.prepareComponents(['listener-runtime'])

    expect(spawnMock).toHaveBeenCalledWith(
      'C:\\Program Files\\EvoHime\\evohime-updater.exe',
      ['--apply', '--install-dir', 'C:\\Program Files\\EvoHime', '--wait-pid', String(process.pid), '--relaunch', 'C:\\Program Files\\EvoHime\\EvoHime.exe'],
      expect.objectContaining({
        detached: true,
        windowsHide: true,
        shell: false,
        stdio: 'ignore'
      })
    )
    expect(quitForApply).toBe(true)
  })

  it('marks a crashed worker failed when no status file was written', async () => {
    let close: ((code: number | null, signal: NodeJS.Signals | null) => void) | undefined
    spawnMock.mockImplementationOnce(() => ({
      once: vi.fn((event: string, listener: (code: number | null, signal: NodeJS.Signals | null) => void) => {
        if (event === 'close') close = listener
      }),
      unref: vi.fn()
    }))
    const service = new ModuleUpdateService({
      dataDirectory: 'C:\\missing-update-state',
      branch: 'main',
      enabled: true,
      updaterPath: 'C:\\EvoHime\\evohime-updater.exe',
      installDirectory: 'C:\\EvoHime',
      emit: () => {},
      intervalMs: 60_000
    })

    await service.prepareComponents(['shell-host'])
    close?.(1, null)

    expect(service.status.phase).toBe('failed')
    expect(service.status.error).toContain('без диагностического статуса')
  })

  it('reads the structured worker error and keeps failed status authoritative', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-module-status-'))
    try {
      const state = join(root, 'update-state')
      mkdirSync(state)
      writeFileSync(join(state, 'updater.json'), JSON.stringify({
        phase: 'failed',
        message: 'Обновление не применено.',
        error: 'updater: transaction worker завершился с кодом 1: invalid shell-host archive',
        available: [{ module: 'shell-host', installed: '0.0.000052', available: '0.0.000053' }]
      }))
      const service = new ModuleUpdateService({
        dataDirectory: root,
        branch: 'main',
        enabled: true,
        updaterPath: 'C:\\EvoHime\\evohime-updater.exe',
        installDirectory: 'C:\\EvoHime',
        emit: () => {},
        intervalMs: 60_000
      })

      await service.runLaunchGate()

      expect(service.status.phase).toBe('failed')
      expect(service.status.message).toBe('Обновление не применено.')
      expect(service.status.error).toContain('invalid shell-host archive')
      service.stop()
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('keeps installed module versions when no update is available', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-module-version-'))
    try {
      const state = join(root, 'update-state')
      const install = join(root, 'install')
      mkdirSync(state)
      mkdirSync(install)
      writeFileSync(join(install, 'evohime.components.json'), JSON.stringify({
        components: [{ id: 'core', version: '0.0.000243' }]
      }))
      writeFileSync(join(state, 'updater.json'), JSON.stringify({
        phase: 'up-to-date',
        message: 'Модули актуальны.',
        available: []
      }))

      const service = new ModuleUpdateService({
        dataDirectory: root,
        branch: 'main',
        enabled: true,
        updaterPath: 'C:\\EvoHime\\evohime-updater.exe',
        installDirectory: install,
        emit: () => {},
        intervalMs: 60_000
      })

      await service.runLaunchGate()

      expect(service.status.installedModules).toEqual({ core: '0.0.000243' })
      service.stop()
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })
})
