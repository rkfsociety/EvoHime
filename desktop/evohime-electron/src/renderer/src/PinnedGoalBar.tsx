import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent, GoalProjection } from '@shared/api'

import { translate } from './i18n'
import { useShellApi } from './shell-api'
import { UiIcon } from './UiIcon'

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
 * starting, pausing, resuming, verifying, or cancelling a goal in Core.
 * Renders nothing when there is no goal to display.
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
  const [expanded, setExpanded] = useState(false)
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
      || (event.eventType === 'tool.output'
        && (event.payload.includes('"goal_create"')
          || event.payload.includes('"goal_verify"')
          // Transcripts recorded before the rename still carry the dotted id.
          || event.payload.includes('"goal.create"')
          || event.payload.includes('"goal.verify"')))
    )
    return match?.sequenceId ?? 0
  }, [events])

  useEffect(() => {
    setGoals([])
    setNotice(null)
    setExpanded(false)
  }, [chatId, workspace])

  useEffect(() => {
    if (listProjection) {
      setGoals(listProjection.goals.filter((goal) => goal.chatId === chatId))
    }
  }, [chatId, listProjection])

  useEffect(() => {
    const goal = actionEvent?.goal
    if (goal?.chatId === chatId) setGoals((current) => mergeGoal(current, goal))
    if (actionEvent?.errorCode) {
      setNotice(actionEvent.errorMessage || actionEvent.errorCode)
    }
  }, [actionEvent, chatId])

  useEffect(() => {
    if (!api || !workspace || !chatId || !connected) return
    void api.invoke('core.listGoals', { workspacePath: workspace, chatId, limit: 32 })
  }, [api, chatId, connected, workspace, latestGoalNotificationSeq])

  const pinnedGoal = useMemo(() => {
    // IPC projections are untrusted: a malformed list must render as empty
    // instead of crashing the composer.
    const visible = goals.filter((goal): goal is GoalProjection =>
      goal != null && !dismissedGoalIds.includes(goal.goalId))
    const byChat = (goal: GoalProjection): boolean =>
      chatId !== null && goal.chatId === chatId
    const inFlight = (goal: GoalProjection): boolean =>
      goal.status === 'active' || goal.status === 'paused' || goal.status === 'blocked' || goal.status === 'budget_limited'
    return visible.find((goal) => inFlight(goal) && byChat(goal))
      ?? visible.find((goal) => goal.status === 'completed' && byChat(goal))
      ?? null
  }, [chatId, dismissedGoalIds, goals])

  if (!workspace || !chatId) return null

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

  if (!pinnedGoal) return null

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
            <UiIcon name={expanded ? 'chevron-up' : 'chevron-down'} size={13} />
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
                <span className={`pinned-goal-bar__criterion-badge pinned-goal-bar__criterion-badge--${item.status}`} aria-hidden="true">
                  <UiIcon name={item.status === 'verified' ? 'check-circle' : 'circle'} size={13} />
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
  const rest = current.filter((goal) => goal != null && goal.goalId !== updated.goalId)
  return [updated, ...rest]
}

function makeId(): string {
  return typeof crypto !== 'undefined' && 'randomUUID' in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(16).slice(2)}`
}
