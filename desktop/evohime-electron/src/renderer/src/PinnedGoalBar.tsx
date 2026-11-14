import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent, GoalProjection } from '@shared/api'

import { translate } from './i18n'
import { useShellApi } from './shell-api'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

const STATUS_LABELS: Record<string, string> = {
  active: 'В работе',
  paused: 'Пауза',
  blocked: 'Заблокирована',
  budget_limited: 'Лимит бюджета',
  completed: 'Выполнена',
  failed: 'Ошибка',
  cancelled: 'Отменена'
}

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly workspace: string | null
  readonly chatId: string | null
  readonly taskRunning: boolean
  readonly disabled: boolean
  readonly onRunGoal: (goalId: string, objective: string) => Promise<void>
}

/**
 * Compact Goal bar pinned directly above the composer in TaskTimeline.
 * Displays the active or paused goal for the current workspace/chat and allows
 * creating, starting, pausing, resuming, or cancelling a goal in Core.
 */
export function PinnedGoalBar({
  connection,
  events,
  workspace,
  chatId,
  taskRunning,
  disabled,
  onRunGoal
}: Props): React.JSX.Element | null {
  const api = useShellApi()
  const [goals, setGoals] = useState<readonly GoalProjection[]>([])
  const [composerOpen, setComposerOpen] = useState(false)
  const [expanded, setExpanded] = useState(false)
  const [objective, setObjective] = useState('')
  const [criterion, setCriterion] = useState('')
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  const [dismissedGoalIds, setDismissedGoalIds] = useState<readonly string[]>([])

  const connected = CONNECTED_STATES.includes(connection)

  const listProjection = useMemo(
    () => events.find((event) => event.goalList)?.goalList ?? null,
    [events]
  )
  const actionEvent = useMemo(
    () => events.find((event) => event.goalAction)?.goalAction ?? null,
    [events]
  )
  const latestGoalNotificationSeq = useMemo(() => {
    const match = events.find((event) =>
      event.eventType === 'goal.created'
      || event.eventType === 'goal.updated'
      || event.eventType === 'goal.criterion_verified'
      || event.eventType === 'goal.completed'
      || event.eventType === 'goal.paused'
      || event.eventType === 'goal.resumed'
      || event.eventType === 'goal.blocked'
      || event.eventType === 'goal.budget_limited'
      || event.eventType === 'goal.cancelled'
      || event.eventType === 'task.completed'
      || event.eventType === 'task.failed'
    )
    return match?.sequenceId ?? 0
  }, [events])

  useEffect(() => {
    setGoals([])
    setNotice(null)
    setComposerOpen(false)
    setExpanded(false)
  }, [workspace])

  useEffect(() => {
    if (listProjection) {
      setGoals(listProjection.goals)
    }
  }, [listProjection])

  useEffect(() => {
    const goal = actionEvent?.goal
    if (goal) setGoals((current) => mergeGoal(current, goal))
    if (actionEvent?.errorCode) {
      setNotice(actionEvent.errorMessage || actionEvent.errorCode)
    }
  }, [actionEvent])

  useEffect(() => {
    if (!api || !workspace || !connected) return
    void api.invoke('core.listGoals', { workspacePath: workspace, limit: 32 })
  }, [api, connected, workspace, latestGoalNotificationSeq])

  const pinnedGoal = useMemo(() => {
    const visible = goals.filter((goal) => !dismissedGoalIds.includes(goal.goalId))
    const byChat = (goal: GoalProjection): boolean =>
      chatId !== null && goal.chatId === chatId
    const inFlight = (goal: GoalProjection): boolean =>
      goal.status === 'active' || goal.status === 'paused' || goal.status === 'blocked' || goal.status === 'budget_limited'
    return visible.find((goal) => inFlight(goal) && byChat(goal))
      ?? visible.find((goal) => inFlight(goal))
      ?? visible.find((goal) => goal.status === 'completed' && byChat(goal))
      ?? null
  }, [chatId, dismissedGoalIds, goals])

  if (!workspace) return null

  const createGoal = async (runImmediately: boolean): Promise<void> => {
    if (!api || !workspace || !connected) return
    const trimmedObjective = objective.trim()
    if (trimmedObjective.length === 0) return
    const goalId = `goal-${makeId()}`
    const criterionText = criterion.trim() || 'Цель полностью выполнена и проверена.'
    setBusy(true)
    setNotice(null)
    const outcome = await api.invoke('core.createGoal', {
      goalId,
      workspacePath: workspace,
      ...(chatId ? { chatId } : {}),
      objective: trimmedObjective,
      successCriteria: [{ id: 'criterion-1', kind: 'manual', statement: criterionText }],
      continuationBudget: 12,
      idempotencyKey: `goal:create:${goalId}`
    })
    setBusy(false)
    if (!outcome.ok) {
      setNotice(outcome.message)
      return
    }
    setObjective('')
    setCriterion('')
    setComposerOpen(false)
    if (runImmediately) {
      await onRunGoal(goalId, trimmedObjective)
    }
  }

  const transition = async (
    goal: GoalProjection,
    action: 'pauseGoal' | 'resumeGoal' | 'cancelGoal',
    runAfterResume = false
  ): Promise<void> => {
    if (!api || !connected) return
    setBusy(true)
    setNotice(null)
    const outcome = await api.invoke(`core.${action}`, {
      goalId: goal.goalId,
      expectedVersion: goal.version,
      idempotencyKey: `goal:${action}:${goal.goalId}:${makeId()}`
    })
    setBusy(false)
    if (!outcome.ok) {
      setNotice(outcome.message)
      return
    }
    if (action === 'resumeGoal' && runAfterResume) {
      await onRunGoal(goal.goalId, goal.objective)
    }
  }

  const verifyCriterion = async (goal: GoalProjection, criterionId: string): Promise<void> => {
    if (!api || !connected) return
    setBusy(true)
    setNotice(null)
    const outcome = await api.invoke('core.verifyGoalCriterion', {
      goalId: goal.goalId,
      expectedVersion: goal.version,
      criterionId,
      idempotencyKey: `goal:verify:${goal.goalId}:${criterionId}:${makeId()}`
    })
    setBusy(false)
    if (!outcome.ok) setNotice(outcome.message)
  }

  if (!pinnedGoal) {
    return (
      <div className="pinned-goal-bar pinned-goal-bar--empty" data-testid="pinned-goal-bar">
        {!composerOpen ? (
          <div className="pinned-goal-bar__trigger-row">
            <button
              type="button"
              className="pinned-goal-bar__trigger"
              disabled={!connected || disabled || busy}
              onClick={() => setComposerOpen(true)}
            >
              <span>+ {translate('Поставить цель')}</span>
            </button>
            <span className="pinned-goal-bar__hint">
              {translate('Цель фиксируется над композером и выполняется до конца')}
            </span>
          </div>
        ) : (
          <form
            className="pinned-goal-bar__form"
            onSubmit={(event) => {
              event.preventDefault()
              void createGoal(true)
            }}
          >
            <div className="pinned-goal-bar__form-fields">
              <input
                type="text"
                className="pinned-goal-bar__input"
                value={objective}
                onChange={(event) => setObjective(event.target.value)}
                placeholder={translate('Сформулируй цель (например: починить все тесты модуля и проверить сборку)…')}
                disabled={busy}
                autoFocus
              />
              <input
                type="text"
                className="pinned-goal-bar__input pinned-goal-bar__input--criterion"
                value={criterion}
                onChange={(event) => setCriterion(event.target.value)}
                placeholder={translate('Критерий готовности (необязательно)')}
                disabled={busy}
              />
            </div>
            <div className="pinned-goal-bar__form-actions">
              <button
                type="submit"
                className="pinned-goal-bar__btn pinned-goal-bar__btn--primary"
                disabled={!connected || busy || disabled || objective.trim().length === 0}
              >
                {translate('Поставить и выполнить')}
              </button>
              <button
                type="button"
                className="pinned-goal-bar__btn"
                disabled={!connected || busy || objective.trim().length === 0}
                onClick={() => void createGoal(false)}
              >
                {translate('Только закрепить')}
              </button>
              <button
                type="button"
                className="pinned-goal-bar__btn"
                disabled={busy}
                onClick={() => setComposerOpen(false)}
              >
                {translate('Отмена')}
              </button>
            </div>
            {notice ? <p className="pinned-goal-bar__notice" role="alert">{translate(notice)}</p> : null}
          </form>
        )}
      </div>
    )
  }

  const totalCriteria = pinnedGoal.successCriteria.length
  const completedCount = pinnedGoal.completedCriteria.length
  const isResumable =
    pinnedGoal.status === 'paused'
    || pinnedGoal.status === 'blocked'
    || pinnedGoal.status === 'budget_limited'
  const isCompleted = pinnedGoal.status === 'completed'

  return (
    <section
      className={`pinned-goal-bar pinned-goal-bar--${pinnedGoal.status}`}
      aria-label={translate('Активная цель')}
      data-testid="pinned-goal-bar"
    >
      <div className="pinned-goal-bar__header">
        <div className="pinned-goal-bar__title-wrap">
          <span className={`pinned-goal-bar__status pinned-goal-bar__status--${pinnedGoal.status}`}>
            {translate(STATUS_LABELS[pinnedGoal.status] ?? pinnedGoal.status)}
          </span>
          <strong className="pinned-goal-bar__objective" title={pinnedGoal.objective}>
            {pinnedGoal.objective}
          </strong>
          <span className="pinned-goal-bar__counter">
            {completedCount}/{totalCriteria}
          </span>
        </div>

        <div className="pinned-goal-bar__actions">
          {pinnedGoal.status === 'active' && !taskRunning ? (
            <button
              type="button"
              className="pinned-goal-bar__btn pinned-goal-bar__btn--primary"
              disabled={!connected || busy || disabled}
              onClick={() => void onRunGoal(pinnedGoal.goalId, pinnedGoal.objective)}
            >
              {translate('Выполнить')}
            </button>
          ) : null}
          {pinnedGoal.status === 'active' ? (
            <button
              type="button"
              className="pinned-goal-bar__btn"
              disabled={!connected || busy}
              onClick={() => void transition(pinnedGoal, 'pauseGoal')}
            >
              {translate('Пауза')}
            </button>
          ) : null}
          {isResumable ? (
            <button
              type="button"
              className="pinned-goal-bar__btn pinned-goal-bar__btn--primary"
              disabled={!connected || busy || disabled}
              onClick={() => void transition(pinnedGoal, 'resumeGoal', true)}
            >
              {translate('Продолжить')}
            </button>
          ) : null}
          {!isCompleted && pinnedGoal.status !== 'cancelled' ? (
            <button
              type="button"
              className="pinned-goal-bar__btn"
              disabled={!connected || busy}
              onClick={() => void transition(pinnedGoal, 'cancelGoal')}
            >
              {translate('Отменить')}
            </button>
          ) : null}
          {isCompleted ? (
            <button
              type="button"
              className="pinned-goal-bar__btn"
              onClick={() => setDismissedGoalIds((current) => [...current, pinnedGoal.goalId])}
              title={translate('Скрыть выполненную цель')}
            >
              {translate('Скрыть')}
            </button>
          ) : null}
          <button
            type="button"
            className="pinned-goal-bar__btn pinned-goal-bar__btn--icon"
            aria-expanded={expanded}
            aria-label={translate('Детали цели')}
            onClick={() => setExpanded((value) => !value)}
          >
            {expanded ? '▴' : '▾'}
          </button>
        </div>
      </div>

      {expanded ? (
        <div className="pinned-goal-bar__details">
          {pinnedGoal.progressSummary ? (
            <p className="pinned-goal-bar__summary">{pinnedGoal.progressSummary}</p>
          ) : null}
          {pinnedGoal.nextAction ? (
            <p className="pinned-goal-bar__next">
              {translate('Следующий шаг:')} {pinnedGoal.nextAction}
            </p>
          ) : null}
          <ul className="pinned-goal-bar__criteria">
            {pinnedGoal.successCriteria.map((item) => (
              <li key={item.id} className="pinned-goal-bar__criterion">
                <span className={`pinned-goal-bar__criterion-badge pinned-goal-bar__criterion-badge--${item.status}`}>
                  {item.status === 'verified' ? '✓' : '○'}
                </span>
                <span>{item.statement}</span>
                {item.status !== 'verified' && !isCompleted ? (
                  <button
                    type="button"
                    className="pinned-goal-bar__btn pinned-goal-bar__btn--small"
                    disabled={!connected || busy}
                    onClick={() => void verifyCriterion(pinnedGoal, item.id)}
                  >
                    {translate('Подтвердить')}
                  </button>
                ) : null}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {notice ? <p className="pinned-goal-bar__notice" role="alert">{translate(notice)}</p> : null}
    </section>
  )
}

function mergeGoal(current: readonly GoalProjection[], updated: GoalProjection): readonly GoalProjection[] {
  const rest = current.filter((goal) => goal.goalId !== updated.goalId)
  return [updated, ...rest]
}

function makeId(): string {
  return typeof crypto !== 'undefined' && 'randomUUID' in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(16).slice(2)}`
}
