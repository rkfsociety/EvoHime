/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'

import { disabledUpdateStatus, initialUpdateSteps, updateProgress, type UpdateStatus } from '@shared/update'
import { UpdateIndicator } from '../src/renderer/src/UpdateIndicator'
import { UpdaterApp } from '../src/renderer/src/UpdaterApp'
import { updaterUiStatus, type UpdaterUiStatus } from '../src/shared/updater'

function status(overrides: Partial<UpdateStatus> = {}): UpdateStatus {
  return { ...disabledUpdateStatus('main'), phase: 'idle', ...overrides }
}

function installApi(): ReturnType<typeof vi.fn> {
  const invoke = vi.fn().mockResolvedValue({ ok: true, value: status() })
  ;(window as unknown as { evohime: unknown }).evohime = {
    v1: { apiVersion: 1, invoke, subscribe: () => () => {}, writeClipboardText: async () => true }
  }
  return invoke
}

function installUpdaterApi(view: UpdaterUiStatus): void {
  ;(window as unknown as { evohimeUpdater: unknown }).evohimeUpdater = {
    getStatus: vi.fn().mockResolvedValue(view),
    subscribe: vi.fn(() => () => {}),
    close: vi.fn().mockResolvedValue(undefined),
    minimize: vi.fn().mockResolvedValue(undefined)
  }
}

function updaterStatus(overrides: Partial<UpdaterUiStatus> = {}): UpdaterUiStatus {
  return {
    phase: 'ready',
    heading: 'Обновление завершено',
    badge: 'Готово к запуску',
    message: 'Все компоненты EvoHime установлены и готовы к работе.',
    detail: '',
    percent: null,
    modules: [{
      id: 'shell-host',
      label: 'Оболочка Electron',
      installed: '0.0.00068',
      available: null,
      summary: 'Работает в установленной версии.'
    }],
    canApply: false,
    ...overrides
  }
}

afterEach(() => {
  cleanup()
  delete (window as unknown as { evohime?: unknown }).evohime
  delete (window as unknown as { evohimeUpdater?: unknown }).evohimeUpdater
})

describe('update progress', () => {
  it('stays unknown until a step starts, then follows the checklist', () => {
    const steps = initialUpdateSteps()
    expect(updateProgress(status({ steps }))).toBeNull()

    const active = steps.map((step, index) =>
      index === 0 ? { ...step, state: 'done' as const } : step
    )
    expect(updateProgress(status({ steps: active }))).toBeCloseTo(1 / steps.length)

    const allSkipped = steps.map((step) => ({ ...step, state: 'skipped' as const }))
    expect(updateProgress(status({ steps: allSkipped }))).toBeNull()
  })
})

