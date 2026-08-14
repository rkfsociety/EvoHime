import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent, ModelTier, PlanReviewResult } from '@shared/api'

import { useShellApi } from './shell-api'
import { MarkdownMessage } from './MarkdownMessage'

const CONNECTED: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']
const MIN_REVIEWERS = 2
const MAX_REVIEWERS = 8
const REVIEW_PREFERENCES_KEY = 'evohime.review-preferences.v2'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

interface ReviewPreferences {
  readonly tier: ModelTier
  readonly reviewerModels: readonly string[]
  readonly synthesisModel: string
}

interface ReviewProgress {
  readonly reviewId: string
  readonly stage: string
  readonly status: string
  readonly model: string | null
  readonly completed: number
  readonly total: number
}

function loadReviewPreferences(): ReviewPreferences {
  try {
    const raw = window.localStorage.getItem(REVIEW_PREFERENCES_KEY)
    if (!raw) return { tier: 'free', reviewerModels: [], synthesisModel: '' }
    const value = JSON.parse(raw) as { tier?: unknown; reviewerModels?: unknown; synthesisModel?: unknown }
    return {
      tier: value.tier === 'paid' ? 'paid' : 'free',
      reviewerModels: Array.isArray(value.reviewerModels) ? value.reviewerModels.filter((model): model is string => typeof model === 'string').slice(0, MAX_REVIEWERS) : [],
      synthesisModel: typeof value.synthesisModel === 'string' ? value.synthesisModel : ''
    }
  } catch {
    return { tier: 'free', reviewerModels: [], synthesisModel: '' }
  }
}

