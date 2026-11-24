import { describe, expect, it } from 'vitest'

import { updaterUiStatus } from '../src/shared/updater'
import type { UpdateStatus } from '../src/shared/update'

function status(patch: Partial<UpdateStatus> = {}): UpdateStatus {
  return {
    phase: 'up-to-date',
    blocking: false,
    message: 'Все модули актуальны.',
    detail: '',
    steps: [],
    installedCommit: null,
    installedModules: { core: '1.2.0' },
    remoteCommit: null,
    branch: 'main',
    error: null,
    checkedAtMs: Date.now(),
    downloadProgress: null,
    selectedComponents: [],
    availableModules: [],
    availableModuleVersions: {},
    availableModuleSummaries: {},
    availableModuleChanges: {},
    downloadedBytes: null,
    totalBytes: null,
    restartRequired: false,
    evidence: [],
    ...patch
  }
}

describe('updaterUiStatus', () => {
  it('renders a ready state with the canonical module list', () => {
    const view = updaterUiStatus(status())

    expect(view.phase).toBe('ready')
    expect(view.heading).toBe('Обновление завершено')
    expect(view.canApply).toBe(false)
    expect(view.modules.map((module) => module.id)).toEqual(['core', 'shell-host', 'supervisor', 'listener'])
  })

  it('preserves available versions and exposes the apply action', () => {
    const view = updaterUiStatus(status({
      phase: 'available',
      message: 'Доступны обновления модулей.',
      availableModules: ['core'],
      availableModuleVersions: { core: '1.3.0' },
      availableModuleSummaries: { core: 'Исправлена стабильность агента.' },
      installedModules: { core: '1.2.0' }
    }))

    expect(view.phase).toBe('available')
    expect(view.canApply).toBe(true)
    expect(view.modules[0]).toMatchObject({
      id: 'core',
      label: 'EvoHime Core',
      installed: '1.2.0',
      available: '1.3.0',
      summary: 'Исправлена стабильность агента.'
    })
  })

  it('extracts bounded progress from the worker status and formats download telemetry without tautology', () => {
    const first = updaterUiStatus(status({ phase: 'applying', message: 'Скачивание core — 74%' }))
    expect(first.percent).toBe(74)
    expect(first.message).toBe('core')

    const clamped = updaterUiStatus(status({ phase: 'applying', message: 'Скачивание core — 144%' }))
    expect(clamped.percent).toBe(100)
    expect(clamped.message).toBe('core')

    const preparing = updaterUiStatus(status({ phase: 'applying', message: 'Подготовка' }))
    expect(preparing.percent).toBeNull()
    expect(preparing.message).toBe('Подготовка')
  })

  it('shows download speed, ETA, size and module version without repeating download wording', () => {
    const mb = 1024 * 1024
    const download = updaterUiStatus(status({
      phase: 'applying',
      message: 'Скачивание shell-host — 56%',
      activeModule: 'shell-host',
      downloadedBytes: Math.round(24.3 * mb),
      totalBytes: Math.round(43.1 * mb),
      downloadSpeedBytesPerSec: Math.round(4.8 * mb),
      availableModules: ['shell-host'],
      installedModules: { 'shell-host': '1.4.2' },
      availableModuleVersions: { 'shell-host': '1.5.0' }
    }))

    expect(download.heading).toBe('Скачиваю обновление')
    expect(download.message).toBe('shell-host · 4.8 МБ/с · осталось ~4 с')
    expect(download.badge).toBe('v1.4.2 → v1.5.0 · 24.3 / 43.1 МБ')

    const withoutSpeed = updaterUiStatus(status({
      phase: 'applying',
      message: 'Скачивание shell-host — 56%',
      activeModule: 'shell-host',
      downloadedBytes: Math.round(24.3 * mb),
      totalBytes: Math.round(43.1 * mb),
      availableModules: ['shell-host'],
      installedModules: { 'shell-host': '1.4.2' },
      availableModuleVersions: { 'shell-host': '1.5.0' }
    }))

    expect(withoutSpeed.message).toBe('shell-host · 24.3 / 43.1 МБ')
    expect(withoutSpeed.badge).toBe('v1.4.2 → v1.5.0')

    const minimal = updaterUiStatus(status({ phase: 'applying', message: 'Скачивание shell-host — 60%' }))
    expect(minimal.heading).toBe('Скачиваю обновление')
    expect(minimal.badge).toBe('Обновление')
    expect(minimal.message).toBe('shell-host')

    const install = updaterUiStatus(status({ phase: 'applying', message: 'Установка компонентов — 90%' }))

    expect(install.heading).toBe('Устанавливаю обновление')
    expect(install.badge).toBe('Установка')

    const preparing = updaterUiStatus(status({ phase: 'applying', message: 'Подготовка' }))

    expect(preparing.heading).toBe('Устанавливаю обновление')
    expect(preparing.badge).toBe('Установка')
  })

  it('never shows a ready state when the worker supplied an error', () => {
    const view = updaterUiStatus(status({
      phase: 'up-to-date',
      error: 'updater: manifest core: GitHub вернул пустой ответ вместо JSON'
    }))

    expect(view.phase).toBe('failed')
    expect(view.heading).toBe('Обновление не завершено')
    expect(view.badge).toBe('Ошибка')
  })

  it('keeps the explicit failed worker phase failed even when updates are listed', () => {
    const view = updaterUiStatus(status({
      phase: 'failed',
      message: 'updater: transaction worker завершился с кодом 1: invalid shell-host archive',
      error: 'updater: transaction worker завершился с кодом 1: invalid shell-host archive',
      availableModules: ['shell-host'],
      availableModuleVersions: { 'shell-host': '0.0.000053' }
    }))

    expect(view.phase).toBe('failed')
    expect(view.canApply).toBe(false)
    expect(view.message).toContain('invalid shell-host archive')
  })

  it('tracks module chip states across done, active, pending, and error', () => {
    const downloading = updaterUiStatus(status({
      phase: 'applying',
      message: 'Скачивание ui-bundle — 40%',
      activeModule: 'ui-bundle',
      completedModules: ['core'],
      availableModules: ['core', 'ui-bundle', 'shell-host'],
      availableModuleVersions: {
        core: '1.0.1',
        'ui-bundle': '1.0.1',
        'shell-host': '1.0.1'
      }
    }))

    expect(downloading.modules.map((item) => [item.id, item.state])).toEqual([
      ['core', 'done'],
      ['ui-bundle', 'active'],
      ['shell-host', 'pending']
    ])

    const failed = updaterUiStatus(status({
      phase: 'failed',
      message: 'Ошибка скачивания ui-bundle',
      error: 'Ошибка скачивания ui-bundle',
      activeModule: 'ui-bundle',
      completedModules: ['core'],
      availableModules: ['core', 'ui-bundle', 'shell-host'],
      availableModuleVersions: {
        core: '1.0.1',
        'ui-bundle': '1.0.1',
        'shell-host': '1.0.1'
      }
    }))

    expect(failed.modules.map((item) => [item.id, item.state])).toEqual([
      ['core', 'done'],
      ['ui-bundle', 'error'],
      ['shell-host', 'pending']
    ])
  })
})