describe('sidebar update indicator', () => {
  it('shows the download percentage directly inside the circular control', () => {
    installApi()
    render(
      <UpdateIndicator
        status={status({
          phase: 'preparing',
          message: 'Скачиваю проверенный установщик…',
          downloadProgress: 0.3,
          steps: initialUpdateSteps().slice(0, 2).map((step, index) => ({
            ...step,
            state: index === 0 ? 'active' as const : 'pending' as const
          }))
        })}
      />
    )

    expect(screen.getByText('30%')).toBeTruthy()
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('does not show 100% when steps are skipped and downloadProgress is unknown', () => {
    installApi()
    render(
      <UpdateIndicator
        status={status({
          phase: 'applying',
          message: 'Проверяю модули…',
          downloadProgress: null,
          steps: initialUpdateSteps().map((step) => ({ ...step, state: 'skipped' as const }))
        })}
      />
    )

    expect(screen.getByText('…')).toBeTruthy()
    expect(screen.queryByText('100%')).toBeNull()
  })

  it('renders step status bar, progress bar, and active module progress inside the dialog', () => {
    installApi()
    render(
      <UpdateIndicator
        status={status({
          phase: 'applying',
          message: 'Скачивание core — 46%',
          downloadProgress: 0.46,
          steps: initialUpdateSteps().map((step) => ({ ...step, state: 'skipped' as const })),
          availableModules: ['shell-host', 'core'],
          installedModules: { 'shell-host': '0.0.000146', core: '0.0.000522' },
          availableModuleVersions: { 'shell-host': '0.0.000147', core: '0.0.000524' }
        })}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'Прогресс скачивания обновления' }))
    const progressbar = screen.getByRole('progressbar', { name: 'Прогресс обновления' })
    expect(progressbar.getAttribute('aria-valuenow')).toBe('46')
    expect(screen.getByText('Проверка')).toBeTruthy()
    expect(screen.getByText('Загрузка')).toBeTruthy()
    expect(screen.getByText('Установка')).toBeTruthy()
    const coreRow = screen.getByText('core').closest('li')
    expect(coreRow?.className).toContain('update-confirm__module--active')
    expect(coreRow?.textContent).toContain('46%')
  })

  it('opens the module version details after the update is ready', () => {
    const invoke = installApi()
    render(<UpdateIndicator status={status({
      phase: 'ready',
      restartRequired: true,
      downloadProgress: 1,
      availableModules: ['shell-host'],
      installedModules: { 'shell-host': '1.2.0' },
      availableModuleVersions: { 'shell-host': '1.3.0' }
    })} />)

    fireEvent.click(screen.getByRole('button', { name: 'Подтвердить установку обновления' }))
    expect(screen.getByRole('dialog', { name: 'Подтверждение обновления' })).toBeTruthy()
    expect(screen.getByText('shell-host')).toBeTruthy()
    expect(screen.getByText('1.2.0 → 1.3.0')).toBeTruthy()

    fireEvent.click(screen.getByRole('button', { name: 'Обновить' }))
    expect(invoke).toHaveBeenCalledWith('update.restart', {})
  })

  it('starts an available production module update from the running shell', () => {
    const invoke = installApi()
    render(<UpdateIndicator status={status({
      phase: 'available',
      message: 'Доступны обновления модулей.',
      availableModules: ['shell-host', 'updater'],
      installedModules: { 'shell-host': '0.0.000068', updater: '0.0.000101' },
      availableModuleVersions: { 'shell-host': '0.0.000073', updater: '0.0.000103' }
    })} />)

    fireEvent.click(screen.getByRole('button', { name: 'Открыть обновление' }))
    fireEvent.click(screen.getByRole('button', { name: 'Обновить сейчас' }))
    expect(invoke).toHaveBeenCalledWith('update.prepareComponents', { selected: ['shell-host', 'updater'] })
  })

  it('keeps the compact control out of the sidebar when there is no update to show', () => {
    const { container } = render(<UpdateIndicator status={status({ phase: 'up-to-date' })} />)
    expect(container.firstChild).toBeNull()
  })
})

