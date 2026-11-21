import { useId, useMemo, useState } from 'react'

import { translate } from '../i18n'

/** Минимальное представление активного агента из `HiveStateSnapshot` для графа улья. */
export interface HiveGraphAgent {
  readonly id: string
  readonly role: string
  readonly currentAction: string
  readonly depth: number
  readonly spawnedSubagents: number
  readonly ceiling: number
}

/** Параметры интерактивного графа связей «Королева Ева и воркеры». */
export interface HiveGraphProps {
  readonly queenName: string
  readonly queenActive: boolean
  readonly activeAgents: readonly HiveGraphAgent[]
}

interface PositionedAgent {
  readonly agent: HiveGraphAgent
  readonly x: number
  readonly y: number
  readonly running: boolean
  readonly monogram: string
  readonly shortRole: string
}

const VIEWBOX_WIDTH = 380
const VIEWBOX_HEIGHT = 300
const CENTER_X = 190
const CENTER_Y = 132
const IDLE_ACTIONS = new Set(['', 'idle', 'waiting', 'sleeping', 'paused'])

/** Определяет, выполняет ли агент активную задачу (`running`) или простаивает (`idle`). */
export function isHiveAgentRunning(currentAction: string): boolean {
  return !IDLE_ACTIONS.has(currentAction.trim().toLowerCase())
}

function agentShortMonogram(role: string, id: string): string {
  const source = (role || id || 'AG').replace(/[^a-zA-Zа-яА-Я0-9]/g, '')
  return (source.slice(0, 2) || 'AG').toUpperCase()
}

function truncateLabel(value: string, maxLength: number): string {
  const trimmed = value.trim()
  if (trimmed.length <= maxLength) return trimmed
  return `${trimmed.slice(0, maxLength - 1)}…`
}

function computeAgentPositions(agents: readonly HiveGraphAgent[]): PositionedAgent[] {
  const count = agents.length
  if (count === 0) return []

  const useDualOrbit = count > 6
  const innerCount = useDualOrbit ? Math.ceil(count / 2) : count
  const outerCount = count - innerCount

  return agents.map((agent, index) => {
    const isOuter = useDualOrbit && index >= innerCount
    const ringIndex = isOuter ? index - innerCount : index
    const ringTotal = isOuter ? outerCount : innerCount
    const radius = useDualOrbit ? (isOuter ? 106 : 70) : count <= 2 ? 84 : 92
    const angleOffset = isOuter ? Math.PI / Math.max(ringTotal, 1) : 0
    const angle = -Math.PI / 2 + angleOffset + (2 * Math.PI * ringIndex) / Math.max(ringTotal, 1)

    return {
      agent,
      x: Math.round((CENTER_X + Math.cos(angle) * radius) * 10) / 10,
      y: Math.round((CENTER_Y + Math.sin(angle) * radius) * 10) / 10,
      running: isHiveAgentRunning(agent.currentAction),
      monogram: agentShortMonogram(agent.role, agent.id),
      shortRole: truncateLabel(agent.role || 'worker', 12)
    }
  })
}

/**
 * Интерактивный SVG-граф связей улья: Королева Ева в центре и активные агенты на орбите.
 * Показывает пульсирующие линии делегирования для выполняющихся воркеров, статичные связи для `idle`
 * и спокойную анимацию спящей Королевы при отсутствии активных агентов.
 */