export function PlanReviewPanel({ connection, events }: Props): React.JSX.Element {
  const api = useShellApi()
  const [preferences] = useState(loadReviewPreferences)
  const [fileName, setFileName] = useState('')
  const [sourceMarkdown, setSourceMarkdown] = useState('')
  const [tier, setTier] = useState<ModelTier>(preferences.tier)
  const [models, setModels] = useState<readonly string[]>([])
  const [reviewerCount, setReviewerCount] = useState(Math.max(MIN_REVIEWERS, Math.min(MAX_REVIEWERS, preferences.reviewerModels.length || MIN_REVIEWERS)))
  const [reviewers, setReviewers] = useState<readonly string[]>(normalizeSlots(preferences.reviewerModels, reviewerCount))
  const [synthesisModel, setSynthesisModel] = useState(preferences.synthesisModel)
  const [reviewId, setReviewId] = useState<string | null>(null)
  const [selectedResult, setSelectedResult] = useState<PlanReviewResult | null>(null)
  const [error, setError] = useState<string | null>(null)

  const catalogModels = useMemo(() => latestCatalog(events, tier), [events, tier])
  const reviewResult = useMemo(() => latestReviewResult(events), [events])
  const progress = useMemo(() => reviewId ? latestReviewProgress(events, reviewId) : null, [events, reviewId])
  const reviewFinished = reviewResult?.reviewId === reviewId
  const running = reviewId !== null && !reviewFinished && progress?.stage !== 'completed' && progress?.stage !== 'failed'

  useEffect(() => {
    if (!api || !CONNECTED.includes(connection)) return
    void api.invoke('core.listModelCatalog', { mode: tier })
    void api.invoke('review.list', { limit: 20 })
  }, [api, connection, tier])

  useEffect(() => {
    setModels(catalogModels)
    setReviewers((current) => normalizeSlots(current.filter((model) => catalogModels.includes(model)), reviewerCount, catalogModels))
    setSynthesisModel((current) => catalogModels.includes(current) ? current : catalogModels[0] || '')
  }, [catalogModels, reviewerCount])

  useEffect(() => {
    try {
      window.localStorage.setItem(REVIEW_PREFERENCES_KEY, JSON.stringify({ tier, reviewerModels: reviewers, synthesisModel }))
    } catch {
      // Persistence is best-effort and must not block review execution.
    }
  }, [reviewers, synthesisModel, tier])

  useEffect(() => {
    if (reviewResult?.reviewId === reviewId) setSelectedResult(reviewResult)
  }, [reviewId, reviewResult])

  useEffect(() => {
    if (!api || !running || !reviewId) return
    const timer = window.setInterval(() => { void api.invoke('review.get', { reviewId }) }, 1500)
    return () => window.clearInterval(timer)
  }, [api, reviewId, running])

  const pick = async (): Promise<void> => {
    if (!api) return
    const outcome = await api.invoke('review.pickPlan', {})
    if (!outcome.ok || outcome.value.cancelled) return
    setFileName(outcome.value.fileName)
    setSourceMarkdown(outcome.value.sourceMarkdown)
    setSelectedResult(null)
    setReviewId(null)
    setError(null)
  }

  const changeCount = (count: number): void => {
    setReviewerCount(count)
    setReviewers((current) => normalizeSlots(current, count, models))
  }

  const setReviewerModel = (index: number, model: string): void => {
    setReviewers((current) => current.map((item, position) => position === index ? model : item))
  }

  const start = async (): Promise<void> => {
    if (!api || !fileName || !sourceMarkdown || reviewers.length < MIN_REVIEWERS || reviewers.some((model) => !model) || new Set(reviewers).size !== reviewers.length || !synthesisModel) return
    const id = makeReviewId()
    setReviewId(id)
    setSelectedResult(null)
    setError(null)
    const outcome = await api.invoke('review.start', { reviewId: id, fileName, sourceMarkdown, reviewerModels: reviewers, synthesisModel })
    if (!outcome.ok) setError(outcome.message)
  }

  const stop = async (): Promise<void> => {
    if (!api || !reviewId) return
    const outcome = await api.invoke('review.stop', { reviewId })
    if (!outcome.ok) setError(outcome.message)
  }

  const openHistory = async (id: string): Promise<void> => {
    if (!api) return
    await api.invoke('review.get', { reviewId: id })
    setReviewId(id)
    setSelectedResult(null)
  }

  const exportResult = async (): Promise<void> => {
    if (!api || !selectedResult) return
    const outcome = await api.invoke('review.export', { reviewId: selectedResult.reviewId, destinationPath: '', includeReviewers: false })
    if (!outcome.ok) setError(outcome.message)
  }

  const canStart = CONNECTED.includes(connection) && fileName.length > 0 && reviewers.length >= MIN_REVIEWERS && reviewers.every(Boolean) && new Set(reviewers).size === reviewers.length && synthesisModel.length > 0 && !running
  const status = reviewStatus(progress, reviewFinished, selectedResult !== null)

  return (
    <section className="review-panel" aria-label="Ревью планов">
      <div className="review-panel__toolbar">
        <button type="button" onClick={() => void pick()}>Выбрать Markdown-план</button>
        {fileName ? <span className="review-panel__file">{fileName}</span> : <span>Файл не выбран</span>}
      </div>
      {fileName ? <p className="review-panel__source">Загружено символов: {sourceMarkdown.length}</p> : null}

      <div className="review-panel__controls">
        <label>Режим каталога<select value={tier} onChange={(event) => setTier(event.target.value as ModelTier)} disabled={running}><option value="free">Бесплатные</option><option value="paid">Платные</option></select></label>
        <label>Количество рецензентов<select value={reviewerCount} onChange={(event) => changeCount(Number(event.target.value))} disabled={running}>{Array.from({ length: MAX_REVIEWERS - MIN_REVIEWERS + 1 }, (_, index) => MIN_REVIEWERS + index).map((count) => <option key={count} value={count}>{count}</option>)}</select></label>
      </div>

      <fieldset className="review-panel__models">
        <legend>Модели-рецензенты · {reviewers.filter(Boolean).length} из {reviewerCount}</legend>
        {models.length === 0 ? <p>Каталог {tier === 'free' ? 'бесплатных' : 'платных'} моделей недоступен или пуст.</p> : reviewers.map((model, index) => (
          <label key={index} className="review-panel__model-row">
            <span>Рецензент {index + 1}</span>
            <select aria-label={`Модель рецензента ${index + 1}`} value={model} onChange={(event) => setReviewerModel(index, event.target.value)} disabled={running}>
              <option value="">Выбери модель</option>
              {models.map((candidate) => <option key={candidate} value={candidate} disabled={reviewers.some((selected, selectedIndex) => selectedIndex !== index && selected === candidate)}>{candidate}</option>)}
            </select>
          </label>
        ))}
      </fieldset>

      <label className="review-panel__synthesis">Главная модель-синтезатор<select value={synthesisModel} onChange={(event) => setSynthesisModel(event.target.value)} disabled={running}><option value="">Выбери модель</option>{models.map((model) => <option key={model} value={model}>{model}</option>)}</select></label>

      <div className="review-panel__actions"><button type="button" onClick={() => void start()} disabled={!canStart}>{reviewId && (reviewFinished || progress?.stage === 'failed') ? 'Запустить снова' : 'Запустить ревью'}</button><button type="button" onClick={() => void stop()} disabled={!running}>Остановить</button></div>
      {reviewId ? <ProgressCard events={events} reviewId={reviewId} progress={progress} status={status} reviewers={reviewers} /> : null}
      {error ? <p role="alert" className="shell__reason">{error}</p> : null}

      {selectedResult ? <div className="review-panel__result"><div className="review-panel__result-heading"><h3>Итоговое ревью</h3><button type="button" onClick={() => void exportResult()}>Экспортировать итоговый Markdown</button></div><MarkdownMessage text={selectedResult.finalMarkdown} /><h3>Исходные ответы</h3>{selectedResult.reviewers.map((review) => <details key={review.model}><summary>{review.model} · {review.status}</summary>{review.error ? <p>{review.error}</p> : <MarkdownMessage text={review.content} />}</details>)}</div> : null}
      <History events={events} onOpen={(id) => void openHistory(id)} />
    </section>
  )
}

