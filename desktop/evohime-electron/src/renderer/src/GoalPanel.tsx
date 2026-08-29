import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent, GoalProjection } from '@shared/api'

import { useShellApi } from './shell-api'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

const STATUS_LABELS: Record<string, string> = {
  active: 'В работе',
  paused: 'Пауза',
  blocked: 'Заблокирована',
  budget_limited: 'Лимит бюджета',
  completed: 'Завершена',
  failed: 'Ошибка',
  cancelled: 'Отменена'
}

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly workspace: string | null
}

/**
 * Bounded Core projection of durable Goals. The renderer never decides that
 * a criterion passed: the manual button sends an explicit user-decision
 * command and displays the typed result returned by Core. Evidence is minted
 * by Core, never supplied by this renderer.
 */
export function GoalPanel({ connection, events, workspace }: Props): React.JSX.Element {
  const api = useShellApi()
  const [goals, setGoals] = useState<readonly GoalProjection[]>([])
  const [objective, setObjective] = useState('')
  const [criterion, setCriterion] = useState('')
  const [message, setMessage] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const listProjection = useMemo(
    () => events.find((event) => event.goalList)?.goalList ?? null,
    [events]
  )
  const actionEvent = useMemo(
    () => events.find((event) => event.goalAction)?.goalAction ?? null,
    [events]
  )

  useEffect(() => {
    setGoals([])
    setMessage(null)
  }, [workspace])

  useEffect(() => {
    if (listProjection) {
      setGoals(listProjection.goals)
      if (listProjection.errorCode) setMessage(`Список целей: ${listProjection.errorCode}`)
    }
  }, [listProjection])

  useEffect(() => {
    const goal = actionEvent?.goal
    if (goal) setGoals((current) => mergeGoal(current, goal))
    if (actionEvent?.errorCode) setMessage(`Core: ${actionEvent.errorMessage || actionEvent.errorCode}`)
  }, [actionEvent])

  useEffect(() => {
    if (!api || !workspace || !CONNECTED_STATES.includes(connection)) return
    void api.invoke('core.listGoals', { workspacePath: workspace, limit: 64 })
  }, [api, connection, workspace])

  const createGoal = async (event: React.FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault()
    if (!api || !workspace || objective.trim() === '' || criterion.trim() === '') return
    setBusy(true)
    setMessage(null)
    const outcome = await api.invoke('core.createGoal', {
      goalId: `goal-${makeId()}`,
      workspacePath: workspace,
      objective: objective.trim(),
      successCriteria: [{ id: 'criterion-1', kind: 'manual', statement: criterion.trim() }],
      idempotencyKey: `goal:create:${makeId()}`
    })
    setBusy(false)
    if (!outcome.ok) setMessage(outcome.message)
    else {
      setObjective('')
      setCriterion('')
      setMessage('Цель отправлена в Core; жду typed-проекцию.')
    }
  }

  const transition = async (goal: GoalProjection, action: 'pauseGoal' | 'resumeGoal' | 'cancelGoal'): Promise<void> => {
    if (!api) return
    setBusy(true)
    setMessage(null)
    const outcome = await api.invoke(`core.${action}`, {
      goalId: goal.goalId,
      expectedVersion: goal.version,
      idempotencyKey: `goal:${action}:${goal.goalId}:${makeId()}`
    })
    setBusy(false)
    if (!outcome.ok) setMessage(outcome.message)
  }

  const verify = async (goal: GoalProjection, criterionId: string): Promise<void> => {
    if (!api) return
    setBusy(true)
    setMessage(null)
    const outcome = await api.invoke('core.verifyGoalCriterion', {
      goalId: goal.goalId,
      expectedVersion: goal.version,
      criterionId,
      idempotencyKey: `goal:verify:${goal.goalId}:${criterionId}:${makeId()}`
    })
    setBusy(false)
    if (!outcome.ok) setMessage(outcome.message)
  }

  return (
    <section className="goal-panel" aria-label="Постоянные цели">
      <div className="goal-panel__header">
        <div>
          <span className="goal-panel__eyebrow">Core Goal v1</span>
          <h3>Постоянные цели</h3>
        </div>
        <span className="goal-panel__count">{goals.length || '—'}</span>
      </div>

      {workspace ? (
        <form className="goal-create" onSubmit={(event) => void createGoal(event)}>
          <input
            aria-label="Новая цель"
            value={objective}
            onChange={(event) => setObjective(event.target.value)}
            placeholder="Что должно быть достигнуто?"
            maxLength={4096}
          />
          <input
            aria-label="Критерий успеха"
            value={criterion}
            onChange={(event) => setCriterion(event.target.value)}
            placeholder="Как Core подтвердит результат?"
            maxLength={4096}
          />
          <button type="submit" disabled={busy || objective.trim() === '' || criterion.trim() === ''}>Создать цель</button>
        </form>
      ) : <p className="empty-state">Выбери workspace, чтобы создать или открыть цели.</p>}

      {listProjection?.truncated ? <p className="goal-card__warning" role="status">Список целей ограничен размером IPC-проекции; открой цель отдельно для полного состояния.</p> : null}

      {goals.length > 0 ? (
        <div className="goal-list">
          {goals.map((goal) => (
            <article key={goal.goalId} className={`goal-card goal-card--${goal.status}`}>
              <div className="goal-card__heading">
                <div>
                  <strong>{goal.objective}</strong>
                  <small>{STATUS_LABELS[goal.status] ?? goal.status} · версия {goal.version}</small>
                </div>
                <code>{goal.completedCriteria.length}/{goal.successCriteria.length}</code>
              </div>
              <p className="goal-card__summary">{goal.progressSummary}</p>
              {goal.recoveryWarning ? <p className="goal-card__warning" role="alert">{goal.recoveryWarning}</p> : null}
              {goal.blockers.length > 0 ? <ul className="goal-card__blockers">{goal.blockers.map((item) => <li key={item}>{item}</li>)}</ul> : null}
              <ul className="goal-card__criteria">
                {goal.successCriteria.map((item) => (
                  <li key={item.id} className={item.status === 'verified' ? 'goal-criterion--verified' : ''}>
                    <span aria-hidden="true">{item.status === 'verified' ? '✓' : '○'}</span>
                    <span>{item.statement}</span>
                    {item.status !== 'verified' && goal.status !== 'completed' && goal.status !== 'cancelled' ? (
                      <button type="button" disabled={busy} onClick={() => void verify(goal, item.id)}>Подтвердить</button>
                    ) : null}
                  </li>
                ))}
              </ul>
              {goal.workflowRunIds.length > 0 || goal.childRunIds.length > 0 || goal.checkpointId || goal.successCriteria.some((item) => item.evidenceRef) ? (
                <div className="goal-card__links">
                  {goal.workflowRunIds.length > 0 ? <small>Workflow: {goal.workflowRunIds.join(', ')}</small> : null}
                  {goal.childRunIds.length > 0 ? <small>Дочерние runs: {goal.childRunIds.join(', ')}</small> : null}
                  {goal.checkpointId ? <small>Checkpoint: {goal.checkpointId}</small> : null}
                  {goal.successCriteria.filter((item) => item.evidenceRef).map((item) => <small key={`evidence-${item.id}`}>Evidence {item.id}: {item.evidenceRef}</small>)}
                </div>
              ) : null}
              {goal.nextAction ? <small className="goal-card__next">Следующее действие: {goal.nextAction}</small> : null}
              <div className="goal-card__actions">
                {goal.status === 'active' ? <button type="button" disabled={busy} onClick={() => void transition(goal, 'pauseGoal')}>Пауза</button> : null}
                {goal.status === 'paused' || goal.status === 'blocked' || goal.status === 'budget_limited' ? <button type="button" disabled={busy} onClick={() => void transition(goal, 'resumeGoal')}>Продолжить</button> : null}
                {!['completed', 'cancelled'].includes(goal.status) ? <button type="button" disabled={busy} onClick={() => void transition(goal, 'cancelGoal')}>Отменить</button> : null}
              </div>
              {goal.tokenBudget || goal.costBudgetMicros || goal.continuationBudget ? (
                <small className="goal-card__budget">Бюджет: tokens {goal.tokenBudget || '—'} · cost {goal.costBudgetMicros || '—'} · продолжений {goal.continuationBudget || '—'}</small>
              ) : null}
            </article>
          ))}
        </div>
      ) : <p className="empty-state">Сохранённых целей пока нет.</p>}

      {message ? <p className="goal-panel__message" role="status">{message}</p> : null}
    </section>
  )
}

function mergeGoal(current: readonly GoalProjection[], goal: GoalProjection): readonly GoalProjection[] {
  const found = current.some((item) => item.goalId === goal.goalId)
  return found ? current.map((item) => item.goalId === goal.goalId ? goal : item) : [goal, ...current]
}

function makeId(): string {
  return typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(16).slice(2)}`
}
