// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { VisualWorkflowBuilderPanel } from '../src/renderer/src/VisualWorkflowBuilderPanel'

const calls: Array<{ command: string; payload: unknown }> = []
const apiResult = { ok: true, value: { accepted: true } } as never

beforeEach(() => {
  calls.length = 0
  const api: EvoHimeApiV1 = { apiVersion: 1, invoke: (async (command: RendererCommand, payload: unknown) => { calls.push({ command, payload }); return apiResult }) as EvoHimeApiV1['invoke'], subscribe: () => () => {}, writeClipboardText: async () => true, openExternal: async () => true, pathForFile: () => '' }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})
afterEach(() => cleanup())

function event(payload: Record<string, unknown>): CoreEvent { return { sequenceId: 1, taskId: '', eventType: 'workflow_builder.result', payload: JSON.stringify(payload) } }

describe('VisualWorkflowBuilderPanel', () => {
  it('отправляет bounded validate через typed bridge', async () => {
    render(<VisualWorkflowBuilderPanel connection="connected" events={[]} workspace="C:\\work" />)
    fireEvent.change(screen.getByLabelText('Workflow draft JSON'), { target: { value: '{"contract_version":"visual-workflow-builder/v1"}' } })
    fireEvent.click(screen.getByRole('button', { name: 'Проверить draft' }))
    await waitFor(() => expect(calls[0].command).toBe('workflowBuilder.command'))
    expect((calls[0].payload as { operation: string }).operation).toBe('validate')
  })

  it('не публикует без Core-issued handoff', () => {
    render(<VisualWorkflowBuilderPanel connection="connected" events={[event({ status: 'valid' })]} workspace="C:\\work" />)
    expect(screen.getByRole('button', { name: 'Опубликовать' }).hasAttribute('disabled')).toBe(true)
  })
})
