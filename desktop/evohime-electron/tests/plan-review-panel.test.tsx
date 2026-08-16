// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { PlanReviewPanel } from '../src/renderer/src/PlanReviewPanel'

const calls: { command: string; payload: unknown }[] = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

// The shell prepends: the newest event is first, and the oldest falls off the
// end. Every fixture below is written in that order.
function event(eventType: string, payload: Record<string, unknown>, taskId = ''): CoreEvent {
  return { sequenceId: 0, taskId, eventType, payload: JSON.stringify(payload) }
}

function startedReviewId(): string {
  return (calls.find((call) => call.command === 'review.start')?.payload as { reviewId: string }).reviewId
}

async function startReview(models: readonly string[]): Promise<void> {
  await userEvent.click(screen.getByRole('button', { name: 'Выбрать Markdown-планы' }))
  await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Модель рецензента 1' }), models[0] as string)
  await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Модель рецензента 2' }), models[1] as string)
  await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Главная модель-синтезатор' }), models[2] as string)
  await userEvent.click(screen.getByRole('button', { name: 'Запустить ревью' }))
}

beforeEach(() => {
  calls.length = 0
  window.localStorage.clear()
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      return ok(command === 'review.list' ? { reviews: [] } : command === 'review.get' ? { review: null } : command === 'review.pickPlan' ? { cancelled: false, files: [{ fileName: 'plan.md', sourceMarkdown: '# Plan' }], directory: '/plans' } : { accepted: true })
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

    await userEvent.click(screen.getByRole('button', { name: 'Выбрать Markdown-планы' }))
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

  it('reads progress from the externally tagged form the core actually sends', async () => {
    const catalog = event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })
    const view = render(<PlanReviewPanel connection="connected" events={[catalog]} />)

    await startReview(['a', 'b', 'main'])
    const reviewId = startedReviewId()
    view.rerender(<PlanReviewPanel connection="connected" events={[
      event('review.progress', { ReviewProgress: { review_id: reviewId, stage: 'reviewers', status: 'completed', model: 'b', completed: 1, total: 2 } }, reviewId),
      event('review.progress', { ReviewProgress: { review_id: reviewId, stage: 'reviewers', status: 'working', model: 'a', completed: 0, total: 2 } }, reviewId),
      catalog
    ]} />)

    expect(screen.getByText('Рецензенты: 1/2')).toBeTruthy()
    const statuses = screen.getAllByText(/^(работает|готово|ожидает)$/).map((node) => node.textContent)
    expect(statuses).toEqual(['работает', 'готово'])
  })

  it('shows the result delivered as a tagged task.completed event', async () => {
    const catalog = event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })
    const view = render(<PlanReviewPanel connection="connected" events={[catalog]} />)

    await startReview(['a', 'b', 'main'])
    const reviewId = startedReviewId()
    const result = { review_id: reviewId, file_name: 'plan.md', synthesis_model: 'main', final_markdown: '# Итог', reviewers: [{ model: 'a', status: 'completed', content: 'ok' }] }
    view.rerender(<PlanReviewPanel connection="connected" events={[
      catalog,
      event('task.completed', { TaskCompleted: { task_id: reviewId, final_message: JSON.stringify(result) } }, reviewId)
    ]} />)

    expect(screen.getByText('Итоговое ревью')).toBeTruthy()
    expect(screen.getByText('Готово')).toBeTruthy()
  })

  it('copies the final markdown to the clipboard', async () => {
    const copied: string[] = []
    const catalog = event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })
    const api = (window as unknown as { evohime: { v1: EvoHimeApiV1 } }).evohime.v1
    Object.defineProperty(window, 'evohime', {
      value: Object.freeze({ v1: { ...api, writeClipboardText: async (text: string) => { copied.push(text); return true } } }),
      configurable: true
    })
    const view = render(<PlanReviewPanel connection="connected" events={[catalog]} />)

    await startReview(['a', 'b', 'main'])
    const reviewId = startedReviewId()
    const result = { review_id: reviewId, file_name: 'plan.md', synthesis_model: 'main', final_markdown: '# Итог\n\nВывод', reviewers: [] }
    view.rerender(<PlanReviewPanel connection="connected" events={[
      catalog,
      event('task.completed', { TaskCompleted: { task_id: reviewId, final_message: JSON.stringify(result) } }, reviewId)
    ]} />)

    await userEvent.click(screen.getByRole('button', { name: 'Скопировать итоговый Markdown' }))
    expect(copied).toEqual(['# Итог\n\nВывод'])
    expect(screen.getByRole('button', { name: 'Скопировано' })).toBeTruthy()
  })

  it('keeps the picked models when a catalogue refresh fails', async () => {
    const catalog = event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })
    const view = render(<PlanReviewPanel connection="connected" events={[catalog]} />)

    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Модель рецензента 1' }), 'a')
    view.rerender(<PlanReviewPanel connection="connected" events={[event('model.catalog', { mode: 'free', models: [], error: 'provider error: 429 rate limit' }), catalog]} />)

    expect((screen.getByRole('combobox', { name: 'Модель рецензента 1' }) as HTMLSelectElement).value).toBe('a')
    expect(screen.getByRole('alert').textContent).toContain('429 rate limit')
  })

  it('keeps every reviewer listed after their events leave the window', async () => {
    const catalog = event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })
    const view = render(<PlanReviewPanel connection="connected" events={[catalog]} />)

    await startReview(['a', 'b', 'main'])
    const reviewId = startedReviewId()
    const progress = (model: string, status: string, completed: number): CoreEvent =>
      event('review.progress', { ReviewProgress: { review_id: reviewId, stage: 'reviewers', status, model, completed, total: 2 } }, reviewId)
    // The shell keeps newest-first and drops the oldest, so a run outlives its
    // own early events.
    view.rerender(<PlanReviewPanel connection="connected" events={[progress('a', 'working', 0), progress('b', 'waiting', 0), progress('a', 'waiting', 0), catalog]} />)
    view.rerender(<PlanReviewPanel connection="connected" events={[progress('b', 'working', 1), progress('a', 'completed', 1)]} />)
    view.rerender(<PlanReviewPanel connection="connected" events={[progress('b', 'completed', 2)]} />)

    const card = screen.getByRole('status')
    const rows = within(card).getAllByRole('listitem').map((node) => node.textContent)
    expect(rows).toEqual(['aготово', 'bготово'])
  })

  it('shows only one reviewer working at a time', async () => {
    const catalog = event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })
    const view = render(<PlanReviewPanel connection="connected" events={[catalog]} />)

    await startReview(['a', 'b', 'main'])
    const reviewId = startedReviewId()
    const progress = (model: string, status: string, completed: number): CoreEvent =>
      event('review.progress', { ReviewProgress: { review_id: reviewId, stage: 'reviewers', status, model, completed, total: 2 } }, reviewId)
    view.rerender(<PlanReviewPanel connection="connected" events={[progress('b', 'working', 1), progress('a', 'completed', 1), progress('a', 'working', 0), progress('b', 'waiting', 0), progress('a', 'waiting', 0), catalog]} />)

    const card = screen.getByRole('status')
    expect(within(card).getAllByText('работает')).toHaveLength(1)
    expect(within(card).getAllByRole('listitem').map((node) => node.textContent)).toEqual(['aготово', 'bработает'])
  })

  it('names the reviewers of a run from its own events', async () => {
    const catalog = event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })
    const view = render(<PlanReviewPanel connection="connected" events={[catalog]} />)

    await startReview(['a', 'b', 'main'])
    const reviewId = startedReviewId()
    // The catalogue drops out mid-run, so the selects lose their options.
    view.rerender(<PlanReviewPanel connection="connected" events={[
      event('model.catalog', { mode: 'free', models: [], error: 'provider error: 429 rate limit' }),
      event('review.progress', { ReviewProgress: { review_id: reviewId, stage: 'reviewers', status: 'working', model: 'b', completed: 1, total: 2 } }, reviewId),
      event('review.progress', { ReviewProgress: { review_id: reviewId, stage: 'reviewers', status: 'completed', model: 'a', completed: 1, total: 2 } }, reviewId),
      catalog
    ]} />)

    const card = screen.getByRole('status')
    expect(within(card).getByText('a')).toBeTruthy()
    expect(within(card).getByText('b')).toBeTruthy()
    expect(within(card).queryByText('Рецензент 1')).toBeNull()
  })

  it('clears the run history only after a confirmation', async () => {
    const history = event('review.list', { reviews: [{ review_id: 'review-1', file_name: 'plan.md', reviewers: [{ status: 'completed' }] }] })
    render(<PlanReviewPanel connection="connected" events={[event('model.catalog', { mode: 'free', models: ['a', 'b'] }), history]} />)

    await userEvent.click(screen.getByRole('button', { name: 'Очистить историю' }))
    expect(calls.some((call) => call.command === 'review.clearHistory')).toBe(false)
    await userEvent.click(screen.getByRole('button', { name: 'Отмена' }))
    expect(calls.some((call) => call.command === 'review.clearHistory')).toBe(false)

    await userEvent.click(screen.getByRole('button', { name: 'Очистить историю' }))
    await userEvent.click(screen.getByRole('button', { name: 'Очистить' }))
    await waitFor(() => expect(calls.some((call) => call.command === 'review.clearHistory')).toBe(true))
    // The list is owned by the core, so it has to be re-read after clearing.
    expect(calls.filter((call) => call.command === 'review.list').length).toBeGreaterThan(1)
  })

  it('offers no clear action for an empty history', async () => {
    render(<PlanReviewPanel connection="connected" events={[event('model.catalog', { mode: 'free', models: ['a', 'b'] }), event('review.list', { reviews: [] })]} />)

    expect(screen.getByText('Завершённых ревью пока нет.')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Очистить историю' })).toBeNull()
  })

  it('keeps quiet while reviewers are working', async () => {
    const catalog = event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })
    const view = render(<PlanReviewPanel connection="connected" events={[catalog]} />)

    await startReview(['a', 'b', 'main'])
    const reviewId = startedReviewId()
    view.rerender(<PlanReviewPanel connection="connected" events={[
      catalog,
      event('review.progress', { ReviewProgress: { review_id: reviewId, stage: 'reviewers', status: 'working', model: 'a', completed: 0, total: 2 } }, reviewId)
    ]} />)

    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('confirms that the core accepted the plan', async () => {
    const catalog = event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })
    const view = render(<PlanReviewPanel connection="connected" events={[catalog]} />)

    await startReview(['a', 'b', 'main'])
    expect(screen.getByText('Отправка плана')).toBeTruthy()
    view.rerender(<PlanReviewPanel connection="connected" events={[catalog, event('review.started', { review_id: startedReviewId(), accepted: true })]} />)
    expect(screen.getByText('Отправлено в ядро')).toBeTruthy()
    expect(screen.getByText(/Ядро приняло план/)).toBeTruthy()
  })

  it('surfaces the failure reason instead of waiting forever', async () => {
    const catalog = event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })
    const view = render(<PlanReviewPanel connection="connected" events={[catalog]} />)

    await startReview(['a', 'b', 'main'])
    const reviewId = startedReviewId()
    view.rerender(<PlanReviewPanel connection="connected" events={[catalog, event('task.failed', { error: 'provider error: 401 unauthorized' }, reviewId)]} />)

    expect(screen.getByText('Ошибка')).toBeTruthy()
    expect(screen.getByRole('alert').textContent).toContain('401 unauthorized')
    expect(screen.getByRole('button', { name: 'Повторить ревью' }).hasAttribute('disabled')).toBe(false)
  })

  it('reopens the picker in the directory of the previous plan', async () => {
    const catalog = event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })
    const first = render(<PlanReviewPanel connection="connected" events={[catalog]} />)

    await userEvent.click(screen.getByRole('button', { name: 'Выбрать Markdown-планы' }))
    expect(calls).toContainEqual({ command: 'review.pickPlan', payload: { directory: '' } })
    first.unmount()

    // A fresh mount stands for the next launch of the shell: the folder must
    // survive it, otherwise the user navigates to their plans every time.
    render(<PlanReviewPanel connection="connected" events={[catalog]} />)
    await userEvent.click(screen.getByRole('button', { name: 'Выбрать Markdown-планы' }))
    expect(calls).toContainEqual({ command: 'review.pickPlan', payload: { directory: '/plans' } })
  })

  it('merges several picked plans into one review document', async () => {
    Object.defineProperty(window, 'evohime', {
      value: Object.freeze({ v1: {
        apiVersion: 1,
        invoke: (async (command: RendererCommand, payload: unknown) => {
          calls.push({ command, payload })
          return ok(command === 'review.list' ? { reviews: [] } : command === 'review.pickPlan' ? { cancelled: false, files: [{ fileName: 'a.md', sourceMarkdown: '# A' }, { fileName: 'b.md', sourceMarkdown: '# B' }], directory: '/plans' } : { accepted: true })
        }) as EvoHimeApiV1['invoke'],
        subscribe: () => () => {},
        writeClipboardText: async () => true,
        openExternal: async () => true
      } satisfies EvoHimeApiV1 }),
      configurable: true
    })
    render(<PlanReviewPanel connection="connected" events={[event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })]} />)

    await startReview(['a', 'b', 'main'])
    const payload = calls.find((call) => call.command === 'review.start')?.payload as { fileName: string; sourceMarkdown: string }
    expect(payload.fileName).toBe('a.md, b.md')
    expect(payload.sourceMarkdown).toBe('## Файл 1 из 2: a.md\n\n# A\n\n---\n\n## Файл 2 из 2: b.md\n\n# B')
    expect(screen.getByText(/Файлов: 2/)).toBeTruthy()
  })

  it('accepts markdown dropped onto the panel and skips other files', async () => {
    render(<PlanReviewPanel connection="connected" events={[event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })]} />)

    fireEvent.drop(screen.getByRole('group', { name: 'Планы на ревью' }), {
      dataTransfer: { files: [new File(['# A'], 'a.md'), new File(['x'], 'notes.txt')] }
    })

    await waitFor(() => expect(screen.getByText('a.md')).toBeTruthy())
    expect(screen.queryByText('notes.txt')).toBeNull()
    expect(screen.getByRole('alert').textContent).toContain('notes.txt')

    // Крестик убирает лишний файл, не трогая остальной выбор.
    await userEvent.click(screen.getByRole('button', { name: 'Убрать a.md' }))
    expect(screen.getByText('Файлы не выбраны.')).toBeTruthy()
  })

  it('adds dropped plans to the ones already picked', async () => {
    render(<PlanReviewPanel connection="connected" events={[event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })]} />)

    await userEvent.click(screen.getByRole('button', { name: 'Выбрать Markdown-планы' }))
    fireEvent.drop(screen.getByRole('group', { name: 'Планы на ревью' }), {
      dataTransfer: { files: [new File(['# Extra'], 'extra.md')] }
    })

    await waitFor(() => expect(screen.getByText('extra.md')).toBeTruthy())
    expect(screen.getByText('plan.md')).toBeTruthy()
    await userEvent.click(screen.getByRole('button', { name: 'Очистить список' }))
    expect(screen.getByText('Файлы не выбраны.')).toBeTruthy()
  })

  it('reports a rejected start request', async () => {
    Object.defineProperty(window, 'evohime', {
      value: Object.freeze({ v1: {
        apiVersion: 1,
        invoke: (async (command: RendererCommand, payload: unknown) => {
          calls.push({ command, payload })
          if (command === 'review.start') return { ok: false, message: 'provider is not configured' } as CommandOutcome<RendererCommand>
          return ok(command === 'review.list' ? { reviews: [] } : command === 'review.pickPlan' ? { cancelled: false, files: [{ fileName: 'plan.md', sourceMarkdown: '# Plan' }], directory: '/plans' } : { accepted: true })
        }) as EvoHimeApiV1['invoke'],
        subscribe: () => () => {},
        writeClipboardText: async () => true,
        openExternal: async () => true
      } satisfies EvoHimeApiV1 }),
      configurable: true
    })
    render(<PlanReviewPanel connection="connected" events={[event('model.catalog', { mode: 'free', models: ['a', 'b', 'main'] })]} />)

    await startReview(['a', 'b', 'main'])
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('provider is not configured'))
  })
})
