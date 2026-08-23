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

/** Одна карточка предложения ровно в том виде, в каком её отдаёт ядро. */
function proposal(id: string, overrides: Record<string, unknown> = {}) {
  return {
    proposal_id: id,
    kind: 'reminder',
    subject: 'хлеб',
    title: 'Напомнить купить хлеб',
    source_episode_id: 'ep-1',
    created_at_ms: 1_770_000_000_000,
    expires_at_ms: 1_770_086_400_000,
    occurrences: 1,
    state: 'proposed',
    ...overrides
  }
}

function proposalList(proposals: Array<Record<string, unknown>>) {
  return event('ambient.proposals', {
    proposals,
    max_per_hour: 3,
    max_per_day: 10,
    min_interval_ms: 600_000,
    error_code: ''
  })
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

  it('берёт счётчики из самого нового memory.pending, а не из самого старого в буфере', async () => {
    // App.tsx кладёт новое событие в начало events, так что первое
    // совпадение — самое свежее. Раньше latest() брал .filter().at(-1) —
    // самое старое ещё не вытесненное совпадение — и на длинной сессии с
    // несколькими memory.pending показывал протухшие счётчики.
    render(
      <OperationsPanel
        connection="connected"
        events={[
          event('memory.pending', {
            records: [metadata('cand-fresh')],
            counts: { pending_confirmation: 3, confirmed: 9, expired: 1 }
          }),
          event('memory.pending', {
            records: [metadata('cand-stale')],
            counts: { pending_confirmation: 1, confirmed: 4, expired: 2 }
          })
        ]}
      />
    )
    expect(await screen.findByText('ждут решения')).toBeTruthy()
    expect(screen.getByText('3')).toBeTruthy()
    expect(screen.getByText(/9 активных · 1 истекло/)).toBeTruthy()
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

  it('marks an ambient candidate as heard and says the speaker is unverified', async () => {
    render(
      <OperationsPanel
        connection="connected"
        events={[
          event('memory.pending', {
            records: [metadata('cand-5', { source_trust: 'ambient', validation_status: 'unknown' })],
            counts: { pending_confirmation: 1 }
          })
        ]}
      />
    )
    expect(await screen.findByText('услышано')).toBeTruthy()
    expect(screen.getByText(/говорящий не подтверждён/)).toBeTruthy()
  })

  it('filters the queue by source without confirming anything hidden', async () => {
    render(
      <OperationsPanel
        connection="connected"
        events={[
          event('memory.pending', {
            records: [
              metadata('cand-dialog'),
              metadata('cand-ambient', { source_trust: 'ambient', kind: 'entity' })
            ],
            counts: { pending_confirmation: 2 }
          })
        ]}
      />
    )
    // Выбираем услышанного кандидата, затем прячем его фильтром: скрытая
    // строка не должна уехать в подтверждение вместе с видимыми.
    await userEvent.click(await screen.findByRole('checkbox', { name: 'факт' }))
    await userEvent.selectOptions(screen.getByLabelText('Источник'), 'dialog')
    expect(screen.queryByRole('checkbox', { name: 'факт' })).toBeNull()
    expect(screen.getByRole('checkbox', { name: 'предпочтение' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Сохранить выбранные' })).toHaveProperty(
      'disabled',
      true
    )

    await userEvent.selectOptions(screen.getByLabelText('Источник'), 'ambient')
    expect(screen.queryByRole('checkbox', { name: 'предпочтение' })).toBeNull()
    expect(screen.getByRole('checkbox', { name: 'факт' })).toBeTruthy()
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

  /**
   * Карточка приходит отдельным списком, а не подмешивается в `memory.pending`:
   * это разные очереди с разными решениями.
   */
  it('shows a heard proposal as a card with the ceiling spelled out', async () => {
    render(<OperationsPanel connection="connected" events={[proposalList([proposal('p-1')])]} />)
    await waitFor(() => {
      expect(screen.getByText('Напомнить купить хлеб')).toBeTruthy()
    })
    expect(screen.getByText(/не больше 3 в час и 10 в сутки/)).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Напомнить' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Больше не предлагать такое' })).toBeTruthy()
  })

  /**
   * Повтор не плодит карточки: одна строка со счётчиком, а не две одинаковые.
   */
  it('renders a repeated proposal once, with its counter', async () => {
    render(
      <OperationsPanel
        connection="connected"
        events={[proposalList([proposal('p-1', { occurrences: 3 })])]}
      />
    )
    await waitFor(() => {
      expect(screen.getByText(/упомянуто 3 раза/)).toBeTruthy()
    })
    expect(screen.getAllByText(/Напомнить купить хлеб/)).toHaveLength(1)
  })

  /**
   * Каждое решение уходит в ядро с ключом идемпотентности: без него двойной
   * клик по карточке породил бы две задачи.
   */
  it('sends every decision with an idempotency key and the mute flag', async () => {
    render(<OperationsPanel connection="connected" events={[proposalList([proposal('p-1')])]} />)
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Напомнить' })).toBeTruthy()
    })
    await userEvent.click(screen.getByRole('button', { name: 'Напомнить' }))
    await waitFor(() => {
      expect(calls.some((call) => call.command === 'ambient.resolveProposal')).toBe(true)
    })
    const decision = calls.find((call) => call.command === 'ambient.resolveProposal')
    expect(decision?.payload).toMatchObject({
      proposalId: 'p-1',
      accepted: true,
      mute: false
    })
    expect(
      typeof (decision?.payload as { idempotencyKey?: string }).idempotencyKey === 'string' &&
        (decision?.payload as { idempotencyKey: string }).idempotencyKey.length > 0
    ).toBe(true)
  })

  it('sends the mute flag when the user asks not to be offered this again', async () => {
    render(<OperationsPanel connection="connected" events={[proposalList([proposal('p-1')])]} />)
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Больше не предлагать такое' })).toBeTruthy()
    })
    await userEvent.click(screen.getByRole('button', { name: 'Больше не предлагать такое' }))
    await waitFor(() => {
      expect(calls.some((call) => call.command === 'ambient.resolveProposal')).toBe(true)
    })
    expect(calls.find((call) => call.command === 'ambient.resolveProposal')?.payload).toMatchObject({
      accepted: false,
      mute: true
    })
  })

  /** Решённое и просроченное в очереди не висит. */
  it('never shows a resolved or expired proposal as waiting', async () => {
    render(
      <OperationsPanel
        connection="connected"
        events={[
          proposalList([
            proposal('p-accepted', { state: 'accepted' }),
            proposal('p-expired', { state: 'expired', title: 'Старое предложение' })
          ])
        ]}
      />
    )
    await waitFor(() => {
      expect(screen.getByText('Предложений нет: Ева ничего не предлагает.')).toBeTruthy()
    })
    expect(screen.queryByText('Старое предложение')).toBeNull()
  })
})