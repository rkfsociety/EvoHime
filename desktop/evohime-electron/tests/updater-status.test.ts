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

  it('extracts bounded progress from the worker status and strips the trailing percent from message', () => {
    const first = updaterUiStatus(status({ phase: 'applying', message: 'Скачивание core — 74%' }))
    expect(first.percent).toBe(74)
    expect(first.message).toBe('Скачивание core')

    const clamped = updaterUiStatus(status({ phase: 'applying', message: 'Скачивание core — 144%' }))
    expect(clamped.percent).toBe(100)
    expect(clamped.message).toBe('Скачивание core')

    const preparing = updaterUiStatus(status({ phase: 'applying', message: 'Подготовка' }))
    expect(preparing.percent).toBeNull()
    expect(preparing.message).toBe('Подготовка')
  })

  it('labels the download sub-step of the applying phase as download, not install', () => {
    const download = updaterUiStatus(status({ phase: 'applying', message: 'Скачивание shell-host — 60%' }))

    expect(download.heading).toBe('Скачиваю обновление')
    expect(download.badge).toBe('Загрузка')

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
})