function ProgressCard({ events, reviewId, progress, status, reviewers }: { readonly events: readonly CoreEvent[]; readonly reviewId: string; readonly progress: ReviewProgress | null; readonly status: string; readonly reviewers: readonly string[] }): React.JSX.Element {
  const completed = progress?.completed ?? 0
  const total = progress?.total || reviewers.length
  return <div className="review-panel__progress" role="status" aria-live="polite"><div className="review-panel__progress-heading"><strong>{status}</strong>{progress?.stage === 'reviewers' ? <span>{Math.min(completed, total)}/{total}</span> : null}</div><div className="review-panel__progress-bar"><span style={{ width: `${total ? Math.round((Math.min(completed, total) / total) * 100) : 0}%` }} /></div><p>{progress?.stage === 'synthesis' ? `Синтез результата · ${progress.model ?? 'модель'}` : 'Рецензенты работают параллельно'}</p><ul>{reviewers.map((model, index) => { const state = model ? latestReviewerStatus(events, reviewId, model) : 'waiting'; return <li key={`${model}-${index}`}><span>{model || `Рецензент ${index + 1}`}</span><span className={`review-panel__reviewer-status review-panel__reviewer-status--${state}`}>{reviewStatusLabel(state)}</span></li> })}</ul></div>
}

function History({ events, onOpen }: { readonly events: readonly CoreEvent[]; readonly onOpen: (id: string) => void }): React.JSX.Element {
  const payload = latestPayload(events, 'review.list')
  const reviews = Array.isArray(payload?.reviews) ? payload.reviews : []
  return <div className="review-panel__history"><h3>История запусков</h3>{reviews.length === 0 ? <p>Завершённых ревью пока нет.</p> : reviews.map((item) => { const value = item as Record<string, unknown>; const reviewers = Array.isArray(value.reviewers) ? value.reviewers : []; const completed = reviewers.filter((review) => (review as Record<string, unknown>).status === 'completed').length; return <button key={String(value.review_id)} type="button" onClick={() => onOpen(String(value.review_id))}><span>{String(value.file_name)}</span><small>{completed}/{reviewers.length} рецензентов · {String(value.review_id)}</small></button> })}</div>
}

function latestCatalog(events: readonly CoreEvent[], tier: ModelTier): readonly string[] {
  const event = [...events].reverse().find((item) => item.eventType === 'model.catalog' && readPayload(item)?.mode === tier)
  const models = readPayload(event)?.models
  return Array.isArray(models) ? models.filter((model): model is string => typeof model === 'string').sort() : []
}

