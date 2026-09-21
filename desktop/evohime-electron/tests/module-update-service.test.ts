import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { beforeEach, describe, expect, it, vi } from 'vitest'

const { spawnMock } = vi.hoisted(() => ({
  spawnMock: vi.fn(() => ({
    once: vi.fn((event: string, listener: (code?: number | null, signal?: NodeJS.Signals | null) => void) => {
      if (event === 'spawn') queueMicrotask(() => listener())
      if (event === 'close') queueMicrotask(() => listener(0, null))
    }),
    unref: vi.fn()
  }))
}))

vi.mock('node:child_process', () => ({ spawn: spawnMock }))

import {
  ModuleUpdateService,
  resolveInstalledUpdaterPath,
  shouldApplyBootstrap
} from '../src/main/update/module-update-service'

describe('ModuleUpdateService', () => {
  beforeEach(() => spawnMock.mockClear())

  it('routes the launch action into apply while a bootstrap shell is missing', () => {
    expect(shouldApplyBootstrap(false, ['shell-host'])).toBe(true)
    expect(shouldApplyBootstrap(true, ['shell-host'])).toBe(false)
    expect(shouldApplyBootstrap(false, [])).toBe(false)
  })

  it('resolves the native worker beside the installed shell', () => {
    expect(resolveInstalledUpdaterPath('C:\\Program Files\\EvoHime'))
      .toBe('C:\\Program Files\\EvoHime\\evohime-updater.exe')
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

    const expectedPath = process.platform === 'win32'
      ? '"C:\\Program Files\\EvoHime\\evohime-updater.exe"'
      : 'C:\\Program Files\\EvoHime\\evohime-updater.exe'
    const expectedInstall = process.platform === 'win32'
      ? '"C:\\Program Files\\EvoHime"'
      : 'C:\\Program Files\\EvoHime'
    const expectedRelaunch = process.platform === 'win32'
      ? '"C:\\Program Files\\EvoHime\\EvoHime.exe"'
      : 'C:\\Program Files\\EvoHime\\EvoHime.exe'
    const expectedHealth = process.platform === 'win32'
      ? '"C:\\data\\EvoHime\\update-state\\health.json"'
      : 'C:\\data\\EvoHime\\update-state\\health.json'
    expect(spawnMock).toHaveBeenCalledWith(
      expectedPath,
      ['--apply', '--install-dir', expectedInstall, '--wait-pid', String(process.pid), '--relaunch', expectedRelaunch, '--health-file', expectedHealth],
      expect.objectContaining({
        detached: true,
        windowsHide: true,
        shell: process.platform === 'win32',
        stdio: 'ignore'
      })
    )
    expect(quitForApply).toBe(false)
  })

  it('skips the recursive launch gate after a transaction relaunch', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-post-update-launch-'))
    try {
      const state = join(root, 'update-state')
      mkdirSync(state)
      writeFileSync(join(state, 'updater.json'), JSON.stringify({
        phase: 'available',
        available: [{ module: 'core', installed: '0.0.000243', available: '0.0.000244' }]
      }))
      const service = new ModuleUpdateService({
        dataDirectory: root,
        branch: 'main',
        enabled: true,
        updaterPath: 'C:\\EvoHime\\evohime-updater.exe',
        installDirectory: join(root, 'install'),
        emit: () => {},
        intervalMs: 60_000,
        skipLaunchGate: true
      })

      await expect(service.runLaunchGate()).resolves.toBe('continue')
      expect(spawnMock).not.toHaveBeenCalled()
      service.stop()
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
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

  it('accepts a successful updater self-replacement handoff', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-updater-handoff-'))
    try {
      const state = join(root, 'update-state')
      mkdirSync(state)
      writeFileSync(join(state, 'updater.json'), JSON.stringify({
        phase: 'applying',
        message: 'Загрузка завершена. Перезапускаю updater для применения…',
        requires_exit: true,
        available: [{ module: 'updater', installed: '0.0.000123', available: '0.0.000124' }]
      }))
      let close: ((code: number | null, signal: NodeJS.Signals | null) => void) | undefined
      spawnMock.mockImplementationOnce(() => ({
        once: vi.fn((event: string, listener: (code: number | null, signal: NodeJS.Signals | null) => void) => {
          if (event === 'close') close = listener
        }),
        unref: vi.fn()
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

      await service.prepareComponents(['updater'])
      close?.(0, null)

      expect(service.status.phase).toBe('applying')
      expect(service.status.error).toBeNull()
      service.stop()
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('reads the structured worker error and keeps failed status authoritative', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-module-status-'))
    try {
      const state = join(root, 'update-state')
      mkdirSync(state)
      writeFileSync(join(state, 'updater.json'), JSON.stringify({
        phase: 'failed',
        message: 'Обновление не применено.',
        error: 'updater: встроенное применение модулей не удалось: invalid shell-host archive',
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

  it('automatically applies available modules during the launch gate', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-module-launch-gate-'))
    try {
      const state = join(root, 'update-state')
      const install = join(root, 'install')
      mkdirSync(state)
      mkdirSync(install)
      writeFileSync(join(state, 'updater.json'), JSON.stringify({
        phase: 'available',
        message: 'Доступны обновления модулей.',
        available: [{ module: 'core', installed: '0.0.000243', available: '0.0.000244' }]
      }))
      let quitForApply = false
      const service = new ModuleUpdateService({
        dataDirectory: root,
        branch: 'main',
        enabled: true,
        updaterPath: 'C:\\EvoHime\\evohime-updater.exe',
        installDirectory: install,
        emit: () => {},
        intervalMs: 60_000,
        quitForApply: () => { quitForApply = true }
      })

      await expect(service.runLaunchGate()).resolves.toBe('applying')

      expect(quitForApply).toBe(false)
      expect(spawnMock).toHaveBeenCalledTimes(2)
      const applyCall = spawnMock.mock.calls[1] as unknown as [string, readonly string[]] | undefined
      const expectedInstall = process.platform === 'win32' ? `"${install}"` : install
      const expectedRelaunch = process.platform === 'win32' ? `"${join(install, 'EvoHime.exe')}"` : join(install, 'EvoHime.exe')
      const expectedHealth = process.platform === 'win32' ? `"${join(root, 'update-state', 'health.json')}"` : join(root, 'update-state', 'health.json')
      expect(applyCall?.[1]).toEqual([
        '--apply',
        '--install-dir',
        expectedInstall,
        '--wait-pid',
        String(process.pid),
        '--relaunch',
        expectedRelaunch,
        '--health-file',
        expectedHealth
      ])
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('keeps the updater visible while downloading and exits only for file replacement', async () => {
    const root = mkdtempSync(join(tmpdir(), 'evohime-visible-download-'))
    try {
      const state = join(root, 'update-state')
      const install = join(root, 'install')
      mkdirSync(state)
      mkdirSync(install)
      let quitCount = 0
      const service = new ModuleUpdateService({
        dataDirectory: root,
        branch: 'main',
        enabled: true,
        updaterPath: 'C:\\EvoHime\\evohime-updater.exe',
        installDirectory: install,
        emit: () => {},
        intervalMs: 60_000,
        quitForApply: () => { quitCount += 1 }
      })

      await service.prepareComponents(['updater'])
      writeFileSync(join(state, 'updater.json'), JSON.stringify({
        phase: 'applying',
        message: 'Скачивание updater — 58%',
        requires_exit: false,
        available: [{ module: 'updater', installed: '0.0.000106', available: '0.0.000107' }]
      }))
      ;(service as unknown as { refresh(): void }).refresh()
      expect(service.status.message).toBe('Скачивание updater — 58%')
      expect(quitCount).toBe(0)

      writeFileSync(join(state, 'updater.json'), JSON.stringify({
        phase: 'applying',
        message: 'Загрузка завершена. Перезапускаю updater для применения…',
        requires_exit: true,
        available: [{ module: 'updater', installed: '0.0.000106', available: '0.0.000107' }]
      }))
      ;(service as unknown as { refresh(): void }).refresh()
      ;(service as unknown as { refresh(): void }).refresh()
      expect(quitCount).toBe(1)
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
