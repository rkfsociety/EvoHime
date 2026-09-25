import { useEffect, useMemo, useRef, useState } from 'react'

import type {
  CapabilityRecipeCatalog,
  CapabilityRecipeDescriptor,
  CapabilityRecipeForkResult,
  CapabilityRecipePreflight,
  CapabilityRecipeRunResult,
  CapabilityRecipeStartResult,
  ConnectionState,
  CoreEvent,
  WorkflowRunProjection
} from '@shared/api'
import { useShellApi } from './shell-api'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly workspace: string | null
  readonly onOpenDraft: (draftId: string, ownerScope: string) => void
}

interface PendingRequest {
  readonly afterSequence: number
  readonly signature: string
  readonly workspacePath?: string
}

function latestPayload<T>(events: readonly CoreEvent[], eventType: string): { event: CoreEvent; payload: T } | null {
  const event = events.find((item) => item.eventType === eventType)
  if (!event) return null
  try {
    return { event, payload: JSON.parse(event.payload) as T }
  } catch {
    return null
  }
}

function isReady(preflight: CapabilityRecipePreflight | null): boolean {
  return preflight?.state === 'ready' || preflight?.state === 'ready_with_warnings'
}

function statusLabel(state: string): string {
  const labels: Readonly<Record<string, string>> = {
    ready: 'готово к запуску',
    ready_with_warnings: 'готово с предупреждением',
    blocked: 'заблокировано политикой',
    unsupported: 'нет совместимого исполнителя',
    invalid_definition: 'неверная версия или входы',
    pending: 'ожидает запуска',
    running: 'выполняется',
    waiting_approval: 'ждёт подтверждения',
    completed: 'завершено',
    failed: 'ошибка',
    degraded: 'частичный результат',
    cancelled: 'отменено',
    interrupted: 'прервано, исход неизвестен'
  }
  return labels[state] ?? `неизвестное состояние (${state})`
}

function maxSequence(events: readonly CoreEvent[], eventType: string): number {
  return events
    .filter((event) => event.eventType === eventType)
    .reduce((max, event) => Math.max(max, event.sequenceId), 0)
}

