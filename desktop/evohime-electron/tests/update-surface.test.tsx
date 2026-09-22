/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'

import { disabledUpdateStatus, initialUpdateSteps, updateProgress, type UpdateStatus } from '@shared/update'
import { UpdateIndicator } from '../src/renderer/src/UpdateIndicator'
import { UpdaterApp } from '../src/renderer/src/UpdaterApp'
import type { UpdaterUiStatus } from '../src/shared/updater'

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
  })
})
