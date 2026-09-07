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
    expect(view.heading).toBe('Модули проверены')
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

  it('extracts bounded progress from the worker status', () => {
    expect(updaterUiStatus(status({ phase: 'applying', message: 'Скачивание core — 74%' })).percent).toBe(74)
    expect(updaterUiStatus(status({ phase: 'applying', message: 'Скачивание core — 144%' })).percent).toBe(100)
    expect(updaterUiStatus(status({ phase: 'applying', message: 'Подготовка' })).percent).toBeNull()
  })

  it('never shows a ready state when the worker supplied an error', () => {
    const view = updaterUiStatus(status({
      phase: 'up-to-date',
      error: 'updater: manifest core: GitHub вернул пустой ответ вместо JSON'
    }))

    expect(view.phase).toBe('failed')
    expect(view.heading).toBe('Проверка требует внимания')
    expect(view.badge).toBe('Ошибка')
  })
})
