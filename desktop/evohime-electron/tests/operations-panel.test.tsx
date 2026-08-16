// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { OperationsPanel } from '../src/renderer/src/OperationsPanel'

const calls: Array<{ command: string; payload: unknown }> = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function event(eventType: string, payload: Record<string, unknown>): CoreEvent {
  return { sequenceId: 0, taskId: '', eventType, payload: JSON.stringify(payload) }
}

/** Metadata exactly as Core emits it: no statement, no provenance body. */
function metadata(id: string, overrides: Record<string, unknown> = {}) {
  return {
    id,
    kind: 'preference',
    canonical_subject: 'язык интерфейса',
    confirmation_state: 'pending_confirmation',
    privacy_class: 'normal',
    source_trust: 'user',
    model_confidence: 0.9,
    verification_confidence: 0,
    validation_status: 'not_required',
    policy_version: 'extraction-policy-v1',
    expires_at_ms: null,
    ...overrides
  }
}

beforeEach(() => {
  calls.length = 0
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      if (command === 'workspace.list') {
        return ok({ selected: 'G:/github/EvoHime', options: [] })
      }
      return ok({ accepted: true })
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

describe('operations panel', () => {
  it('asks Core for the pending queue and conflicts once the workspace is known', async () => {
    render(<OperationsPanel connection="connected" events={[]} />)
    await waitFor(() => {
      expect(calls.map((call) => call.command)).toContain('core.listMemoryPending')
    })
    const request = calls.find((call) => call.command === 'core.listMemoryPending')?.payload
    expect(request).toMatchObject({ scopeKind: 'project', workspacePath: 'G:/github/EvoHime' })
    expect(calls.map((call) => call.command)).toContain('core.getMemoryConflicts')
  })

  it('shows counters and confirms a selected candidate with an approval token', async () => {
    render(
      <OperationsPanel
        connection="connected"
        events={[
          event('memory.pending', {
            records: [metadata('cand-1')],
            counts: { pending_confirmation: 1, confirmed: 4, expired: 2 }
          })
        ]}
      />
    )
    expect(await screen.findByText('ждут решения')).toBeTruthy()
    expect(screen.getByText(/4 активных · 2 истекло/)).toBeTruthy()

    await userEvent.click(screen.getByRole('checkbox', { name: 'предпочтение' }))
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить выбранные' }))

    const confirm = calls.find((call) => call.command === 'core.confirmMemory')
    expect(confirm).toBeTruthy()
    const payload = confirm?.payload as { ids: string[]; approvalId: string; idempotencyKey: string }
    expect(payload.ids).toEqual(['cand-1'])
    expect(payload.approvalId.length).toBeGreaterThan(0)
    expect(payload.idempotencyKey.length).toBeGreaterThan(0)
  })

  it('never renders a body and marks sensitive candidates as hidden', async () => {
    render(
      <OperationsPanel
        connection="connected"
        events={[
          event('memory.pending', {
            records: [metadata('cand-2', { privacy_class: 'sensitive' })],
            counts: { pending_confirmation: 1 }
          })
        ]}
      />
    )
    expect(await screen.findByText(/содержимое скрыто/)).toBeTruthy()
    // The panel only ever receives metadata, so nothing statement-shaped can
    // appear even for a normal-privacy record.
    expect(screen.queryByText(/statement/i)).toBeNull()
  })

  it('edits a candidate without confirming it', async () => {
    render(
      <OperationsPanel
        connection="connected"
        events={[
          event('memory.pending', { records: [metadata('cand-3')], counts: { pending_confirmation: 1 } })
        ]}
      />
    )
    await userEvent.click(await screen.findByRole('button', { name: 'Изменить' }))
    await userEvent.type(screen.getByLabelText('Новая формулировка'), 'уточнённая формулировка')
    await userEvent.click(screen.getByRole('button', { name: 'Сохранить правку' }))

    const revise = calls.find((call) => call.command === 'core.reviseMemoryCandidate')
    expect(revise?.payload).toMatchObject({
      id: 'cand-3',
      statement: 'уточнённая формулировка',
      sessionOnly: false
    })
    // Editing must not smuggle in a confirmation.
    expect(calls.some((call) => call.command === 'core.confirmMemory')).toBe(false)
  })

  it('keeps a candidate for this session only, with a session id', async () => {
    render(
      <OperationsPanel
        connection="connected"
        events={[
          event('memory.pending', { records: [metadata('cand-4')], counts: { pending_confirmation: 1 } })
        ]}
      />
    )
    await userEvent.click(await screen.findByRole('button', { name: 'Только на эту сессию' }))
    const revise = calls.find((call) => call.command === 'core.reviseMemoryCandidate')
    const payload = revise?.payload as { sessionOnly: boolean; sessionId: string }
    expect(payload.sessionOnly).toBe(true)
    expect(payload.sessionId.length).toBeGreaterThan(0)
  })

  it('resolves a conflict only through an explicit supersede', async () => {
    render(
      <OperationsPanel
        connection="connected"
        events={[
          event('memory.conflicts', {
            conflicts: [
              {
                pending: metadata('new-1'),
                active: metadata('old-1', { confirmation_state: 'confirmed' }),
                conflict_key: 'preference|язык интерфейса|project',
                supersession_chain: ['old-1']
              }
            ]
          })
        ]}
      />
    )
    expect(await screen.findByText('неразрешённых')).toBeTruthy()
    await userEvent.click(screen.getByRole('button', { name: 'Заменить новой записью' }))
    const supersede = calls.find((call) => call.command === 'core.supersedeMemory')
    expect(supersede?.payload).toMatchObject({
      oldId: 'old-1',
      newId: 'new-1',
      reason: 'user_choice'
    })
  })
})
