/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'

import { disabledUpdateStatus, initialUpdateSteps, updateProgress, type UpdateStatus } from '@shared/update'
import { UpdateBanner } from '../src/renderer/src/UpdateBanner'
import { UpdateGate } from '../src/renderer/src/UpdateGate'
import { UpdateIndicator } from '../src/renderer/src/UpdateIndicator'

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

afterEach(() => {
  cleanup()
  delete (window as unknown as { evohime?: unknown }).evohime
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
    installApi()
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
    expect(screen.getByText('0 из 6 этапов завершено')).toBeTruthy()
    expect(screen.getByText('main')).toBeTruthy()
    expect(screen.getByRole('progressbar')).toBeTruthy()
    expect(screen.getAllByText('Сборка Core').some((element) => element.closest('li')?.getAttribute('data-state') === 'active')).toBe(true)
  })

  it('stays out of the way when the run is not blocking', () => {
    installApi()
    render(<UpdateGate status={status({ phase: 'preparing', blocking: false })} />)

    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('lets the user drop the update and start the installed build', () => {
    const invoke = installApi()
    render(<UpdateGate status={status({ phase: 'preparing', blocking: true })} />)

    fireEvent.click(screen.getByRole('button', { name: 'Пропустить и запустить' }))

    expect(invoke).toHaveBeenCalledWith('update.skip', {})
  })
})

describe('update banner', () => {
  it('says nothing while the installation is current', () => {
    installApi()
    const { container } = render(<UpdateBanner status={status({ phase: 'up-to-date' })} />)

    expect(container.firstChild).toBeNull()
  })

  it('offers the rebuild when the branch moved ahead', () => {
    const invoke = installApi()
    render(<UpdateBanner status={status({ phase: 'available', message: 'Доступно обновление.' })} />)

    fireEvent.click(screen.getByRole('button', { name: 'Обновить' }))

    expect(invoke).toHaveBeenCalledWith('update.prepare', {})
  })

  it('asks for the restart only once a package is staged', () => {
    const invoke = installApi()
    render(
      <UpdateBanner
        status={status({ phase: 'ready', restartRequired: true, message: 'Обновление собрано — нужен перезапуск.' })}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'Перезапустить' }))

    expect(invoke).toHaveBeenCalledWith('update.restart', {})
  })

  it('shows a failed rebuild with a retry instead of a restart', () => {
    installApi()
    render(<UpdateBanner status={status({ phase: 'failed', error: 'Сборка обновления не удалась: cargo' })} />)

    expect(screen.getByText('Сборка обновления не удалась: cargo')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Перезапустить' })).toBeNull()
    expect(screen.getByRole('button', { name: 'Повторить' })).toBeTruthy()
  })

  it('never competes with the blocking gate', () => {
    installApi()
    const { container } = render(<UpdateBanner status={status({ phase: 'preparing', blocking: true })} />)

    expect(container.firstChild).toBeNull()
  })
})

describe('sidebar update indicator', () => {
  it('opens a detailed, closable status popover for a running rebuild', () => {
    installApi()
    render(
      <UpdateIndicator
        status={status({
          phase: 'preparing',
          message: 'Пересобираю Еву…',
          detail: 'Compiling evohime-core',
          steps: initialUpdateSteps().map((step) =>
            step.id === 'core' ? { ...step, state: 'active' as const } : step
          )
        })}
      />
    )

    fireEvent.click(screen.getByRole('button', { name: 'Показать статус фонового обновления' }))
    expect(screen.getByRole('dialog')).toBeTruthy()
    expect(screen.getByText('Compiling evohime-core')).toBeTruthy()
    expect(screen.getByText('Сборка Core').getAttribute('data-state')).toBe('active')

    fireEvent.keyDown(window, { key: 'Escape' })
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('keeps the compact control out of the sidebar when there is no update to show', () => {
    const { container } = render(<UpdateIndicator status={status({ phase: 'up-to-date' })} />)
    expect(container.firstChild).toBeNull()
  })
})

describe('commit tracking', () => {
  it('names the installed and target commits instead of a version number', () => {
    installApi()
    render(
      <UpdateBanner
        status={status({
          phase: 'available',
          message: 'Доступно обновление.',
          installedCommit: 'a'.repeat(40),
          remoteCommit: 'b'.repeat(40)
        })}
      />
    )

    expect(screen.getByText('aaaaaaa → bbbbbbb')).toBeTruthy()
  })

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
