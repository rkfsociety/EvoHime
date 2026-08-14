import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent, PlanReviewResult } from '@shared/api'

import { useShellApi } from './shell-api'
import { MarkdownMessage } from './MarkdownMessage'

const CONNECTED: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

export function PlanReviewPanel({ connection, events }: Props): React.JSX.Element {
  const api = useShellApi()
  const [fileName, setFileName] = useState('')
  const [sourceMarkdown, setSourceMarkdown] = useState('')
  const [models, setModels] = useState<readonly string[]>([])
  const [reviewers, setReviewers] = useState<readonly string[]>([])
  const [synthesisModel, setSynthesisModel] = useState('')
  const [reviewId, setReviewId] = useState<string | null>(null)
  const [selectedResult, setSelectedResult] = useState<PlanReviewResult | null>(null)
  const [error, setError] = useState<string | null>(null)

  const catalogModels = useMemo(() => {
    const seen = new Set<string>()
    for (const event of events.filter((item) => item.eventType === 'model.catalog')) {
      try {
        const payload = JSON.parse(event.payload) as { models?: unknown }
        if (Array.isArray(payload.models)) {
          for (const model of payload.models) if (typeof model === 'string') seen.add(model)
        }
      } catch {
        // Ignore malformed provider payloads; Core already redacts the error.
      }
    }
    return [...seen].sort()
  }, [events])
  const reviewResult = useMemo(() => latestReviewResult(events), [events])
  const reviewFinished = reviewResult?.reviewId === reviewId
  const running = reviewId !== null && !reviewFinished && selectedResult === null

  useEffect(() => {
    if (!api || !CONNECTED.includes(connection)) return
    void api.invoke('core.listModelCatalog', { mode: 'free' })
    void api.invoke('core.listModelCatalog', { mode: 'paid' })
    void api.invoke('review.list', { limit: 20 })
  }, [api, connection])

  useEffect(() => {
    setModels(catalogModels)
    setSynthesisModel((current) => current || catalogModels[0] || '')
  }, [catalogModels])

  useEffect(() => {
    if (reviewResult?.reviewId === reviewId) setSelectedResult(reviewResult)
  }, [reviewId, reviewResult])

  useEffect(() => {
    if (!api || !running || !reviewId) return
    const timer = window.setInterval(() => {
      void api.invoke('review.get', { reviewId })
    }, 1500)
    return () => window.clearInterval(timer)
  }, [api, reviewId, running])

  const pick = async (): Promise<void> => {
    if (!api) return
    const outcome = await api.invoke('review.pickPlan', {})
    if (!outcome.ok || outcome.value.cancelled) return
    setFileName(outcome.value.fileName)
    setSourceMarkdown(outcome.value.sourceMarkdown)
    setSelectedResult(null)
    setError(null)
  }

  const toggleReviewer = (model: string): void => {
    setReviewers((current) => current.includes(model) ? current.filter((item) => item !== model) : [...current, model])
  }

  const start = async (): Promise<void> => {
    if (!api || !fileName || !sourceMarkdown || reviewers.length < 2 || reviewers.length > 8 || !synthesisModel) return
    const id = makeReviewId()
    setReviewId(id)
    setSelectedResult(null)
    setError(null)
    const outcome = await api.invoke('review.start', {
      reviewId: id,
      fileName,
      sourceMarkdown,
      reviewerModels: reviewers,
      synthesisModel
    })
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
  }

  const exportResult = async (): Promise<void> => {
    if (!api || !selectedResult) return
    const outcome = await api.invoke('review.export', {
      reviewId: selectedResult.reviewId,
      destinationPath: '',
      includeReviewers: false
    })
    if (!outcome.ok) setError(outcome.message)
  }

  const canStart = CONNECTED.includes(connection) && fileName.length > 0 && reviewers.length >= 2 && reviewers.length <= 8 && synthesisModel.length > 0 && !running

  return (
    <section className="review-panel" aria-label="Ревью планов">
      <div className="review-panel__toolbar">
        <button type="button" onClick={() => void pick()}>Выбрать Markdown-план</button>
        {fileName ? <span className="review-panel__file">{fileName}</span> : <span>Файл не выбран</span>}
      </div>

      {fileName ? <p className="review-panel__source">Загружено символов: {sourceMarkdown.length}</p> : null}

      <fieldset className="review-panel__models">
        <legend>Модели-рецензенты: выбрано {reviewers.length} из 2–8</legend>
        {models.length === 0 ? <p>Каталог моделей недоступен или пуст.</p> : models.map((model) => (
          <label key={model}>
            <input type="checkbox" checked={reviewers.includes(model)} onChange={() => toggleReviewer(model)} disabled={running} />
            {model}
          </label>
        ))}
      </fieldset>

      <label className="review-panel__synthesis">
        Главная модель-синтезатор
        <select value={synthesisModel} onChange={(event) => setSynthesisModel(event.target.value)} disabled={running}>
          <option value="">Выбери модель</option>
          {models.map((model) => <option key={model} value={model}>{model}</option>)}
        </select>
      </label>

      <div className="review-panel__actions">
        <button type="button" onClick={() => void start()} disabled={!canStart}>Запустить ревью</button>
        <button type="button" onClick={() => void stop()} disabled={!running}>Остановить</button>
      </div>

      {running ? <p role="status">Рецензенты работают параллельно, затем главная модель соберёт итог…</p> : null}
      {error ? <p role="alert" className="shell__reason">{error}</p> : null}

      {selectedResult ? (
        <div className="review-panel__result">
          <h3>Итоговое ревью</h3>
          <button type="button" onClick={() => void exportResult()}>Экспортировать итоговый Markdown</button>
          <MarkdownMessage text={selectedResult.finalMarkdown} />
          <h3>Исходные ответы</h3>
          {selectedResult.reviewers.map((review) => (
            <details key={review.model}>
              <summary>{review.model} · {review.status}</summary>
              {review.error ? <p>{review.error}</p> : <MarkdownMessage text={review.content} />}
            </details>
          ))}
        </div>
      ) : null}

      <History events={events} onOpen={(id) => void openHistory(id)} />
    </section>
  )
}

