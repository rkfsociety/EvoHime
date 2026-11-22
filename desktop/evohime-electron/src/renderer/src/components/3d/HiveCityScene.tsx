import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent, type ReactNode } from 'react'

import hiveCityPanoramaUrl from '../../assets/hive-city-panorama.jpg'
import { UiIcon, type UiIconName } from '../../UiIcon'

export interface HiveCityDepartmentPin {
  readonly key: string
  readonly label: string
  readonly subtitle: string
  readonly icon: UiIconName
  readonly activeCount: number
  readonly maxCapacity: number
}

export interface HiveCitySceneProps {
  /** Статус цикла наблюдения Улья (`observing`, `active`, `waiting_for_limits`, `waiting_for_tasks`, `disabled`, `sleeping`). */
  readonly status: string
  /** Департаменты Улья для отображения парящих пинов и живых магистралей. */
  readonly departments?: readonly HiveCityDepartmentPin[] | undefined
  /** Ключ выбранного департамента (подсвечивает пин и магистраль). */
  readonly selectedDepartment?: string | null | undefined
  /** Обработчик клика по пину департамента. */
  readonly onSelectDepartment?: ((departmentKey: string) => void) | undefined
  /** Флаг активной работы Королевы Евы (усиливает центральное сияние). */
  readonly queenWorking?: boolean | undefined
  /** Центральная парящая карточка Королевы Евы. */
  readonly queenOverlay?: ReactNode | undefined
  /** Подпись неактивного департамента. */
  readonly idleDepartmentsLabel?: string | undefined
  /** Подпись активного департамента. */
  readonly activeDepartmentsLabel?: string | undefined
  /** Пользовательский рендерер иконок департаментов. */
  readonly departmentIconRenderer?: ((icon: UiIconName) => ReactNode) | undefined
}

interface DistrictAnchor {
  readonly x: number
  readonly y: number
  readonly accent: string
}

const QUEEN_ORBIT_CENTER = { x: 800, y: 245 }
const CONDUIT_ORIGIN = { x: 800, y: 405 }

/**
 * Координаты 8 кварталов в процентах 16:9 полотна города и акцентные цвета их магистралей.
 * Точки расположены поверх кварталов и не пересекаются с верхними и нижним HUD-панелями.
 */
const DEPARTMENT_COORDS: Record<string, DistrictAnchor> = {
  analytics: { x: 21, y: 25, accent: '#5ea8ff' },
  planning: { x: 32, y: 36, accent: '#a78bfa' },
  research: { x: 63, y: 23, accent: '#818cf8' },
  development: { x: 74, y: 37, accent: '#38bdf8' },
  testing: { x: 82, y: 50, accent: '#4ec9a4' },
  integrations: { x: 70, y: 63, accent: '#60a5fa' },
  documentation: { x: 30, y: 63, accent: '#34d399' },
  support: { x: 18, y: 49, accent: '#c084fc' }
}

const FALLBACK_COORDS: readonly DistrictAnchor[] = [
  { x: 40, y: 66, accent: '#8d82e8' },
  { x: 60, y: 66, accent: '#5ea8ff' },
  { x: 24, y: 39, accent: '#4ec9a4' },
  { x: 76, y: 43, accent: '#e0b341' }
]

interface ParticleSeed {
  readonly left: number
  readonly bottom: number
  readonly size: number
  readonly duration: number
  readonly delay: number
  readonly tone: 'violet' | 'blue' | 'amber' | 'green'
}

const AMBIENT_PARTICLES: readonly ParticleSeed[] = [
  { left: 19, bottom: 26, size: 3, duration: 7.4, delay: 0.2, tone: 'blue' },
  { left: 28, bottom: 38, size: 4, duration: 8.8, delay: 1.4, tone: 'violet' },
  { left: 36, bottom: 29, size: 3, duration: 6.9, delay: 2.3, tone: 'green' },
  { left: 45, bottom: 44, size: 4, duration: 7.6, delay: 0.7, tone: 'amber' },
  { left: 50, bottom: 52, size: 5, duration: 6.4, delay: 1.9, tone: 'violet' },
  { left: 55, bottom: 42, size: 3, duration: 8.1, delay: 3.1, tone: 'amber' },
  { left: 63, bottom: 30, size: 4, duration: 7.2, delay: 0.9, tone: 'blue' },
  { left: 71, bottom: 40, size: 3, duration: 8.6, delay: 2.6, tone: 'violet' },
  { left: 79, bottom: 33, size: 4, duration: 7.0, delay: 1.1, tone: 'green' },
  { left: 41, bottom: 61, size: 3, duration: 9.2, delay: 3.7, tone: 'blue' },
  { left: 59, bottom: 60, size: 3, duration: 8.4, delay: 2.0, tone: 'violet' },
  { left: 24, bottom: 54, size: 3, duration: 7.9, delay: 4.1, tone: 'amber' }
]

