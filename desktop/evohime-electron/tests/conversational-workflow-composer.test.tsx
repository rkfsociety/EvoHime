// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ConversationalWorkflowComposerPanel } from '../src/renderer/src/ConversationalWorkflowComposerPanel'
import type { EvoHimeApiV1 } from '../src/shared/api'

const invoke = vi.fn()

beforeEach(() => {
  invoke.mockResolvedValue({ ok: true, value: { accepted: true } })
  const api: EvoHimeApiV1 = { apiVersion: 1, invoke: invoke as EvoHimeApiV1['invoke'], subscribe: () => () => {}, writeClipboardText: async () => true, openExternal: async () => true, pathForFile: () => '' }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})
afterEach(() => { cleanup(); invoke.mockReset() })

describe('ConversationalWorkflowComposerPanel', () => {
  it('отправляет natural-language request только через typed bridge', async () => {
    render(<ConversationalWorkflowComposerPanel connection="connected" events={[]} workspace="C:\\work" />)
    fireEvent.change(screen.getByLabelText('Описание workflow'), { target: { value: 'Собери workflow для проверки изменений' } })
    fireEvent.click(screen.getByRole('button', { name: 'Создать draft' }))
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('workflowComposer.command', expect.objectContaining({ operation: 'generate', payload: 'Собери workflow для проверки изменений' })))
  })
})