describe('standalone updater window', () => {
  it('renders a minimal ready state without launch or module controls', async () => {
    installUpdaterApi(updaterStatus({ detail: 'Recovery: committed; слот A; fallback сохранён' }))
    render(<UpdaterApp />)

    await waitFor(() => expect(screen.getByRole('heading', { name: 'Обновление завершено' })).toBeTruthy())
    expect(screen.getByRole('progressbar')).toBeTruthy()
    expect(screen.getByText('100%')).toBeTruthy()
    expect(screen.queryByText('Компоненты')).toBeNull()
    expect(screen.queryByText(/Запустить/)).toBeNull()
    expect(screen.queryByRole('button', { name: 'Закрыть' })).toBeTruthy()
    expect(screen.queryByText(/Recovery:/)).toBeNull()
    expect(screen.queryByText('Надёжный запуск')).toBeNull()
  })

  it('shows useful detail only when the updater needs attention', async () => {
    installUpdaterApi(updaterStatus({
      phase: 'failed',
      heading: 'Обновление не завершено',
      badge: 'Ошибка',
      message: 'Не удалось проверить обновление.',
      detail: 'Проверьте подключение и повторите попытку.'
    }))
    render(<UpdaterApp />)

    await waitFor(() => expect(screen.getByRole('heading', { name: 'Обновление не завершено' })).toBeTruthy())
    expect(screen.getByText('Проверьте подключение и повторите попытку.')).toBeTruthy()
    expect(document.querySelector('.updater-status-icon')?.textContent).toBe('!')
  })

  it('shows a spinner during checking and progress ring while applying, reserving ✓ for ready', async () => {
    installUpdaterApi(updaterStatus({
      phase: 'checking',
      heading: 'Проверяю обновления',
      badge: 'Проверка',
      message: 'Сверяю версии…',
      percent: null
    }))
    const { unmount } = render(<UpdaterApp />)

    await waitFor(() => expect(screen.getByRole('heading', { name: 'Проверяю обновления' })).toBeTruthy())
    expect(screen.queryByText('✓')).toBeNull()
    expect(document.querySelector('.updater-status-spinner--indeterminate')).toBeTruthy()
    unmount()

    installUpdaterApi(updaterStatus({
      phase: 'applying',
      heading: 'Устанавливаю обновление',
      badge: 'Установка',
      message: 'Загрузка…',
      percent: 28
    }))
    render(<UpdaterApp />)

    await waitFor(() => expect(screen.getByRole('heading', { name: 'Устанавливаю обновление' })).toBeTruthy())
    expect(screen.queryByText('✓')).toBeNull()
    expect(document.querySelector('.updater-status-spinner')).toBeTruthy()
    expect(document.querySelector('.updater-status-spinner--indeterminate')).toBeNull()
  })

  it('shows ✓ in the status badge when ready', async () => {
    installUpdaterApi(updaterStatus({ phase: 'ready' }))
    render(<UpdaterApp />)

    await waitFor(() => expect(screen.getByRole('heading', { name: 'Обновление завершено' })).toBeTruthy())
    expect(document.querySelector('.updater-status-icon')?.textContent).toBe('✓')
    expect(document.querySelector('.updater-status-spinner')).toBeNull()
  })

  it('shows telemetry and version without duplicating download wording or percent', async () => {
    const mb = 1024 * 1024
    installUpdaterApi(updaterUiStatus(status({
      phase: 'applying',
      message: 'Скачивание shell-host — 85%',
      activeModule: 'shell-host',
      downloadedBytes: Math.round(24.3 * mb),
      totalBytes: Math.round(43.1 * mb),
      downloadSpeedBytesPerSec: Math.round(4.8 * mb),
      availableModules: ['shell-host'],
      installedModules: { 'shell-host': '1.4.2' },
      availableModuleVersions: { 'shell-host': '1.5.0' }
    })))
    render(<UpdaterApp />)

    await waitFor(() => expect(screen.getByRole('heading', { name: 'Скачиваю обновление' })).toBeTruthy())
    expect(document.querySelector('.updater-message')?.textContent).toBe('shell-host · 4.8 МБ/с · осталось ~4 с')
    expect(document.querySelector('.updater-progress__label')?.textContent).toBe('85%')
    expect(document.querySelector('.updater-progress__badge')?.textContent).toBe('v1.4.2 → v1.5.0 · 24.3 / 43.1 МБ')
    expect(document.querySelector('[data-step-id="download"]')?.getAttribute('data-step-state')).toBe('active')
    expect(document.querySelector('[data-step-id="install"]')?.getAttribute('data-step-state')).toBe('pending')
  })

  it('tracks live update steps and updatable module chips without decorative glyphs or footer', async () => {
    installUpdaterApi(updaterStatus({
      phase: 'applying',
      heading: 'Устанавливаю обновление',
      badge: 'Установка',
      message: 'Установка компонентов…',
      percent: 85,
      modules: [
        { id: 'core', label: 'EvoHime Core', installed: '1.0.0', available: '1.0.1', summary: 'Обновление' },
        { id: 'shell-host', label: 'Оболочка Electron', installed: '1.0.0', available: '1.0.1', summary: 'Обновление' },
        { id: 'supervisor', label: 'Supervisor', installed: '1.0.0', available: null, summary: 'Без изменений' }
      ]
    }))
    const { unmount } = render(<UpdaterApp />)

    await waitFor(() => expect(screen.getByRole('heading', { name: 'Устанавливаю обновление' })).toBeTruthy())
    expect(document.querySelector('[data-step-id="check"]')?.getAttribute('data-step-state')).toBe('done')
    expect(document.querySelector('[data-step-id="check"] [data-step-icon="done"]')).toBeTruthy()
    expect(document.querySelector('[data-step-id="download"]')?.getAttribute('data-step-state')).toBe('done')
    expect(document.querySelector('[data-step-id="download"] [data-step-icon="done"]')).toBeTruthy()
    expect(document.querySelector('[data-step-id="install"]')?.getAttribute('data-step-state')).toBe('active')
    expect(document.querySelector('[data-step-id="install"] [data-step-icon="done"]')).toBeNull()

    expect(screen.getByText('EvoHime Core')).toBeTruthy()
    expect(screen.getByText('Оболочка Electron')).toBeTruthy()
    expect(screen.queryByText('Supervisor')).toBeNull()
    expect(document.querySelector('[data-module-id="core"]')?.getAttribute('data-module-state')).toBe('pending')
    unmount()

    installUpdaterApi(updaterUiStatus(status({
      phase: 'applying',
      message: 'Скачивание shell-host — 50%',
      activeModule: 'shell-host',
      completedModules: ['core'],
      availableModules: ['core', 'shell-host', 'ui-bundle'],
      installedModules: { core: '1.0.0', 'shell-host': '1.0.0', 'ui-bundle': '1.0.0' },
      availableModuleVersions: { core: '1.0.1', 'shell-host': '1.0.1', 'ui-bundle': '1.0.1' }
    })))
    render(<UpdaterApp />)
    await waitFor(() => expect(document.querySelector('[data-module-id="core"]')?.getAttribute('data-module-state')).toBe('done'))
    expect(document.querySelector('[data-module-id="core"] .updater-module-chip__indicator')?.textContent).toBe('✓')
    expect(document.querySelector('[data-module-id="shell-host"]')?.getAttribute('data-module-state')).toBe('active')
    expect(document.querySelector('[data-module-id="ui-bundle"]')?.getAttribute('data-module-state')).toBe('pending')
    expect(screen.queryByText(/[♢ϟ♡]/)).toBeNull()
    expect(screen.queryByText(/РАЗВИВАЕМСЯ ВМЕСТЕ/)).toBeNull()
    expect(screen.getByTestId('updater-hero-art')).toBeTruthy()
    expect(screen.getByText('Центр обновлений')).toBeTruthy()
    expect(screen.queryByText(/AI COMPANION FOR A BRIGHTER YOU/)).toBeNull()
    expect(document.querySelector('.updater-hero-glow')).toBeNull()
  })

  it('supports hiding to tray, toggling changelog, and retrying a network error', async () => {
    const hideToTray = vi.fn(async () => {})
    const retry = vi.fn(async () => {})
    window.evohimeUpdater = {
      getStatus: vi.fn(async () => updaterUiStatus(status({
        phase: 'applying',
        message: 'Скачивание shell-host — 30%',
        activeModule: 'shell-host',
        availableModules: ['shell-host'],
        installedModules: { 'shell-host': '1.4.2' },
        availableModuleVersions: { 'shell-host': '1.5.0' },
        availableModuleChanges: { 'shell-host': ['Новый экран обновления'] }
      }))),
      subscribe: vi.fn(() => () => {}),
      close: vi.fn(async () => {}),
      minimize: vi.fn(async () => {}),
      hideToTray,
      retry
    }

    const { unmount } = render(<UpdaterApp />)
    await waitFor(() => expect(screen.getByRole('button', { name: 'Свернуть в трей' })).toBeTruthy())

    fireEvent.click(screen.getByRole('button', { name: 'Свернуть в трей' }))
    expect(hideToTray).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: 'Что нового' }))
    expect(screen.getByText('Новый экран обновления')).toBeTruthy()
    unmount()

    window.evohimeUpdater = {
      getStatus: vi.fn(async () => updaterUiStatus(status({
        phase: 'failed',
        message: 'updater: download shell-host: connection timed out',
        error: 'updater: download shell-host: connection timed out',
        activeModule: 'shell-host'
      }))),
      subscribe: vi.fn(() => () => {}),
      close: vi.fn(async () => {}),
      minimize: vi.fn(async () => {}),
      retry
    }

    render(<UpdaterApp />)
    await waitFor(() => expect(screen.getByText('Не удалось скачать shell-host')).toBeTruthy())
    fireEvent.click(screen.getByRole('button', { name: 'Повторить' }))
    expect(retry).toHaveBeenCalledTimes(1)
  })

  it('renders all updatable modules without truncating into a +N badge', async () => {
    installUpdaterApi(updaterUiStatus(status({
      phase: 'applying',
      message: 'Скачивание shell-host — 30%',
      activeModule: 'shell-host',
      completedModules: ['core', 'supervisor'],
      availableModules: ['shell-host', 'ui-bundle', 'core', 'supervisor', 'transaction'],
      installedModules: { 'shell-host': '1.0.0', 'ui-bundle': '1.0.0', core: '1.0.0', supervisor: '1.0.0', transaction: '1.0.0' },
      availableModuleVersions: { 'shell-host': '1.0.1', 'ui-bundle': '1.0.1', core: '1.0.1', supervisor: '1.0.1', transaction: '1.0.1' }
    })))

    render(<UpdaterApp />)
    await waitFor(() => expect(document.querySelector('[data-module-id="transaction"]')).toBeTruthy())
    expect(document.querySelectorAll('.updater-module-chip')).toHaveLength(5)
    expect(screen.queryByText(/^\+\d+$/)).toBeNull()
  })
})