interface CoreVisualPalette {
  readonly primary: string
  readonly secondary: string
  readonly glowOpacity: number
}

function resolveCorePalette(status: string, working: boolean): CoreVisualPalette {
  if (working || status === 'active') {
    return { primary: '#e0b341', secondary: '#8d82e8', glowOpacity: 0.92 }
  }
  if (status === 'observing') {
    return { primary: '#8d82e8', secondary: '#4ec9a4', glowOpacity: 0.78 }
  }
  if (status === 'waiting_for_limits') {
    return { primary: '#e0b341', secondary: '#ed6a7a', glowOpacity: 0.72 }
  }
  return { primary: '#5ea8ff', secondary: '#8d82e8', glowOpacity: 0.56 }
}

function buildConduitPath(targetX: number, targetY: number): string {
  const midX = (CONDUIT_ORIGIN.x + targetX) * 0.5
  const arcOffset = targetY <= CONDUIT_ORIGIN.y ? -34 : 34
  const midY = (CONDUIT_ORIGIN.y + targetY) * 0.5 + arcOffset
  return `M ${CONDUIT_ORIGIN.x} ${CONDUIT_ORIGIN.y} Q ${midX.toFixed(1)} ${midY.toFixed(1)} ${targetX.toFixed(1)} ${targetY.toFixed(1)}`
}

function useReducedMotionPreference(): boolean {
  const [reduced, setReduced] = useState<boolean>(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false
    return window.matchMedia('(prefers-reduced-motion: reduce)').matches
  })

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return undefined
    const media = window.matchMedia('(prefers-reduced-motion: reduce)')
    const update = (): void => setReduced(media.matches)
    update()
    if (typeof media.addEventListener === 'function') {
      media.addEventListener('change', update)
      return () => media.removeEventListener('change', update)
    }
    return undefined
  }, [])

  return reduced
}

function useDocumentHidden(): boolean {
  const [hidden, setHidden] = useState<boolean>(() => {
    if (typeof document === 'undefined') return false
    return document.visibilityState === 'hidden'
  })

  useEffect(() => {
    if (typeof document === 'undefined') return undefined
    const onVisibilityChange = (): void => {
      setHidden(document.visibilityState === 'hidden')
    }
    document.addEventListener('visibilitychange', onVisibilityChange)
    return () => document.removeEventListener('visibilitychange', onVisibilityChange)
  }, [])

  return hidden
}

/**
 * Полноэкранная 2.5D-сцена города Улья:
 * - чистый 16:9 арт-город с мягкой HUD-виньеткой;
 * - живой SVG-слой неоновых магистралей, орбит Королевы Евы и парящих частиц;
 * - интерактивные стеклянные пины департаментов и центральная карточка Королевы,
 *   зафиксированные в единой 16:9 координатной системе.
 */
