import { useEffect, useMemo, useRef, useState } from 'react'

import type { ConnectionState, CoreEvent, ModelLimits, ModelTier, PlanFile, PlanReviewResult } from '@shared/api'

import { useShellApi } from './shell-api'
import { MarkdownMessage } from './MarkdownMessage'

const CONNECTED: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']
const MIN_REVIEWERS = 2
const MAX_REVIEWERS = 8
const REVIEW_PREFERENCES_KEY = 'evohime.review-preferences.v2'
// Папка планов хранится отдельно от выбора моделей: она переживает смену
// каталога моделей и не должна теряться при сбросе настроек ревью.
const PLAN_DIRECTORY_KEY = 'evohime.review-plan-directory.v1'
// A review that never reaches the core says so within a second, so a short
// wait is enough to call the launch itself broken. A review that is running,
// though, is at the mercy of the slowest free model, and warning on total time
// cried wolf on healthy runs — what matters there is how long nothing at all
// has happened.
const SILENT_LAUNCH_SECONDS = 20
const SILENT_PROGRESS_SECONDS = 180
// Тот же потолок, что и в мосте оболочки и в ядре: считаем по склеенному
// документу, потому что именно он уходит в ревью.
const MAX_PLAN_BYTES = 512 * 1024
// Ядро обрезает ответ рецензента на этой границе, и синтезатор получает все
// ответы разом — это и есть худший случай, который надо уметь назвать.
const MAX_REVIEW_OUTPUT_BYTES = 256 * 1024
// Та же грубая оценка, что и у HeuristicEstimator ядра: 3 байта на токен.
// Точный токенизатор здесь недоступен, а расхождение играет в запас — реальные
// тексты дают меньше токенов, чем эта формула.
const BYTES_PER_TOKEN = 3
// Системное сообщение и инструкция рецензента, которые ядро добавляет к плану.
const PROMPT_OVERHEAD_TOKENS = 300
// Ответу нужно место в том же окне. Больше 8k рецензент всё равно не напишет —
// ядро обрежет его раньше по MAX_REVIEW_OUTPUT_BYTES.
const OUTPUT_RESERVE_TOKENS = 8_192

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

interface ReviewFailure {
  readonly kind: 'failed' | 'stopped'
  readonly message: string
}

/**
 * Reviewers of one run, accumulated as their events arrive.
 *
 * The shell keeps only the newest events, so a long run pushes its own early
 * events out of that window. Deriving the roster from the window alone made
 * finished reviewers disappear from the list and left the survivors showing
 * whatever stale status was still visible.
 */
