// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { PlanReviewPanel } from '../src/renderer/src/PlanReviewPanel'

const calls: { command: string; payload: unknown }[] = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

function event(eventType: string, payload: Record<string, unknown>): CoreEvent {
  return { sequenceId: 0, taskId: '', eventType, payload: JSON.stringify(payload) }
}

beforeEach(() => {
  calls.length = 0
  window.localStorage.clear()
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      return ok(command === 'review.list' ? { reviews: [] } : command === 'review.get' ? { review: null } : command === 'review.pickPlan' ? { cancelled: false, fileName: 'plan.md', sourceMarkdown: '# Plan' } : { accepted: true })
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

describe('plan review panel', () => {
  it('requests and displays the selected free or paid catalogue', async () => {
    const view = render(<PlanReviewPanel connection="connected" events={[event('model.catalog', { mode: 'free', models: ['free-a', 'free-b'] }), event('model.catalog', { mode: 'paid', models: ['paid-a'] })]} />)

    await waitFor(() => expect(calls).toContainEqual({ command: 'core.listModelCatalog', payload: { mode: 'free' } }))
    expect(within(screen.getByRole('combobox', { name: 'Модель рецензента 1' })).getByRole('option', { name: 'free-a' })).toBeTruthy()
    await userEvent.selectOptions(screen.getByLabelText('Режим каталога'), 'paid')
    expect(within(screen.getByRole('combobox', { name: 'Модель рецензента 1' })).getByRole('option', { name: 'paid-a' })).toBeTruthy()
    expect(within(screen.getByRole('combobox', { name: 'Модель рецензента 1' })).queryByRole('option', { name: 'free-a' })).toBeNull()
    expect(calls).toContainEqual({ command: 'core.listModelCatalog', payload: { mode: 'paid' } })
    view.unmount()
  })

  it('changes reviewer count from two to eight and creates model slots', async () => {
    render(<PlanReviewPanel connection="connected" events={[event('model.catalog', { mode: 'free', models: ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'] })]} />)

    await userEvent.selectOptions(screen.getByLabelText('Количество рецензентов'), '8')
    expect(screen.getAllByRole('combobox', { name: /Модель рецензента/ })).toHaveLength(8)
  })

  it('disables a model already selected in another reviewer slot', async () => {
    render(<PlanReviewPanel connection="connected" events={[event('model.catalog', { mode: 'free', models: ['a', 'b', 'c'] })]} />)

    const first = screen.getByRole('combobox', { name: 'Модель рецензента 1' })
    const second = screen.getByRole('combobox', { name: 'Модель рецензента 2' })
    await userEvent.selectOptions(first, 'a')
    expect(within(second).getByRole('option', { name: 'a', exact: true }).hasAttribute('disabled')).toBe(true)
    expect(second).toBeTruthy()
  })

  it('renders durable reviewer progress and the synthesis stage', async () => {
    const view = render(<PlanReviewPanel connection="connected" events={[
      event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] }),
    ]} />)

    await userEvent.click(screen.getByRole('button', { name: 'Выбрать Markdown-план' }))
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Модель рецензента 1' }), 'a')
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Модель рецензента 2' }), 'b')
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Главная модель-синтезатор' }), 'main')
    await userEvent.click(screen.getByRole('button', { name: 'Запустить ревью' }))
    view.rerender(<PlanReviewPanel connection="connected" events={[
      event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] }),
      event('review.progress', { review_id: calls.find((call) => call.command === 'review.start')?.payload && (calls.find((call) => call.command === 'review.start')?.payload as { reviewId: string }).reviewId, stage: 'synthesis', status: 'working', model: 'main', completed: 2, total: 2 })
    ]} />)
    expect(screen.getByText(/Синтез результата · main/)).toBeTruthy()
  })
})