export function HiveCityScene({
  status,
  departments = [],
  selectedDepartment = null,
  onSelectDepartment,
  queenWorking = false,
  queenOverlay,
  idleDepartmentsLabel = 'Ожидание',
  activeDepartmentsLabel = 'Работает',
  departmentIconRenderer
}: HiveCitySceneProps): React.JSX.Element {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const rafRef = useRef<number | null>(null)
  const [hoveredDepartment, setHoveredDepartment] = useState<string | null>(null)

  const reducedMotion = useReducedMotionPreference()
  const documentHidden = useDocumentHidden()
  const animationsPaused = reducedMotion || documentHidden

  const normalizedStatus = status.trim().toLowerCase() || 'idle'
  const corePalette = useMemo(
    () => resolveCorePalette(normalizedStatus, queenWorking),
    [normalizedStatus, queenWorking]
  )

  const positionedDepartments = useMemo(() => {
    let fallbackIndex = 0
    return departments.map((department) => {
      const mapped = DEPARTMENT_COORDS[department.key]
      if (mapped) {
        return { department, coord: mapped }
      }
      const fallback = FALLBACK_COORDS[fallbackIndex % FALLBACK_COORDS.length] ?? { x: 50, y: 66, accent: '#8d82e8' }
      fallbackIndex += 1
      return { department, coord: fallback }
    })
  }, [departments])

  const hasInteractiveOverlay = departments.length > 0 || Boolean(queenOverlay)

  useEffect(() => {
    return () => {
      if (rafRef.current !== null && typeof cancelAnimationFrame === 'function') {
        cancelAnimationFrame(rafRef.current)
      }
    }
  }, [])

  const updateParallax = useCallback((px: number, py: number): void => {
    const node = containerRef.current
    if (!node) return
    node.style.setProperty('--px', px.toFixed(3))
    node.style.setProperty('--py', py.toFixed(3))
  }, [])

  const handleMouseMove = useCallback((event: MouseEvent<HTMLDivElement>): void => {
    if (animationsPaused || !containerRef.current) return
    const rect = containerRef.current.getBoundingClientRect()
    if (rect.width <= 0 || rect.height <= 0) return
    const normalizedX = Math.max(-1, Math.min(1, ((event.clientX - rect.left) / rect.width - 0.5) * 2))
    const normalizedY = Math.max(-1, Math.min(1, ((event.clientY - rect.top) / rect.height - 0.5) * 2))
    if (typeof requestAnimationFrame === 'function') {
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current)
      rafRef.current = requestAnimationFrame(() => {
        rafRef.current = null
        updateParallax(normalizedX, normalizedY)
      })
    } else {
      updateParallax(normalizedX, normalizedY)
    }
  }, [animationsPaused, updateParallax])

  const handleMouseLeave = useCallback((): void => {
    if (rafRef.current !== null && typeof cancelAnimationFrame === 'function') {
      cancelAnimationFrame(rafRef.current)
      rafRef.current = null
    }
    updateParallax(0, 0)
  }, [updateParallax])

  return (
    <div
      ref={containerRef}
      className="hive-city-scene"
      data-testid="hive-city-scene"
      data-status={normalizedStatus}
      data-paused={animationsPaused ? 'true' : 'false'}
      aria-hidden={hasInteractiveOverlay ? undefined : true}
      onMouseMove={handleMouseMove}
      onMouseLeave={handleMouseLeave}
    >
      <div className="hive-city-scene__world">
        <div className="hive-city-scene__backdrop" aria-hidden="true">
          <img
            className="hive-city-scene__art"
            src={hiveCityPanoramaUrl}
            alt=""
            draggable={false}
            data-testid="hive-city-scene-art"
          />
          <div className="hive-city-scene__vignette" />
        </div>

        <svg
          className="hive-city-scene__fx"
          viewBox="0 0 1600 900"
          preserveAspectRatio="xMidYMid slice"
          data-testid="hive-city-scene-fx"
          aria-hidden="true"
        >
          <defs>
            <radialGradient id="hive-queen-core-glow" cx="50%" cy="50%" r="50%">
              <stop offset="0%" stopColor={corePalette.primary} stopOpacity="0.46" />
              <stop offset="52%" stopColor={corePalette.secondary} stopOpacity="0.18" />
              <stop offset="100%" stopColor="#070812" stopOpacity="0" />
            </radialGradient>
            <filter id="hive-conduit-glow" x="-20%" y="-20%" width="140%" height="140%">
              <feGaussianBlur stdDeviation="4" result="blur" />
              <feMerge>
                <feMergeNode in="blur" />
                <feMergeNode in="SourceGraphic" />
              </feMerge>
            </filter>
          </defs>

          {/* Орбитальное дыхание и аура Королевы Евы */}
          <g className="hive-city-scene__queen-fx" opacity={corePalette.glowOpacity}>
            <circle
              cx={QUEEN_ORBIT_CENTER.x}
              cy={QUEEN_ORBIT_CENTER.y}
              r="185"
              fill="url(#hive-queen-core-glow)"
              className="hive-city-scene__queen-halo"
            />
            <ellipse
              cx={QUEEN_ORBIT_CENTER.x}
              cy={QUEEN_ORBIT_CENTER.y + 18}
              rx="144"
              ry="42"
              fill="none"
              stroke={corePalette.primary}
              strokeOpacity="0.55"
              strokeWidth="1.4"
              strokeDasharray="10 14"
              className="hive-city-scene__orbit hive-city-scene__orbit--outer"
            />
            <ellipse
              cx={QUEEN_ORBIT_CENTER.x}
              cy={QUEEN_ORBIT_CENTER.y + 20}
              rx="108"
              ry="31"
              fill="none"
              stroke={corePalette.secondary}
              strokeOpacity="0.48"
              strokeWidth="1.2"
              strokeDasharray="5 9"
              className="hive-city-scene__orbit hive-city-scene__orbit--inner"
            />
          </g>

          {/* Магистрали энергии от шпиля Королевы к кварталам */}
          {positionedDepartments.map(({ department, coord }) => {
            const tx = (coord.x / 100) * 1600
            const ty = (coord.y / 100) * 900
            const pathData = buildConduitPath(tx, ty)
            const isActive = department.activeCount > 0
            const isHighlighted = isActive || selectedDepartment === department.key || hoveredDepartment === department.key
            return (
              <g key={department.key} className="hive-city-conduit-group">
                <path
                  d={pathData}
                  fill="none"
                  stroke={coord.accent}
                  strokeOpacity={isHighlighted ? 0.34 : 0.14}
                  strokeWidth={isHighlighted ? 3.5 : 1.6}
                  className="hive-city-conduit__base"
                />
                <path
                  d={pathData}
                  fill="none"
                  stroke={coord.accent}
                  strokeWidth={isActive ? 2.6 : 1.8}
                  strokeLinecap="round"
                  filter={isHighlighted ? 'url(#hive-conduit-glow)' : undefined}
                  className={`hive-city-conduit${isActive ? ' hive-city-conduit--active' : ''}${isHighlighted ? ' hive-city-conduit--highlighted' : ''}`}
                  data-testid="hive-city-conduit"
                  data-dept={department.key}
                  data-active={isActive ? 'true' : 'false'}
                />
                <circle
                  cx={tx}
                  cy={ty}
                  r={isHighlighted ? 22 : 12}
                  fill={coord.accent}
                  fillOpacity={isHighlighted ? 0.24 : 0.08}
                  className="hive-city-conduit__beacon"
                />
              </g>
            )
          })}
        </svg>

        <div className="hive-city-scene__particles" aria-hidden="true">
          {AMBIENT_PARTICLES.map((particle, index) => (
            <span
              key={index}
              className={`hive-city-scene__particle hive-city-scene__particle--${particle.tone}`}
              style={{
                left: `${particle.left}%`,
                bottom: `${particle.bottom}%`,
                width: `${particle.size}px`,
                height: `${particle.size}px`,
                animationDuration: `${particle.duration}s`,
                animationDelay: `${particle.delay}s`
              }}
            />
          ))}
        </div>

        {hasInteractiveOverlay ? (
          <div className="hive-city-scene__pins">
            {positionedDepartments.map(({ department, coord }) => {
              const isActive = department.activeCount > 0
              const isSelected = selectedDepartment === department.key
              const percent = department.maxCapacity > 0
                ? Math.min(100, Math.round((department.activeCount / department.maxCapacity) * 100))
                : 0
              const iconNode = departmentIconRenderer
                ? departmentIconRenderer(department.icon)
                : <UiIcon name={department.icon} size={16} />

              return (
                <button
                  key={department.key}
                  type="button"
                  className={`hive-pin${isActive ? ' hive-pin--active' : ''}${isSelected ? ' hive-pin--selected' : ''}`}
                  data-testid={`hive-pin-${department.key}`}
                  data-dept={department.key}
                  data-active={isActive ? 'true' : 'false'}
                  aria-pressed={isSelected}
                  style={{
                    left: `${coord.x}%`,
                    top: `${coord.y}%`,
                    ['--pin-accent' as string]: coord.accent
                  }}
                  onClick={() => onSelectDepartment?.(department.key)}
                  onMouseEnter={() => setHoveredDepartment(department.key)}
                  onMouseLeave={() => setHoveredDepartment((current) => (current === department.key ? null : current))}
                >
                  <span className="hive-pin__icon" aria-hidden="true">{iconNode}</span>
                  <span className="hive-pin__body">
                    <strong className="hive-pin__title">{department.label}</strong>
                    <span className="hive-pin__subtitle">{department.subtitle}</span>
                    <span className="hive-pin__status">
                      <span className="hive-pin__dot" aria-hidden="true" />
                      {isActive
                        ? `${activeDepartmentsLabel} · ${department.activeCount}/${department.maxCapacity}`
                        : idleDepartmentsLabel}
                    </span>
                  </span>
                  {isActive ? (
                    <span className="hive-pin__bar" aria-hidden="true">
                      <span style={{ width: `${percent}%` }} />
                    </span>
                  ) : null}
                </button>
              )
            })}

            {queenOverlay ? (
              <div className="hive-city-scene__queen-anchor">
                {queenOverlay}
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  )
}