interface ReviewRoster {
  readonly reviewId: string
  readonly models: readonly string[]
  readonly statuses: Readonly<Record<string, string>>
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

function loadPlanDirectory(): string {
  try {
    return window.localStorage.getItem(PLAN_DIRECTORY_KEY) ?? ''
  } catch {
    return ''
  }
}

/**
 * Папка файла по его пути. Renderer не знает про `node:path`, а разделитель
 * зависит от системы, поэтому берётся последний из встреченных.
 */
function parentDirectory(path: string): string {
  const cut = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'))
  return cut > 0 ? path.slice(0, cut) : ''
}

function savePlanDirectory(directory: string): void {
  if (typeof directory !== 'string' || directory.length === 0) return
  try {
    window.localStorage.setItem(PLAN_DIRECTORY_KEY, directory)
  } catch {
    // Запоминание папки — удобство, а не условие запуска ревью.
  }
}

export function PlanReviewPanel({ connection, events }: Props): React.JSX.Element {
  const api = useShellApi()
  const [preferences] = useState(loadReviewPreferences)
  const [plans, setPlans] = useState<readonly PlanFile[]>([])
  const [dragging, setDragging] = useState(false)
  const [tier, setTier] = useState<ModelTier>(preferences.tier)
  const [models, setModels] = useState<readonly string[]>([])
  const [limits, setLimits] = useState<Readonly<Record<string, ModelLimits>>>({})
  const [reviewerCount, setReviewerCount] = useState(Math.max(MIN_REVIEWERS, Math.min(MAX_REVIEWERS, preferences.reviewerModels.length || MIN_REVIEWERS)))
  const [reviewers, setReviewers] = useState<readonly string[]>(normalizeSlots(preferences.reviewerModels, reviewerCount))
  const [synthesisModel, setSynthesisModel] = useState(preferences.synthesisModel)
  const [reviewId, setReviewId] = useState<string | null>(null)
  const [selectedResult, setSelectedResult] = useState<PlanReviewResult | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [startedAt, setStartedAt] = useState<number | null>(null)
  const [now, setNow] = useState(() => Date.now())
  const [lastChangeAt, setLastChangeAt] = useState(() => Date.now())
  const [copied, setCopied] = useState(false)
  const [roster, setRoster] = useState<ReviewRoster>({ reviewId: '', models: [], statuses: {} })
  // Перетаскивание над вложенными узлами шлёт dragleave родителю, поэтому
  // подсветка снимается по счётчику входов, а не по первому же выходу.
  const dragDepth = useRef(0)

  // Ядро принимает один документ, поэтому несколько планов склеиваются здесь:
  // рецензенты читают их как разделы одного плана и видят связи между ними.
  const sourceMarkdown = useMemo(() => mergePlans(plans), [plans])
  const fileName = useMemo(() => describePlans(plans), [plans])
  const catalog = useMemo(() => latestCatalog(events, tier), [events, tier])
  const checks = useMemo(() => preflight(sourceMarkdown, reviewers, synthesisModel, limits), [limits, reviewers, sourceMarkdown, synthesisModel])
  const reviewResult = useMemo(() => latestReviewResult(events), [events])
  const progress = useMemo(() => reviewId ? latestReviewProgress(events, reviewId) : null, [events, reviewId])
  const failure = useMemo(() => reviewId ? latestReviewFailure(events, reviewId) : null, [events, reviewId])
  const accepted = useMemo(() => reviewId !== null && (progress !== null || events.some((item) => item.eventType === 'review.started' && readPayload(item)?.review_id === reviewId)), [events, progress, reviewId])
  const reviewFinished = reviewResult?.reviewId === reviewId
  const running = reviewId !== null && !reviewFinished && failure === null && progress?.stage !== 'completed' && progress?.stage !== 'failed'
  const progressKey = progress === null ? '' : `${progress.stage}:${progress.status}:${progress.model ?? ''}:${progress.completed}`
  const elapsed = startedAt === null ? 0 : Math.round((now - startedAt) / 1000)
  const silence = Math.round((now - lastChangeAt) / 1000)

  useEffect(() => {
    if (!api || !CONNECTED.includes(connection)) return
    // Refreshing the catalogue costs a provider request, and doing it during a
    // run competes with the review for the same rate limit.
    if (running) return
    void api.invoke('core.listModelCatalog', { mode: tier })
    void api.invoke('review.list', { limit: 20 })
  }, [api, connection, running, tier])

  // A failed catalogue request answers with an empty list. Applying it would
  // erase the models the user picked — including those of a review in flight —
  // so an empty catalogue is treated as "no news", not as "no models".
  useEffect(() => {
    if (catalog.models.length === 0) return
    const available = catalog.models
    setModels(available)
    // Лимиты обновляются только вместе с непустым каталогом — по той же
    // причине, по которой не затирается список моделей.
    setLimits(catalog.limits)
    setReviewers((current) => normalizeSlots(current.filter((model) => available.includes(model)), reviewerCount, available))
    setSynthesisModel((current) => available.includes(current) ? current : available[0] || '')
  }, [catalog, reviewerCount])

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

  // A visible clock is the only proof that a silent run is still alive.
  useEffect(() => {
    if (!running || startedAt === null) return
    setNow(Date.now())
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [running, startedAt])

  // Silence is measured from the last change the core reported, not from the
  // start: a reviewer that is still thinking keeps the run visibly alive.
  useEffect(() => {
    setLastChangeAt(Date.now())
  }, [progressKey, reviewId])

  // Applied oldest to newest so the last word on a model wins, and kept in
  // state so a model never leaves the list once the core has named it.
  useEffect(() => {
    if (reviewId === null) return
    setRoster((current) => {
      const fresh = current.reviewId !== reviewId
      const models = fresh ? [] : [...current.models]
      const statuses: Record<string, string> = fresh ? {} : { ...current.statuses }
      let changed = fresh
      for (const event of [...events].reverse()) {
        if (event.eventType !== 'review.progress') continue
        const payload = readPayload(event)
        if (payload?.review_id !== reviewId || payload.stage !== 'reviewers') continue
        const model = typeof payload.model === 'string' ? payload.model : ''
        if (model.length === 0) continue
        const status = String(payload.status ?? 'waiting')
        if (!models.includes(model)) {
          models.push(model)
          changed = true
        }
        if (statuses[model] !== status) {
          statuses[model] = status
          changed = true
        }
      }
      return changed ? { reviewId, models, statuses } : current
    })
  }, [events, reviewId])

  // Файл, брошенный мимо зоны, иначе открылся бы вместо оболочки: Electron
  // считает такой drop навигацией и выгружает окно вместе с текущим ревью.
  useEffect(() => {
    const swallow = (event: DragEvent): void => event.preventDefault()
    window.addEventListener('dragover', swallow)
    window.addEventListener('drop', swallow)
    return () => {
      window.removeEventListener('dragover', swallow)
      window.removeEventListener('drop', swallow)
    }
  }, [])

  const pick = async (): Promise<void> => {
    if (!api) return
    const outcome = await api.invoke('review.pickPlan', { directory: loadPlanDirectory() })
    if (!outcome.ok) {
      setError(outcome.message)
      return
    }
    if (outcome.value.cancelled) return
    savePlanDirectory(outcome.value.directory)
    addPlans(outcome.value.files, [])
  }

  /**
   * Список пополняется, а не заменяется: набрать план из нескольких файлов
   * можно и в несколько заходов — диалогом, перетаскиванием или вперемешку.
   * Лишнее снимается крестиком у файла или кнопкой «Очистить список».
   */
  const addPlans = (incoming: readonly PlanFile[], rejected: readonly string[]): void => {
    const merged = [...plans]
    for (const plan of incoming) {
      if (merged.some((item) => item.fileName === plan.fileName && item.sourceMarkdown === plan.sourceMarkdown)) continue
      merged.push(plan)
    }
    if (byteLength(mergePlans(merged)) > MAX_PLAN_BYTES) {
      setError('Планы в сумме превышают 512 КБ — ядро такой документ не примет. Убери часть файлов.')
      return
    }
    setPlans(merged)
    setSelectedResult(null)
    setReviewId(null)
    setError(rejected.length > 0 ? `Приняты только .md: пропущены ${rejected.join(', ')}.` : null)
  }

  const removePlan = (index: number): void => {
    setPlans((current) => current.filter((_, position) => position !== index))
  }

  const drop = async (transfer: DataTransfer): Promise<void> => {
    const dropped = Array.from(transfer.files)
    if (dropped.length === 0) return
    const accepted = dropped.filter((file) => file.name.toLowerCase().endsWith('.md'))
    const rejected = dropped.filter((file) => !file.name.toLowerCase().endsWith('.md')).map((file) => file.name)
    if (accepted.length === 0) {
      setError(`Приняты только .md: пропущены ${rejected.join(', ')}.`)
      return
    }
    const loaded: PlanFile[] = []
    for (const file of accepted) loaded.push({ fileName: file.name, sourceMarkdown: await file.text() })
    // Папку запоминаем и по брошенным файлам: иначе тот, кто планы только
    // перетаскивает, каждый раз получал бы диалог в папке загрузок. Путь —
    // удобство, поэтому его отсутствие не мешает добавить сами файлы.
    const [first] = accepted
    if (first) {
      try {
        savePlanDirectory(parentDirectory(api?.pathForFile(first) ?? ''))
      } catch {
        // Файл мог прийти не из файловой системы — папки у него просто нет.
      }
    }
    addPlans(loaded, rejected)
  }

  const changeCount = (count: number): void => {
    setReviewerCount(count)
    setReviewers((current) => normalizeSlots(current, count, models))
  }

  const setReviewerModel = (index: number, model: string): void => {
    setReviewers((current) => current.map((item, position) => position === index ? model : item))
  }

  const start = async (): Promise<void> => {
    if (!api || !fileName || !sourceMarkdown || reviewers.length < MIN_REVIEWERS || reviewers.some((model) => !model) || new Set(reviewers).size !== reviewers.length || !synthesisModel || checks.tooSmall.length > 0) return
    const id = makeReviewId()
    setReviewId(id)
    setSelectedResult(null)
    setError(null)
    setStartedAt(Date.now())
    setNow(Date.now())
    setLastChangeAt(Date.now())
    const outcome = await api.invoke('review.start', { reviewId: id, fileName, sourceMarkdown, reviewerModels: reviewers, synthesisModel })
    if (!outcome.ok) setError(`Ядро отклонило запрос: ${outcome.message}`)
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
    setStartedAt(null)
    setError(null)
  }

  const clearHistory = async (): Promise<void> => {
    if (!api) return
    const outcome = await api.invoke('review.clearHistory', {})
    if (!outcome.ok) {
      setError(outcome.message)
      return
    }
    // The list is rebuilt from the core, so it must be re-read after clearing.
    await api.invoke('review.list', { limit: 20 })
  }

  const copyResult = async (): Promise<void> => {
    if (!api || !selectedResult) return
    if (!(await api.writeClipboardText(selectedResult.finalMarkdown))) {
      setError('Не удалось скопировать итог в буфер обмена.')
      return
    }
    setCopied(true)
    window.setTimeout(() => setCopied(false), 1400)
  }

  const exportResult = async (): Promise<void> => {
    if (!api || !selectedResult) return
    const outcome = await api.invoke('review.export', { reviewId: selectedResult.reviewId, destinationPath: '', includeReviewers: false })
    if (!outcome.ok) setError(outcome.message)
  }

  const canStart = CONNECTED.includes(connection) && fileName.length > 0 && reviewers.length >= MIN_REVIEWERS && reviewers.every(Boolean) && new Set(reviewers).size === reviewers.length && synthesisModel.length > 0 && !running && checks.tooSmall.length === 0
  const status = reviewStatus(progress, reviewFinished, selectedResult !== null, failure, accepted)
  // Without a stall hint a dead core is indistinguishable from a slow model.
  const launchStalled = running && progress === null && elapsed >= SILENT_LAUNCH_SECONDS
  const progressStalled = running && progress !== null && silence >= SILENT_PROGRESS_SECONDS

  return (
    <section className="review-panel" aria-label="Ревью планов">
      <div
        role="group"
        aria-label="Планы на ревью"
        className={`review-panel__dropzone${dragging ? ' review-panel__dropzone--active' : ''}`}
        onDragEnter={(event) => { event.preventDefault(); dragDepth.current += 1; setDragging(true) }}
        onDragOver={(event) => event.preventDefault()}
        onDragLeave={(event) => { event.preventDefault(); dragDepth.current -= 1; if (dragDepth.current <= 0) { dragDepth.current = 0; setDragging(false) } }}
        onDrop={(event) => { event.preventDefault(); dragDepth.current = 0; setDragging(false); if (!running) void drop(event.dataTransfer) }}
      >
        <div className="review-panel__toolbar">
          <button type="button" onClick={() => void pick()}>Выбрать Markdown-планы</button>
          {plans.length > 0 ? <button type="button" onClick={() => setPlans([])} disabled={running}>Очистить список</button> : null}
          <span className="review-panel__hint">Можно выбрать несколько файлов сразу или перетащить их сюда.</span>
        </div>
        {plans.length === 0 ? <p className="review-panel__empty">Файлы не выбраны.</p> : (
          <ul className="review-panel__files">
            {plans.map((plan, index) => (
              <li key={`${plan.fileName}-${index}`}>
                <span className="review-panel__file">{plan.fileName}</span>
                <small>{plan.sourceMarkdown.length} символов</small>
                <button type="button" aria-label={`Убрать ${plan.fileName}`} onClick={() => removePlan(index)} disabled={running}>×</button>
              </li>
            ))}
          </ul>
        )}
      </div>
      {plans.length > 0 ? <p className="review-panel__source">Файлов: {plans.length} · загружено символов: {sourceMarkdown.length}</p> : null}

      <div className="review-panel__controls">
        <label>Режим каталога<select value={tier} onChange={(event) => setTier(event.target.value as ModelTier)} disabled={running}><option value="free">Бесплатные</option><option value="paid">Платные</option></select></label>
        <label>Количество рецензентов<select value={reviewerCount} onChange={(event) => changeCount(Number(event.target.value))} disabled={running}>{Array.from({ length: MAX_REVIEWERS - MIN_REVIEWERS + 1 }, (_, index) => MIN_REVIEWERS + index).map((count) => <option key={count} value={count}>{count}</option>)}</select></label>
      </div>

      <fieldset className="review-panel__models">
        <legend>Модели-рецензенты · {reviewers.filter(Boolean).length} из {reviewerCount}</legend>
        {catalog.error ? <p role="alert" className="review-panel__catalog-error">Каталог моделей не обновился: {catalog.error}{models.length > 0 ? ' Показан прошлый список.' : ''}</p> : null}
        {models.length === 0 ? <p>Каталог {tier === 'free' ? 'бесплатных' : 'платных'} моделей пуст.</p> : reviewers.map((model, index) => (
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

      {plans.length > 0 ? <LimitsCard checks={checks} /> : null}
      <div className="review-panel__actions"><button type="button" onClick={() => void start()} disabled={!canStart}>{failure !== null || progress?.stage === 'failed' ? 'Повторить ревью' : reviewId && reviewFinished ? 'Запустить снова' : 'Запустить ревью'}</button><button type="button" onClick={() => void stop()} disabled={!running}>Остановить</button></div>
      {reviewId ? <ProgressCard roster={roster} progress={progress} status={status} reviewers={reviewers} elapsed={startedAt === null ? null : elapsed} accepted={accepted} failed={failure !== null} /> : null}
      {launchStalled ? <p role="alert" className="shell__reason">Ядро не подтвердило запуск за {elapsed} с. Проверь, запущено ли ядро и настроен ли провайдер, либо останови ревью и запусти снова.</p> : null}
      {progressStalled ? <p role="alert" className="shell__reason">Модели молчат уже {Math.floor(silence / 60)} мин. Бесплатные модели отвечают медленно — можно подождать или остановить ревью.</p> : null}
      {failure ? <p role="alert" className="shell__reason">{failure.kind === 'stopped' ? 'Ревью остановлено.' : `Ревью завершилось ошибкой: ${failure.message}`}</p> : null}
      {error ? <p role="alert" className="shell__reason">{error}</p> : null}

      {selectedResult ? <div className="review-panel__result"><div className="review-panel__result-heading"><h3>Итоговое ревью</h3><div className="review-panel__result-actions"><button type="button" onClick={() => void copyResult()}>{copied ? 'Скопировано' : 'Скопировать итоговый Markdown'}</button><button type="button" onClick={() => void exportResult()}>Экспортировать итоговый Markdown</button></div></div><MarkdownMessage text={selectedResult.finalMarkdown} /><h3>Исходные ответы</h3>{selectedResult.reviewers.map((review) => <details key={review.model}><summary>{review.model} · {review.status}</summary>{review.error ? <p>{review.error}</p> : <MarkdownMessage text={review.content} />}</details>)}</div> : null}
      <History events={events} onOpen={(id) => void openHistory(id)} onClear={() => void clearHistory()} />
    </section>
  )
}

/**
 * Что известно про запуск до его начала. Блокирующее сообщение отделено от
 * предупреждения намеренно: первое — арифметика по окну модели, второе —
 * догадка о том, насколько многословными окажутся рецензенты.
 */
function LimitsCard({ checks }: { readonly checks: Preflight }): React.JSX.Element {
  const synthesisRisk = checks.synthesisContext !== null && checks.synthesisWorstTokens > checks.synthesisContext
  return (
    <div className="review-panel__limits">
      <p>Объём запроса ≈ {formatTokens(checks.planTokens)} токенов на каждого рецензента.</p>
      {checks.tooSmall.length > 0 ? (
        <p role="alert" className="review-panel__limit-error">
          Не влезает в окно: {checks.tooSmall.map((item) => `${item.model} (${formatTokens(item.context)}${item.role === 'synthesis' ? ', синтезатор' : ''})`).join(', ')}. Убери часть файлов или выбери модель с бо́льшим окном.
        </p>
      ) : null}
      {checks.tooSmall.length === 0 && synthesisRisk ? (
        <p className="review-panel__limit-warning">
          В худшем случае синтезатор получит ≈ {formatTokens(checks.synthesisWorstTokens)} токенов при окне {formatTokens(checks.synthesisContext ?? 0)}: столько выйдет, только если каждый рецензент упрётся в свой потолок ответа. Обычно ответы короче, но при длинных ревью синтез может не собраться.
        </p>
      ) : null}
      {checks.unknown.length > 0 ? (
        <p className="review-panel__limit-warning">Провайдер не сообщил окно для {checks.unknown.join(', ')} — заранее проверить нельзя.</p>
      ) : null}
    </div>
  )
}

function formatTokens(value: number): string {
  return value.toLocaleString('ru-RU')
}

function ProgressCard({ roster, progress, status, reviewers, elapsed, accepted, failed }: { readonly roster: ReviewRoster; readonly progress: ReviewProgress | null; readonly status: string; readonly reviewers: readonly string[]; readonly elapsed: number | null; readonly accepted: boolean; readonly failed: boolean }): React.JSX.Element {
  // The roster of a run comes from its own events when it has any: the
  // selection above can be re-picked, or emptied by a failed catalogue refresh,
  // and a finished review must still say who reviewed it.
  const queue = roster.models.length > 0 ? roster.models : reviewers
  const completed = progress?.completed ?? 0
  const total = progress?.total || queue.length
  const working = progress?.stage === 'reviewers' && progress.status === 'working' ? progress.model : null
  const hint = failed ? 'Запуск прерван, подробности ниже' : progress?.stage === 'synthesis' ? `Синтез результата · ${progress.model ?? 'модель'}` : working ? `Рецензенты отвечают по очереди · сейчас ${working}` : progress?.stage === 'reviewers' ? 'Рецензенты отвечают по очереди' : accepted ? 'Ядро приняло план, ждём первый ответ модели' : 'Отправляем план в ядро'
  return <div className="review-panel__progress" role="status" aria-live="polite"><div className="review-panel__progress-heading"><strong>{status}</strong>{elapsed === null ? null : <span className="review-panel__elapsed">{elapsed} с</span>}</div><div className="review-panel__progress-bar"><span style={{ width: `${total ? Math.round((Math.min(completed, total) / total) * 100) : 0}%` }} /></div><p>{hint}</p><ul>{queue.map((model, index) => { const state = roster.statuses[model] ?? 'waiting'; return <li key={`${model}-${index}`}><span>{model || `Рецензент ${index + 1}`}</span><span className={`review-panel__reviewer-status review-panel__reviewer-status--${state}`}>{reviewStatusLabel(state)}</span></li> })}</ul></div>
}

function History({ events, onOpen, onClear }: { readonly events: readonly CoreEvent[]; readonly onOpen: (id: string) => void; readonly onClear: () => void }): React.JSX.Element {
  const payload = latestPayload(events, 'review.list')
  const reviews = Array.isArray(payload?.reviews) ? payload.reviews : []
  const [confirming, setConfirming] = useState(false)
  return <div className="review-panel__history"><div className="review-panel__history-heading"><h3>История запусков</h3>{reviews.length === 0 ? null : confirming ? <span className="review-panel__history-confirm"><button type="button" onClick={() => { setConfirming(false); onClear() }}>Очистить</button><button type="button" onClick={() => setConfirming(false)}>Отмена</button></span> : <button type="button" onClick={() => setConfirming(true)}>Очистить историю</button>}</div>{confirming ? <p className="review-panel__history-note">Список очистится, но сами ревью останутся в журнале и в экспортированных файлах.</p> : null}{reviews.length === 0 ? <p>Завершённых ревью пока нет.</p> : reviews.map((item) => { const value = item as Record<string, unknown>; const reviewers = Array.isArray(value.reviewers) ? value.reviewers : []; const completed = reviewers.filter((review) => (review as Record<string, unknown>).status === 'completed').length; return <button key={String(value.review_id)} type="button" onClick={() => onOpen(String(value.review_id))}><span>{String(value.file_name)}</span><small>{completed}/{reviewers.length} рецензентов · {String(value.review_id)}</small></button> })}</div>
}

interface ModelCatalog {
  readonly models: readonly string[]
  readonly limits: Readonly<Record<string, ModelLimits>>
  readonly error: string | null
}

function latestCatalog(events: readonly CoreEvent[], tier: ModelTier): ModelCatalog {
  const event = events.find((item) => item.eventType === 'model.catalog' && readPayload(item)?.mode === tier)
  const payload = readPayload(event)
  const models = payload?.models
  return {
    models: Array.isArray(models) ? models.filter((model): model is string => typeof model === 'string').sort() : [],
    limits: readLimits(payload?.limits),
    error: typeof payload?.error === 'string' && payload.error.length > 0 ? payload.error : null
  }
}

/**
 * Ядро отдаёт лимиты картой `модель → { context, maxOutput }`. Поле может
 * отсутствовать целиком (старое ядро) или содержать `null` там, где провайдер
 * промолчал; и то, и другое читается как «окно неизвестно».
 */
function readLimits(value: unknown): Record<string, ModelLimits> {
  if (typeof value !== 'object' || value === null) return {}
  const limits: Record<string, ModelLimits> = {}
  for (const [model, raw] of Object.entries(value as Record<string, unknown>)) {
    if (typeof raw !== 'object' || raw === null) continue
    const entry = raw as Record<string, unknown>
    limits[model] = {
      context: typeof entry['context'] === 'number' && entry['context'] > 0 ? entry['context'] : null,
      maxOutput: typeof entry['maxOutput'] === 'number' && entry['maxOutput'] > 0 ? entry['maxOutput'] : null
    }
  }
  return limits
}

interface Preflight {
  readonly planTokens: number
  /** Модели, которым план заведомо не влезет: запуск блокируется. */
  readonly tooSmall: readonly { model: string; context: number; role: 'reviewer' | 'synthesis' }[]
  /** Худший случай синтеза и окно синтезатора, если оно известно. */
  readonly synthesisWorstTokens: number
  readonly synthesisContext: number | null
  /** Выбранные модели, про окно которых провайдер ничего не сказал. */
  readonly unknown: readonly string[]
}

/**
 * Считает, влезает ли запуск в окна выбранных моделей.
 *
 * Рецензент получает план целиком, синтезатор — план плюс все ответы
 * рецензентов, каждый до `MAX_REVIEW_OUTPUT_BYTES`. Первое можно проверить
 * точно и потому блокируется; второе зависит от того, насколько многословными
 * окажутся рецензенты, поэтому остаётся предупреждением.
 */
function preflight(
  sourceMarkdown: string,
  reviewers: readonly string[],
  synthesisModel: string,
  limits: Readonly<Record<string, ModelLimits>>
): Preflight {
  const planTokens = Math.ceil(byteLength(sourceMarkdown) / BYTES_PER_TOKEN) + PROMPT_OVERHEAD_TOKENS
  const tooSmall: { model: string; context: number; role: 'reviewer' | 'synthesis' }[] = []
  const unknown: string[] = []
  const selected: { model: string; role: 'reviewer' | 'synthesis' }[] = [
    ...reviewers.filter(Boolean).map((model) => ({ model, role: 'reviewer' as const })),
    ...(synthesisModel ? [{ model: synthesisModel, role: 'synthesis' as const }] : [])
  ]
  for (const { model, role } of selected) {
    const limit = limits[model]
    if (!limit || limit.context === null) {
      if (!unknown.includes(model)) unknown.push(model)
      continue
    }
    const reserve = Math.min(limit.maxOutput ?? OUTPUT_RESERVE_TOKENS, OUTPUT_RESERVE_TOKENS)
    if (planTokens + reserve > limit.context && !tooSmall.some((item) => item.model === model)) {
      tooSmall.push({ model, context: limit.context, role })
    }
  }
  const reviewerCount = reviewers.filter(Boolean).length
  return {
    planTokens,
    tooSmall,
    synthesisWorstTokens: planTokens + reviewerCount * Math.ceil(MAX_REVIEW_OUTPUT_BYTES / BYTES_PER_TOKEN),
    synthesisContext: synthesisModel ? (limits[synthesisModel]?.context ?? null) : null,
    unknown
  }
}

function latestPayload(events: readonly CoreEvent[], type: string): Record<string, unknown> | null { return readPayload(events.find((item) => item.eventType === type)) }

function latestReviewProgress(events: readonly CoreEvent[], reviewId: string): ReviewProgress | null {
  for (const event of events) { if (event.eventType !== 'review.progress') continue; const payload = readPayload(event); if (payload?.review_id !== reviewId) continue; return { reviewId, stage: String(payload.stage ?? ''), status: String(payload.status ?? ''), model: typeof payload.model === 'string' ? payload.model : null, completed: Number(payload.completed ?? 0), total: Number(payload.total ?? 0) } }
  return null
}

function latestReviewFailure(events: readonly CoreEvent[], reviewId: string): ReviewFailure | null {
  for (const event of [...events].reverse()) {
    if (event.eventType !== 'task.failed' && event.eventType !== 'task.stopped') continue
    const payload = readPayload(event)
    const taskId = event.taskId || String(payload?.task_id ?? '')
    if (taskId !== reviewId) continue
    if (event.eventType === 'task.stopped') return { kind: 'stopped', message: 'Ревью остановлено.' }
    const reason = payload?.error
    return { kind: 'failed', message: typeof reason === 'string' && reason.length > 0 ? reason : 'Ядро не сообщило причину.' }
  }
  return null
}

function latestReviewResult(events: readonly CoreEvent[]): PlanReviewResult | null {
  const responsePayload = readPayload(events.find((item) => item.eventType === 'review.result'))
  if (responsePayload?.result && typeof responsePayload.result === 'object') return normalizeResult(responsePayload.result as Record<string, unknown>)
  const payload = readPayload(events.find((item) => item.eventType === 'task.completed' && item.taskId.startsWith('review-')))
  const finalMessage = payload?.final_message
  if (typeof finalMessage !== 'string') return null
  try { return normalizeResult(JSON.parse(finalMessage) as Record<string, unknown>) } catch { return null }
}

function normalizeResult(value: Record<string, unknown>): PlanReviewResult | null {
  const reviewId = value.review_id ?? value.reviewId; const fileName = value.file_name ?? value.fileName; const synthesisModel = value.synthesis_model ?? value.synthesisModel; const finalMarkdown = value.final_markdown ?? value.finalMarkdown
  if (typeof reviewId !== 'string' || typeof fileName !== 'string' || typeof synthesisModel !== 'string' || typeof finalMarkdown !== 'string' || !Array.isArray(value.reviewers)) return null
  return { reviewId, fileName, synthesisModel, finalMarkdown, reviewers: value.reviewers.map((item) => { const review = item as Record<string, unknown>; return { model: String(review.model ?? ''), status: String(review.status ?? ''), content: String(review.content ?? ''), error: typeof review.error === 'string' ? review.error : null } }) }
}

/**
 * Один файл уходит в ревью как есть: подписывать единственный план незачем, а
 * лишний заголовок сместил бы структуру документа. Несколько файлов становятся
 * разделами с явной нумерацией, чтобы модель понимала, где кончается один план
 * и начинается следующий.
 */
function mergePlans(plans: readonly PlanFile[]): string {
  const [first] = plans
  if (!first) return ''
  if (plans.length === 1) return first.sourceMarkdown
  return plans
    .map((plan, index) => `## Файл ${index + 1} из ${plans.length}: ${plan.fileName}\n\n${plan.sourceMarkdown.trim()}`)
    .join('\n\n---\n\n')
}

/** Имя для ядра и истории. Объединённый документ всегда остаётся Markdown. */
function describePlans(plans: readonly PlanFile[]): string {
  const [first] = plans
  if (!first) return ''
  if (plans.length === 1) return first.fileName
  return 'combined-plan.md'
}

function byteLength(value: string): number {
  return new TextEncoder().encode(value).length
}

function normalizeSlots(values: readonly string[], count: number, available: readonly string[] = []): string[] { const result = values.slice(0, count); while (result.length < count) result.push(''); const candidates = available.filter((model) => !result.includes(model)); for (let index = 0; index < result.length; index += 1) if (!result[index] && candidates.length > 0) result[index] = candidates.shift() as string; return result }
function readPayload(event: CoreEvent | undefined): Record<string, unknown> | null { if (!event) return null; try { const value: unknown = JSON.parse(event.payload); return typeof value === 'object' && value !== null ? unwrapVariant(value as Record<string, unknown>) : null } catch { return null } }

/**
 * Core serialises `CoreEvent` as an externally tagged enum, so event fields live
 * one level down under the variant name, while command responses arrive flat.
 * Both reach the panel over the same stream, and reading an event as flat left
 * the progress card blind to a review that was in fact running.
 */
function unwrapVariant(value: Record<string, unknown>): Record<string, unknown> {
  const keys = Object.keys(value)
  const key = keys.length === 1 ? keys[0] : undefined
  if (key === undefined || !/^[A-Z]/.test(key)) return value
  const inner = value[key]
  // An array belongs to a flat response such as `review.list`, not to a variant.
  return typeof inner === 'object' && inner !== null && !Array.isArray(inner) ? inner as Record<string, unknown> : value
}
function reviewStatus(progress: ReviewProgress | null, finished: boolean, hasResult: boolean, failure: ReviewFailure | null, accepted: boolean): string { if (failure) return failure.kind === 'stopped' ? 'Остановлено' : 'Ошибка'; if (hasResult || finished || progress?.stage === 'completed') return 'Готово'; if (progress?.stage === 'failed') return 'Ошибка'; if (progress?.stage === 'synthesis') return 'Синтез результата'; if (progress?.stage === 'reviewers') return `Рецензенты: ${progress.completed}/${progress.total}`; return accepted ? 'Отправлено в ядро' : 'Отправка плана' }
function reviewStatusLabel(status: string): string { if (status === 'completed') return 'готово'; if (status === 'failed') return 'ошибка'; if (status === 'cancelled') return 'отменено'; if (status === 'working') return 'работает'; return 'ожидает' }
function makeReviewId(): string { return typeof crypto.randomUUID === 'function' ? `review-${crypto.randomUUID()}` : `review-${Date.now()}` }
