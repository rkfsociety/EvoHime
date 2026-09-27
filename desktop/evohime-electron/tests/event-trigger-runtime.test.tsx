// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { ShellEvent } from '../src/shared/api'
import { EventTriggerRuntimePanel } from '../src/renderer/src/EventTriggerRuntimePanel'

afterEach(() => cleanup())

describe('Event Trigger Runtime projection', () => {
  it('loads the scoped Core projection and does not claim unwired sources are available', async () => {
    const listeners: ((event: ShellEvent) => void)[] = []
    const invoke = vi.fn(async (_command: string, payload: unknown) => {
      const requestId = (payload as { requestId: string }).requestId
      queueMicrotask(() => listeners.forEach((listener) => listener({
        kind: 'core-event',
        event: { eventType: 'event_trigger_runtime.result', payload: JSON.stringify({
          request_id: requestId,
          operation: 'list',
          status: 'ok',
          error_code: '',
          triggers: [],
          workflow_templates: [],
          sources: { local_workspace_event: 'unavailable', system_event: 'available', integration_webhook: 'unavailable' }
        }) }
      } as unknown as ShellEvent)))
      return { ok: true, value: { accepted: true } }
    })
    const subscribe = (listener: (event: ShellEvent) => void) => {
      listeners.push(listener)
      return () => undefined
    }
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke, subscribe } } })
    render(<EventTriggerRuntimePanel workspace="C:\\workspace" />)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('eventTriggerRuntime.list', expect.any(Object)))
    expect(await screen.findByText(/Системные события задач Core и изменения файлов в Windows можно включить/)).toBeTruthy()
    expect(document.body.textContent).not.toContain('mvp_sources')
  })

  it('saves a draft with a pinned workflow binding even when the producer is unavailable', async () => {
    const listeners: ((event: ShellEvent) => void)[] = []
    const invoke = vi.fn(async (command: string, payload: unknown) => {
      const requestId = (payload as { requestId: string }).requestId
      const operation = command === 'eventTriggerRuntime.list' ? 'list' : (payload as { operation: string }).operation
      queueMicrotask(() => listeners.forEach((listener) => listener({
        kind: 'core-event',
        event: { eventType: 'event_trigger_runtime.result', payload: JSON.stringify({
          request_id: requestId,
          operation,
          status: 'ok',
          error_code: '',
          triggers: [],
          workflow_templates: [{
            template_id: 'repository-research',
            version: 1,
            display_name: 'Исследование репозитория',
            execution_hash: 'a'.repeat(64),
            inputs: [{ name: 'scope', title: 'Область', required: true, max_chars: 512 }]
          }],
          sources: { local_workspace_event: 'unavailable', system_event: 'unavailable' },
          trigger: operation === 'save' ? JSON.parse((payload as { payload: string }).payload) : undefined,
          version: operation === 'save' ? 1 : undefined
        }) }
      } as unknown as ShellEvent)))
      return { ok: true, value: { accepted: true } }
    })
    const subscribe = (listener: (event: ShellEvent) => void) => {
      listeners.push(listener)
      return () => undefined
    }
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke, subscribe } } })
    render(<EventTriggerRuntimePanel workspace="C:\\workspace" />)

    await waitFor(() => expect(invoke).toHaveBeenCalledWith('eventTriggerRuntime.list', expect.any(Object)))
    fireEvent.click(await screen.findByRole('button', { name: 'Новое правило' }))
    fireEvent.change(screen.getByLabelText('Mapping входов workflow'), { target: { value: '{"scope":"path"}' } })
    fireEvent.click(screen.getByRole('button', { name: 'Сохранить черновик' }))

    await waitFor(() => expect(invoke).toHaveBeenCalledWith('eventTriggerRuntime.command', expect.objectContaining({ operation: 'save' })))
    const saveRequest = invoke.mock.calls.find(([command]) => command === 'eventTriggerRuntime.command')?.[1] as { payload: string }
    const saved = JSON.parse(saveRequest.payload) as { state: string; workflow: { workflow_version: number; execution_hash: string }; mapping: Record<string, string> }
    expect(saved.state).toBe('draft')
    expect(saved.workflow.workflow_version).toBe(1)
    expect(saved.workflow.execution_hash).toBe('a'.repeat(64))
    expect(saved.mapping).toEqual({ scope: 'path' })
  })
})
