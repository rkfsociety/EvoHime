/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'

import { disabledUpdateStatus, initialUpdateSteps, updateProgress, type UpdateStatus } from '@shared/update'
import { UpdateGate } from '../src/renderer/src/UpdateGate'
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

describe('launch gate', () => {
  it('shows the running step and the last build line while it blocks', () => {
    render(
      <UpdateGate
        status={status({
          phase: 'preparing',
          blocking: true,
          message: 'Пересобираю Еву…',
          detail: 'Compiling evohime-core',
          steps: initialUpdateSteps().map((step) =>
            step.id === 'core' ? { ...step, state: 'active' as const } : step
          )
        })}
      />
    )

    expect(screen.getByText('Пересобираю Еву…')).toBeTruthy()
    expect(screen.getByText('Compiling evohime-core')).toBeTruthy()
    expect(within(screen.getByRole('region', { name: 'Текущий этап' })).getByText('Сборка Core')).toBeTruthy()
    expect(within(screen.getByRole('region', { name: 'Текущий этап' })).getByText('Компилирую Rust Core и supervisor.')).toBeTruthy()
    expect(screen.getByText('0 из 6 этапов')).toBeTruthy()
    expect(screen.getByText('main')).toBeTruthy()
    expect(screen.getByRole('progressbar')).toBeTruthy()
    expect(screen.getAllByText('Сборка Core').some((element) => element.closest('li')?.getAttribute('data-state') === 'active')).toBe(true)
  })

  it('stays out of the way when the run is not blocking', () => {
    installApi()
    render(<UpdateGate status={status({ phase: 'preparing', blocking: false })} />)

    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('does not offer the regular shell while a launch update is running', () => {
    render(<UpdateGate status={status({ phase: 'preparing', blocking: true })} />)

    expect(screen.queryByRole('button')).toBeNull()
    expect(screen.getByText('Обычный интерфейс откроется после полного завершения обновления.')).toBeTruthy()
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

describe('commit tracking', () => {
  it('names the commit pair and the branch in the launch gate', () => {
    installApi()
    const commit = 'a'.repeat(40)
    render(
      <UpdateGate
        status={status({
          phase: 'preparing',
          blocking: true,
          installedCommit: commit,
          remoteCommit: commit
        })}
      />
    )

    const commits = screen.getByText((_, element) => element?.classList.contains('update-gate__commits') ?? false)
    expect(commits.textContent).toContain('aaaaaaa → aaaaaaa')
    expect(screen.getByText('main')).toBeTruthy()
  })
})