function History({ events, onOpen }: { readonly events: readonly CoreEvent[]; readonly onOpen: (id: string) => void }): React.JSX.Element {
  const payload = latestPayload(events, 'review.list')
  const reviews = Array.isArray(payload?.reviews) ? payload.reviews : []
  return (
    <div className="review-panel__history">
      <h3>История запусков</h3>
      {reviews.length === 0 ? <p>Завершённых ревью пока нет.</p> : reviews.map((item) => (
        <button key={String(item.review_id)} type="button" onClick={() => onOpen(String(item.review_id))}>
          {String(item.file_name)} · {String(item.review_id)}
        </button>
      ))}
    </div>
  )
}

function latestPayload(events: readonly CoreEvent[], type: string): Record<string, unknown> | null {
  const event = events.find((item) => item.eventType === type)
  if (!event) return null
  try {
    const value: unknown = JSON.parse(event.payload)
    return typeof value === 'object' && value !== null ? value as Record<string, unknown> : null
  } catch {
    return null
  }
}

function latestReviewResult(events: readonly CoreEvent[]): PlanReviewResult | null {
  const response = events.find((item) => item.eventType === 'review.result')
  if (response) {
    try {
      const payload = JSON.parse(response.payload) as { result?: Record<string, unknown> | null }
      if (payload.result) return normalizeResult(payload.result)
    } catch {
      // Continue to the durable task event format below.
    }
  }
  const event = events.find((item) => item.eventType === 'task.completed' && item.taskId.startsWith('review-'))
  if (!event) return null
  try {
    const raw = JSON.parse(event.payload) as { TaskCompleted?: { final_message?: string }; final_message?: string }
    const finalMessage = raw.TaskCompleted?.final_message ?? raw.final_message
    return finalMessage ? normalizeResult(JSON.parse(finalMessage) as Record<string, unknown>) : null
  } catch {
    return null
  }
}

function normalizeResult(value: Record<string, unknown>): PlanReviewResult | null {
  const reviewId = value.review_id ?? value.reviewId
  const fileName = value.file_name ?? value.fileName
  const synthesisModel = value.synthesis_model ?? value.synthesisModel
  const finalMarkdown = value.final_markdown ?? value.finalMarkdown
  if (typeof reviewId !== 'string' || typeof fileName !== 'string' || typeof synthesisModel !== 'string' || typeof finalMarkdown !== 'string' || !Array.isArray(value.reviewers)) return null
  return {
    reviewId,
    fileName,
    synthesisModel,
    finalMarkdown,
    reviewers: value.reviewers.map((item) => {
      const review = item as Record<string, unknown>
      return { model: String(review.model ?? ''), status: String(review.status ?? ''), content: String(review.content ?? ''), error: typeof review.error === 'string' ? review.error : null }
    })
  }
}

function makeReviewId(): string {
  return typeof crypto.randomUUID === 'function' ? `review-${crypto.randomUUID()}` : `review-${Date.now()}`
}