function latestPayload(events: readonly CoreEvent[], type: string): Record<string, unknown> | null { return readPayload([...events].reverse().find((item) => item.eventType === type)) }

function latestReviewProgress(events: readonly CoreEvent[], reviewId: string): ReviewProgress | null {
  for (const event of [...events].reverse()) { if (event.eventType !== 'review.progress') continue; const payload = readPayload(event); if (payload?.review_id !== reviewId) continue; return { reviewId, stage: String(payload.stage ?? ''), status: String(payload.status ?? ''), model: typeof payload.model === 'string' ? payload.model : null, completed: Number(payload.completed ?? 0), total: Number(payload.total ?? 0) } }
  return null
}

function latestReviewerStatus(events: readonly CoreEvent[], reviewId: string, model: string): string {
  for (const event of [...events].reverse()) {
    if (event.eventType !== 'review.progress') continue
    const payload = readPayload(event)
    if (payload?.review_id === reviewId && payload.model === model) return String(payload.status ?? 'waiting')
  }
  return 'waiting'
}

function latestReviewResult(events: readonly CoreEvent[]): PlanReviewResult | null {
  const responsePayload = readPayload([...events].reverse().find((item) => item.eventType === 'review.result'))
  if (responsePayload?.result && typeof responsePayload.result === 'object') return normalizeResult(responsePayload.result as Record<string, unknown>)
  const payload = readPayload([...events].reverse().find((item) => item.eventType === 'task.completed' && item.taskId.startsWith('review-')))
  const finalMessage = payload?.TaskCompleted && typeof payload.TaskCompleted === 'object' ? (payload.TaskCompleted as Record<string, unknown>).final_message : payload?.final_message
  if (typeof finalMessage !== 'string') return null
  try { return normalizeResult(JSON.parse(finalMessage) as Record<string, unknown>) } catch { return null }
}

function normalizeResult(value: Record<string, unknown>): PlanReviewResult | null {
  const reviewId = value.review_id ?? value.reviewId; const fileName = value.file_name ?? value.fileName; const synthesisModel = value.synthesis_model ?? value.synthesisModel; const finalMarkdown = value.final_markdown ?? value.finalMarkdown
  if (typeof reviewId !== 'string' || typeof fileName !== 'string' || typeof synthesisModel !== 'string' || typeof finalMarkdown !== 'string' || !Array.isArray(value.reviewers)) return null
  return { reviewId, fileName, synthesisModel, finalMarkdown, reviewers: value.reviewers.map((item) => { const review = item as Record<string, unknown>; return { model: String(review.model ?? ''), status: String(review.status ?? ''), content: String(review.content ?? ''), error: typeof review.error === 'string' ? review.error : null } }) }
}

function normalizeSlots(values: readonly string[], count: number, available: readonly string[] = []): string[] { const result = values.slice(0, count); while (result.length < count) result.push(''); const candidates = available.filter((model) => !result.includes(model)); for (let index = 0; index < result.length; index += 1) if (!result[index] && candidates.length > 0) result[index] = candidates.shift() as string; return result }
function readPayload(event: CoreEvent | undefined): Record<string, unknown> | null { if (!event) return null; try { const value: unknown = JSON.parse(event.payload); return typeof value === 'object' && value !== null ? value as Record<string, unknown> : null } catch { return null } }
function reviewStatus(progress: ReviewProgress | null, finished: boolean, hasResult: boolean): string { if (hasResult || finished) return 'Готово'; if (progress?.stage === 'synthesis') return 'Синтез результата'; if (progress?.stage === 'reviewers') return `Рецензенты: ${progress.completed}/${progress.total}`; return 'Подготовка' }
function reviewStatusLabel(status: string): string { if (status === 'completed') return 'готово'; if (status === 'failed') return 'ошибка'; if (status === 'working') return 'работает'; return 'ожидает' }
function makeReviewId(): string { return typeof crypto.randomUUID === 'function' ? `review-${crypto.randomUUID()}` : `review-${Date.now()}` }