export function HiveGraph({ queenName, queenActive, activeAgents }: HiveGraphProps): React.JSX.Element {
  const uid = useId().replace(/:/g, '')
  const [hoveredId, setHoveredId] = useState<string | null>(null)
  const [selectedId, setSelectedId] = useState<string | null>(null)

  const positioned = useMemo(() => computeAgentPositions(activeAgents), [activeAgents])
  const isSleeping = !queenActive || activeAgents.length === 0
  const runningCount = positioned.filter((item) => item.running).length
  const nodeRadius = positioned.length > 8 ? 16 : 19
  const queenHaloRadius = isSleeping ? 105 : 64
  const queenPulseRadius = isSleeping ? 72 : 42
  const queenCoreRadius = isSleeping ? 58 : 34
  const crownScale = isSleeping ? 2.5 : 1.45

  const activeDetailId = hoveredId ?? selectedId
  const inspectedAgent = activeDetailId
    ? positioned.find((item) => item.agent.id === activeDetailId)?.agent ?? null
    : null

  const queenGlowId = `hive-queen-glow-${uid}`
  const runningGlowId = `hive-running-glow-${uid}`
  const crownGradId = `hive-crown-grad-${uid}`

  const ariaLabel = isSleeping
    ? `${translate('Королева Ева')}: ${translate('спящий режим')}`
    : `${translate('Королева Ева')}: ${activeAgents.length} ${translate('активных агентов')}, ${runningCount} ${translate('в работе')}`

  return (
    <div
      className={`hive-graph${isSleeping ? ' hive-graph--sleeping' : ' hive-graph--active'}`}
      data-testid="hive-graph"
      data-queen-state={isSleeping ? 'sleeping' : 'active'}
    >
      <svg
        className="hive-graph__svg"
        viewBox={`0 0 ${VIEWBOX_WIDTH} ${VIEWBOX_HEIGHT}`}
        preserveAspectRatio="xMidYMid meet"
        role="img"
        aria-label={ariaLabel}
      >
        <defs>
          <radialGradient id={queenGlowId} cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor={isSleeping ? 'var(--hive-amber)' : 'var(--hive-violet)'} stopOpacity={isSleeping ? '0.45' : '0.72'} />
            <stop offset="55%" stopColor={isSleeping ? 'var(--hive-violet)' : 'var(--hive-blue)'} stopOpacity="0.22" />
            <stop offset="100%" stopColor="transparent" stopOpacity="0" />
          </radialGradient>
          <radialGradient id={runningGlowId} cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="var(--hive-green)" stopOpacity="0.65" />
            <stop offset="100%" stopColor="transparent" stopOpacity="0" />
          </radialGradient>
          <linearGradient id={crownGradId} x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor={isSleeping ? '#f2cc6c' : '#e0d6ff'} />
            <stop offset="100%" stopColor={isSleeping ? '#c79232' : '#7cb8ff'} />
          </linearGradient>
        </defs>

        {/* Фоновая орбитальная сетка */}
        <circle className="hive-graph__ring hive-graph__ring--outer" cx={CENTER_X} cy={CENTER_Y} r={112} />
        <circle className="hive-graph__ring hive-graph__ring--inner" cx={CENTER_X} cy={CENTER_Y} r={74} />

        {/* Орбита воркеров и линий делегирования */}
        <g
          className={`hive-graph__orbit${isSleeping ? ' hive-graph__orbit--paused' : ''}`}
          style={{ transformOrigin: `${CENTER_X}px ${CENTER_Y}px` }}
        >
          {positioned.map(({ agent, x, y, running }) => {
            const highlighted = activeDetailId === agent.id
            return (
              <line
                key={`edge-${agent.id}`}
                x1={CENTER_X}
                y1={CENTER_Y}
                x2={x}
                y2={y}
                className={[
                  'hive-graph__edge',
                  running ? 'hive-graph__edge--running' : 'hive-graph__edge--idle',
                  highlighted ? 'hive-graph__edge--highlighted' : ''
                ]
                  .filter(Boolean)
                  .join(' ')}
                data-testid="hive-graph-edge"
                data-edge-state={running ? 'running' : 'idle'}
              />
            )
          })}

          {positioned.map(({ agent, x, y, running, monogram, shortRole }) => {
            const highlighted = activeDetailId === agent.id
            return (
              <g
                key={agent.id}
                className={[
                  'hive-graph__node',
                  running ? 'hive-graph__node--running' : 'hive-graph__node--idle',
                  highlighted ? 'hive-graph__node--highlighted' : ''
                ]
                  .filter(Boolean)
                  .join(' ')}
                style={{ transformOrigin: `${x}px ${y}px` }}
                data-testid="hive-graph-agent"
                data-agent-id={agent.id}
                data-agent-state={running ? 'running' : 'idle'}
                tabIndex={0}
                role="button"
                aria-label={`${agent.id} (${agent.role}): ${agent.currentAction || 'idle'}`}
                onMouseEnter={() => setHoveredId(agent.id)}
                onMouseLeave={() => setHoveredId((prev) => (prev === agent.id ? null : prev))}
                onFocus={() => setHoveredId(agent.id)}
                onBlur={() => setHoveredId((prev) => (prev === agent.id ? null : prev))}
                onClick={() => setSelectedId((prev) => (prev === agent.id ? null : agent.id))}
              >
                <title>{`${agent.id} · ${agent.role} (${agent.currentAction || 'idle'})`}</title>
                {running ? <circle className="hive-graph__node-aura" cx={x} cy={y} r={nodeRadius + 9} fill={`url(#${runningGlowId})`} /> : null}
                <circle className="hive-graph__node-core" cx={x} cy={y} r={nodeRadius} />
                <g className="hive-graph__node-upright" style={{ transformOrigin: `${x}px ${y}px` }}>
                  <text className="hive-graph__node-code" x={x} y={y + 3.5} textAnchor="middle">
                    {monogram}
                  </text>
                  <text className="hive-graph__node-role" x={x} y={y + nodeRadius + 12} textAnchor="middle">
                    {shortRole}
                  </text>
                </g>
              </g>
            )
          })}
        </g>

        {/* Центральный узел: Королева Ева */}
        <g
          className={`hive-graph__queen${isSleeping ? ' hive-graph__queen--sleeping' : ' hive-graph__queen--active'}`}
          style={{ transformOrigin: `${CENTER_X}px ${CENTER_Y}px` }}
          data-testid="hive-graph-queen"
          data-queen-state={isSleeping ? 'sleeping' : 'active'}
        >
          <title>{`${translate('Королева Ева')} (${queenName || 'EvoHime'})`}</title>
          <circle className="hive-graph__queen-halo" cx={CENTER_X} cy={CENTER_Y} r={queenHaloRadius} fill={`url(#${queenGlowId})`} />
          <circle className="hive-graph__queen-pulse" cx={CENTER_X} cy={CENTER_Y} r={queenPulseRadius} />
          <circle className="hive-graph__queen-core" cx={CENTER_X} cy={CENTER_Y} r={queenCoreRadius} />
          {/* Иконка короны */}
          <g transform={`translate(${CENTER_X}, ${CENTER_Y}) scale(${crownScale})`}>
            <path
              className="hive-graph__crown"
              fill={`url(#${crownGradId})`}
              d="M -11 6 L -13 -5 L -6 0 L 0 -9 L 6 0 L 13 -5 L 11 6 Z"
            />
            <rect
              className="hive-graph__crown-base"
              x={-11}
              y={7.5}
              width={22}
              height={3}
              rx={1.5}
              fill={`url(#${crownGradId})`}
            />
          </g>
        </g>
      </svg>

      {inspectedAgent ? (
        <div className="hive-graph__tooltip" role="status">
          <strong>{inspectedAgent.id}</strong>
          <span>{inspectedAgent.currentAction || translate('ожидание')}</span>
          <small>
            {translate('Глубина')}: {inspectedAgent.depth} · {inspectedAgent.spawnedSubagents}/{inspectedAgent.ceiling}
          </small>
        </div>
      ) : null}
    </div>
  )
}
