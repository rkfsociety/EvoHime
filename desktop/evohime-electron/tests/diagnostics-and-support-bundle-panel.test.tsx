// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import type { EvoHimeApiV1 } from '../src/shared/api'
import { DiagnosticsAndSupportBundlePanel } from '../src/renderer/src/DiagnosticsAndSupportBundlePanel'

afterEach(() => {
  cleanup()
  delete (window as unknown as { evohime?: unknown }).evohime
})

describe('diagnostics and support bundle panel', () => {
  it('sends the redacted bundle only after explicit confirmation', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { url: 'https://github.com/rkfsociety/EvoHime/issues/2' } })
    const openExternal = vi.fn().mockResolvedValue(true)
    const api = { apiVersion: 1, invoke, subscribe: () => () => {}, writeClipboardText: vi.fn(), openExternal, pathForFile: () => '' } as unknown as EvoHimeApiV1
    Object.defineProperty(window, 'evohime', { value: { v1: api }, configurable: true })
    vi.spyOn(window, 'confirm').mockReturnValue(true)

    render(<DiagnosticsAndSupportBundlePanel connection="disconnected" events={[]} />)
    await userEvent.click(screen.getByRole('button', { name: 'Отправить в GitHub issue' }))

    expect(invoke).toHaveBeenCalledWith('shell.submitDiagnostics', {})
    expect(openExternal).toHaveBeenCalledWith('https://github.com/rkfsociety/EvoHime/issues/2')
    expect(screen.getByRole('alert').textContent).toContain('Issue создан и открыт')
  })
})