export function CapabilityRecipePanel({ connection, events, workspace, onOpenDraft }: Props): React.JSX.Element {
  const api = useShellApi()
  const connected = connection === 'connected' || connection === 'replaying' || connection === 'resyncing'
  const [selectedId, setSelectedId] = useState('')
  const [inputs, setInputs] = useState<Record<string, string>>({})
  const [preflight, setPreflight] = useState<CapabilityRecipePreflight | null>(null)
  const [confirmedSignature, setConfirmedSignature] = useState('')
  const [preflightPending, setPreflightPending] = useState<PendingRequest | null>(null)
  const [startPending, setStartPending] = useState<PendingRequest | null>(null)
  const [forkPending, setForkPending] = useState<PendingRequest | null>(null)
  const [runId, setRunId] = useState('')
  const [runWorkspace, setRunWorkspace] = useState<string | null>(null)
  const [notice, setNotice] = useState('')
  const startKeyRef = useRef<{ signature: string; key: string } | null>(null)
  const forkKeyRef = useRef<{ runId: string; key: string } | null>(null)

  const catalogEvent = latestPayload<CapabilityRecipeCatalog>(events, 'capability_recipe.catalog')
  const catalog = catalogEvent?.payload ?? null
  const recipes = catalog?.recipes ?? []
  const selected: CapabilityRecipeDescriptor | null = recipes.find((recipe) => recipe.id === selectedId) ?? null
  const signature = useMemo(
    () => JSON.stringify([selected?.id ?? '', selected?.version ?? 0, selected?.content_hash ?? '', workspace ?? '', inputs]),
    [inputs, selected?.content_hash, selected?.id, selected?.version, workspace]
  )

  const preflightEvent = latestPayload<CapabilityRecipePreflight>(events, 'capability_recipe.preflight')
  const startEvent = latestPayload<CapabilityRecipeStartResult>(events, 'capability_recipe.started')
  const recipeRunEvent = latestPayload<CapabilityRecipeRunResult>(events, 'capability_recipe.run')
  const forkEvent = latestPayload<CapabilityRecipeForkResult>(events, 'capability_recipe.forked')
  const workflowRunEvent = latestPayload<WorkflowRunProjection>(events, 'workflow.run')
  const recipeRunPayload = recipeRunEvent?.payload ?? null
  const recipeRun = recipeRunPayload && recipeRunPayload.recipe_run?.run_id === runId
    ? recipeRunPayload.recipe_run
    : null
  const workflowRun = workflowRunEvent && workflowRunEvent.payload.run_id === runId ? workflowRunEvent.payload : null
  const runState = workflowRun?.state ?? (recipeRunPayload && recipeRun ? recipeRunPayload.run.state : '')
  const runIsTerminal = ['completed', 'failed', 'cancelled', 'degraded', 'interrupted'].includes(runState)

  useEffect(() => {
    if (!api || !connected) return
    void api.invoke('capabilityRecipe.list', {})
  }, [api, connected])

  useEffect(() => {
    if (!selected && recipes.length > 0) {
      setSelectedId((current) => recipes.some((recipe) => recipe.id === current)
        ? current
        : recipes.find((recipe) => recipe.availability.status === 'ready')?.id ?? recipes[0]?.id ?? '')
    }
  }, [recipes, selected])

  useEffect(() => {
    if (!preflightEvent || !preflightPending || preflightEvent.event.sequenceId <= preflightPending.afterSequence) return
    setPreflightPending(null)
    if (preflightPending.signature !== signature) {
      setNotice('Входы или рабочая папка изменились. Повтори проверку перед запуском.')
      return
    }
    setPreflight(preflightEvent.payload)
    setConfirmedSignature(preflightPending.signature)
    setNotice(preflightEvent.payload.error_code ? `Проверка отклонена: ${preflightEvent.payload.error_code}` : '')
  }, [preflightEvent?.event.sequenceId, preflightPending, preflightEvent?.payload, signature])

  useEffect(() => {
    if (!startEvent || !startPending || startEvent.event.sequenceId <= startPending.afterSequence) return
    setStartPending(null)
    if (startEvent.payload.error_code) {
      setNotice(`Core отклонил запуск: ${startEvent.payload.error_code}`)
      return
    }
    startKeyRef.current = null
    setRunId(startEvent.payload.run_id)
    setRunWorkspace(startPending.workspacePath ?? null)
    setNotice(startEvent.payload.deduplicated ? 'Возвращён уже созданный запуск.' : 'Запуск принят Core.')
  }, [startEvent?.event.sequenceId, startPending, startEvent?.payload])

  useEffect(() => {
    if (!forkEvent || !forkPending || forkEvent.event.sequenceId <= forkPending.afterSequence) return
    if (forkEvent.payload.source_run_id !== forkPending.signature) return
    setForkPending(null)
    if (forkEvent.payload.status !== 'draft_created' || forkEvent.payload.error_code) {
      setNotice(`Не удалось создать draft: ${forkEvent.payload.error_code || 'fork_refused'}`)
      return
    }
    setNotice(`Создан draft ${forkEvent.payload.draft_id} · revision ${forkEvent.payload.revision}.`)
    if (forkPending.signature !== runId || !runWorkspace) return
    if (workspace !== runWorkspace) {
      setNotice('Draft сохранён в исходном workspace. Переключись на него и повтори действие, чтобы открыть draft.')
      return
    }
    onOpenDraft(forkEvent.payload.draft_id, runWorkspace)
  }, [forkEvent?.event.sequenceId, forkPending, forkEvent?.payload, onOpenDraft, runId, runWorkspace, workspace])

  useEffect(() => {
    if (!api || !connected || !runId) return
    const refresh = (): void => {
      void api.invoke('workflow.getRun', { runId })
      void api.invoke('capabilityRecipe.getRun', { runId })
    }
    refresh()
    if (runIsTerminal) return
    const timer = setInterval(refresh, 2_000)
    return () => clearInterval(timer)
  }, [api, connected, runId, runIsTerminal])

  const changeRecipe = (recipeId: string): void => {
    setSelectedId(recipeId)
    setInputs({})
    setPreflight(null)
    setConfirmedSignature('')
    setPreflightPending(null)
    setNotice('')
    setRunId('')
    setRunWorkspace(null)
    startKeyRef.current = null
  }

  const check = async (): Promise<void> => {
    if (!api || !selected || !workspace) {
      setNotice('Подключи Core и выбери рабочую папку.')
      return
    }
    setPreflight(null)
    setConfirmedSignature('')
    setNotice('Проверяю recipe, workflow binding и текущую policy…')
    const pending = { afterSequence: maxSequence(events, 'capability_recipe.preflight'), signature }
    setPreflightPending(pending)
    const outcome = await api.invoke('capabilityRecipe.preflight', {
      recipeId: selected.id,
      recipeVersion: selected.version,
      recipeHash: selected.content_hash,
      workspacePath: workspace,
      inputs
    })
    if (!outcome.ok) {
      setPreflightPending(null)
      setNotice(outcome.message)
    }
  }

  const start = async (): Promise<void> => {
    if (
      !api ||
      !selected ||
      selected.availability.status !== 'ready' ||
      !workspace ||
      !preflight ||
      !isReady(preflight) ||
      confirmedSignature !== signature
    ) {
      setNotice('Сначала заверши актуальную проверку recipe.')
      return
    }
    const key = startKeyRef.current?.signature === signature
      ? startKeyRef.current.key
      : crypto.randomUUID()
    startKeyRef.current = { signature, key }
    const pending = { afterSequence: maxSequence(events, 'capability_recipe.started'), signature, workspacePath: workspace }
    setStartPending(pending)
    setNotice('Core запускает подтверждённый workflow…')
    const outcome = await api.invoke('capabilityRecipe.start', {
      recipeId: selected.id,
      recipeVersion: selected.version,
      recipeHash: selected.content_hash,
      workspacePath: workspace,
      inputs,
      idempotencyKey: key,
      preflightHash: preflight.preflight_hash
    })
    if (!outcome.ok) {
      setStartPending(null)
      setNotice(outcome.message)
    }
  }

  const cancel = async (): Promise<void> => {
    if (!api || !runId || !runWorkspace) return
    const outcome = await api.invoke('workflow.cancel', { runId })
    if (!outcome.ok) setNotice(outcome.message)
  }

  const fork = async (): Promise<void> => {
    if (!api || !runId || !runWorkspace) return
    const key = forkKeyRef.current?.runId === runId ? forkKeyRef.current.key : crypto.randomUUID()
    forkKeyRef.current = { runId, key }
    const pending = { afterSequence: maxSequence(events, 'capability_recipe.forked'), signature: runId }
    setForkPending(pending)
    setNotice('Создаю отдельный draft из исходного шаблона…')
    const outcome = await api.invoke('capabilityRecipe.forkRun', { runId, idempotencyKey: key })
    if (!outcome.ok) {
      setForkPending(null)
      setNotice(outcome.message)
    }
  }

  const ready = selected?.availability.status === 'ready' && isReady(preflight) && confirmedSignature === signature

  return (
    <section className="settings-info workflow-recipe" aria-label="Guided capability recipes">
      <h3>Готовые сценарии</h3>
      <p>Каталог и решения о запуске принадлежат Core. Исполняются только сценарии с подтверждённым workflow binding.</p>
      {!connected ? <p role="status">Ядро недоступно — каталог и состояние запуска не обновляются.</p> : null}
      {catalog?.error_code ? <p role="alert">Каталог недоступен: {catalog.error_code}</p> : null}
      <label>
        Сценарий
        <select value={selectedId} onChange={(event) => changeRecipe(event.target.value)} disabled={!connected || recipes.length === 0 || Boolean(startPending)}>
          {recipes.map((recipe) => (
            <option key={recipe.id} value={recipe.id}>
              {recipe.title}{recipe.availability.status === 'unsupported' ? ' · недоступен' : ''}
            </option>
          ))}
        </select>
      </label>
      {selected ? (
        <>
          <p>{selected.description}</p>
          {selected.availability.status === 'unsupported' ? (
            <p role="status">Запуск недоступен: {selected.availability.reason_code}</p>
          ) : (
            <p>Binding: {selected.workflow_binding?.template_id} · версия {selected.workflow_binding?.template_version}</p>
          )}
          {selected.inputs.map((input) => (
            <label key={input.name}>
              {input.title}{input.required ? ' *' : ''}
              <input
                type="text"
                maxLength={input.max_chars}
                value={inputs[input.name] ?? ''}
                disabled={Boolean(startPending)}
                onChange={(event) => {
                  setInputs((current) => ({ ...current, [input.name]: event.target.value }))
                  setPreflight(null)
                  setConfirmedSignature('')
                  setPreflightPending(null)
                  startKeyRef.current = null
                }}
              />
            </label>
          ))}
          <div>
            <button type="button" disabled={!api || !connected || !workspace || Boolean(preflightPending) || Boolean(startPending)} onClick={() => void check()}>
              Проверить
            </button>{' '}
            <button type="button" disabled={!api || !connected || !ready || Boolean(startPending)} onClick={() => void start()}>
              Запустить
            </button>
          </div>
        </>
      ) : recipes.length === 0 && connected ? <p role="status">Core ещё не прислал каталог сценариев.</p> : null}

      {preflight && confirmedSignature === signature ? (
        <div aria-label="Результат проверки сценария">
          <h4>Проверка Core: {statusLabel(preflight.state)}</h4>
          {preflight.preview.length > 0 ? <ol>{preflight.preview.map((line) => <li key={line}>{line}</li>)}</ol> : null}
          {preflight.reason_codes.length > 0 ? <p>Коды: {preflight.reason_codes.join(', ')}</p> : null}
          {preflight.required_capabilities.length > 0 ? <p>Требуются: {preflight.required_capabilities.join(', ')}</p> : null}
          {preflight.optional_capabilities.length > 0 ? <p>Дополнительно: {preflight.optional_capabilities.join(', ')}</p> : null}
          {preflight.workflow_budget ? (
            <p>
              Бюджет: {preflight.workflow_budget.max_parallel_nodes} параллельных узлов · {preflight.workflow_budget.max_tokens} токенов · {preflight.workflow_budget.max_tool_calls} вызовов · {preflight.workflow_budget.max_wall_clock_ms} мс
            </p>
          ) : null}
          {preflight.approval_points.length > 0 ? <p>Требуют подтверждения: {preflight.approval_points.join(', ')}</p> : null}
          {preflight.degraded_paths.length > 0 ? <p>Перепроверяются при запуске: {preflight.degraded_paths.join(', ')}</p> : null}
          {preflight.revisions.length > 0 ? (
            <details>
              <summary>Зафиксированные и отсутствующие revisions</summary>
              <ul>
                {preflight.revisions.map((revision) => (
                  <li key={`${revision.owner_kind}:${revision.owner_id}`}>
                    {revision.owner_kind} · {revision.owner_id} · {revision.state === 'pinned' ? `v${revision.revision ?? '—'}` : `не закреплён (${revision.reason_code})`}
                    {revision.content_hash ? ` · ${revision.content_hash}` : ''}
                  </li>
                ))}
              </ul>
            </details>
          ) : null}
          {preflight.run_graph_hash ? <small>Graph hash: {preflight.run_graph_hash}</small> : null}
        </div>
      ) : null}

      {runId ? (
        <div aria-label="Состояние сценария">
          <h4>Запуск {runId}</h4>
          <p role="status">{statusLabel(runState || 'unknown_state')}</p>
          {workflowRun ? (
            <ol>
              {workflowRun.nodes.map((node) => (
                <li key={node.node_id}>{node.node_id} · {node.action_kind} · {statusLabel(node.state)}{node.error_code ? ` · ${node.error_code}` : ''}</li>
              ))}
            </ol>
          ) : null}
          {recipeRun ? (
            <details>
              <summary>Происхождение и повторяемость</summary>
              <p>{recipeRun.recipe_id} v{recipeRun.recipe_version} · {recipeRun.template_id} v{recipeRun.template_version}</p>
              <p>Recipe hash: {recipeRun.recipe_hash}</p>
              <p>Template hash: {recipeRun.template_graph_hash}</p>
              <p>Input hash: {recipeRun.input_hash}</p>
              <p>Workspace hash: {recipeRun.workspace_hash}</p>
              {recipeRunEvent?.payload.replay_options ? (
                <>
                  <p>ReproduceExact: недоступно ({recipeRunEvent.payload.replay_options.reproduce_exact.reason_code})</p>
                  <p>ReRunWithCurrentCompatible: недоступно ({recipeRunEvent.payload.replay_options.rerun_current_compatible.reason_code})</p>
                </>
              ) : null}
              <p>ForkAndModify создаёт отдельный draft из исходного шаблона после успешного запуска.</p>
            </details>
          ) : null}
          <button type="button" disabled={!api || !connected || ['completed', 'failed', 'cancelled', 'degraded', 'interrupted'].includes(runState)} onClick={() => void cancel()}>
            Отменить запуск
          </button>{' '}
          <button type="button" disabled={!api || !connected || runState !== 'completed' || Boolean(forkPending)} onClick={() => void fork()}>
            Создать draft из шаблона
          </button>
        </div>
      ) : null}
      {notice ? <p role="status">{notice}</p> : null}
    </section>
  )
}
