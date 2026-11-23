import {
  Component,
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ErrorInfo,
  type ReactNode
} from 'react'

import {
  buildCityMapView,
  optimizeDepartmentOrder,
  type CityIsland,
  type HiveCityMapSnapshot
} from '@shared/hive-city-map'
import { HiveArchipelago3D } from './HiveArchipelago3D'

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

/** Эвристический агент для HUD-инспектора здания (только роль/действие). */
export interface HiveCityAgentHint {
  readonly id: string
  readonly role: string
  readonly currentAction: string
}

/**
 * Один дом на внутреннем кольце столицы: слот одного работника департамента
 * Улья.
 * Активные работники получают освещённый дом, ожидающие — приглушённый.
 */
export interface HiveCityHouseLot {
  /** Ключ департамента (клик по дому выбирает департамент). */
  readonly departmentKey: string
  /** Читаемое название департамента для 3D-подписи сектора. */
  readonly departmentLabel?: string | undefined
  /** Стабильный индекс лота в общей застройке (0..total-1). */
  readonly slot: number
  readonly total: number
  readonly active: boolean
  readonly accent: string
}

export type HiveCityMapPayload =
  | { readonly available: true; readonly snapshot: HiveCityMapSnapshot }
  | { readonly available: false; readonly snapshot: null }

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
  /**
   * Карта-архипелаг напрямую от `evohime-hive-daemon` (plan 199.3).
   * `undefined`/`null` — панорама как раньше; `available: false` — трекается
   * статус-индикатором поверх силуэта панорамы; снапшот — живой архипелаг.
   */
  readonly cityMap?: HiveCityMapPayload | null | undefined
  /** Отключает показ старой 2.5D-панорамы и пинов на вкладке Улья (пока грузится 3D-карта). */
  readonly suppressLegacyPanorama?: boolean | undefined
  /** Активные агенты Улья: эвристика для дронов и HUD-инспектора. */
  readonly activeAgents?: readonly HiveCityAgentHint[] | undefined
  /** Режим оптимизации расстановки зданий и секторов Королевой. */
  readonly queenOptimized?: boolean | undefined
}

interface DistrictAnchor {
  readonly x: number
  readonly y: number
  readonly accent: string
}

const QUEEN_ORBIT_CENTER = { x: 800, y: 245 }
const CONDUIT_ORIGIN = { x: 800, y: 405 }

/** Длительность анимации сворачивания острова (удержание перед размонтированием). */
const ISLAND_EXIT_HOLD_MS = 720
/** Длительность вступительной анимации острова. */
const ISLAND_ENTER_MS = 650

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

/** Детект WebGL для выбора режима: jsdom и сломанные драйверы дают false. */
function detectWebGl(): boolean {
  if (typeof document === 'undefined') return false
  try {
    const canvas = document.createElement('canvas')
    return Boolean(canvas.getContext('webgl2') ?? canvas.getContext('webgl'))
  } catch {
    return false
  }
}

type LifecycleEntry = { readonly phase: 'entering' | 'active' | 'exiting'; readonly since: number }
type LifecycleRegistry = ReadonlyMap<string, LifecycleEntry>

/**
 * Реестр жизненного цикла островов (план 199.3): `entering` → `active` →
 * `exiting` с гарантированным удержанием исчезающего острова в сцене.
 */
function useIslandLifecycle(
  satelliteIds: readonly string[],
  paused: boolean
): LifecycleRegistry {
  const [registry, setRegistry] = useState<LifecycleRegistry>(() => new Map())
  const timersRef = useRef<number[]>([])

  const clearTimers = useCallback((): void => {
    for (const timer of timersRef.current) window.clearTimeout(timer)
    timersRef.current = []
  }, [])

  useEffect(() => {
    clearTimers()
    setRegistry((current) => {
      const next = new Map<string, LifecycleEntry>()
      const now = Date.now()
      // Существующие активные/входящие острова сохраняются.
      for (const [id, entry] of current) {
        if (satelliteIds.includes(id)) {
          next.set(id, entry.phase === 'exiting' ? { phase: 'entering', since: now } : entry)
        }
        // exiting-острова без ключа в спутниках вымирают дальше (снизу).
      }
      for (const id of satelliteIds) {
        if (!next.has(id)) {
          next.set(id, { phase: paused ? 'active' : 'entering', since: now })
        }
      }
      // Исчезнувшие острова держим в сцене на время анимации сворачивания.
      for (const [id, entry] of current) {
        if (next.has(id)) continue
        if (entry.phase === 'exiting') {
          // Уже умирает: удержание продолжается до записи таймера ниже.
          next.set(id, entry)
        } else {
          next.set(id, { phase: 'exiting', since: now })
        }
      }
      return next
    })
  }, [satelliteIds, paused, clearTimers])

  // Завершение вступительных и исходящих анимаций по таймерам.
  useEffect(() => {
    clearTimers()
    const now = Date.now()
    for (const [id, entry] of registry) {
      if (entry.phase === 'entering') {
        const remaining = Math.max(0, ISLAND_ENTER_MS - (now - entry.since))
        timersRef.current.push(
          window.setTimeout(() => {
            setRegistry((current) => {
              const entryNow = current.get(id)
              if (entryNow === undefined || entryNow.phase !== 'entering') return current
              const next = new Map(current)
              next.set(id, { phase: 'active', since: Date.now() })
              return next
            })
          }, remaining)
        )
      } else if (entry.phase === 'exiting') {
        const remaining = Math.max(0, ISLAND_EXIT_HOLD_MS - (now - entry.since))
        timersRef.current.push(
          window.setTimeout(() => {
            setRegistry((current) => {
              const entryNow = current.get(id)
              if (entryNow === undefined || entryNow.phase !== 'exiting') return current
              const next = new Map(current)
              next.delete(id)
              return next
            })
          }, remaining)
        )
      }
    }
    return clearTimers
  }, [registry, clearTimers])

  useEffect(() => clearTimers, [clearTimers])

  return registry
}

function phasesView(registry: LifecycleRegistry): Map<string, 'entering' | 'active' | 'exiting'> {
  const view = new Map<string, 'entering' | 'active' | 'exiting'>()
  for (const [id, entry] of registry) view.set(id, entry.phase)
  return view
}

interface InspectorTarget {
  readonly islandId: string
  readonly buildingId: string
}

/**
 * ErrorBoundary вокруг WebGL-канвы: любая ошибка контекста или рендера
 * переключает сцену в SVG-режим без перезагрузки окна (план 199.3).
 */
class ArchipelagoErrorBoundary extends Component<
  { readonly onFallback: () => void; readonly children: ReactNode },
  { readonly failed: boolean }
> {
  override state = { failed: false }

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true }
  }

  override componentDidCatch(error: unknown, info: ErrorInfo): void {
    this.props.onFallback()
    console.error('hive-archipelago 3D render failed, falling back to 2D', error, info.componentStack)
  }

  override render(): ReactNode {
    return this.state.failed ? null : this.props.children
  }
}

 /** Эвристика связей «здание ↔ агенты»: только подписи ролей/путей, без утверждений о файлах. */
function agentsForBuilding(
  agents: readonly HiveCityAgentHint[] | undefined,
  islandName: string,
  buildingName: string,
  relativePath: string
): readonly HiveCityAgentHint[] {
  if (!agents || agents.length === 0) return []
  const pathKey = relativePath.toLowerCase()
  const buildingKey = buildingName.toLowerCase()
  const islandKey = islandName.toLowerCase()
  return agents.filter((agent) => {
    const role = agent.role.toLowerCase()
    const action = agent.currentAction.toLowerCase()
    return action.includes(pathKey) || role === islandKey || role === buildingKey
  }).slice(0, 8)
}

const SERVICE_LABELS: Record<string, string> = {
  citadel: 'Цитадель',
  service: 'Сервис',
  library: 'Библиотека',
  ui: 'Интерфейс',
  storage: 'Хранилище',
  gateway: 'Транспорт',
  worker: 'Фоновый процесс',
  module: 'Модуль'
}

function ArchipelagoInspector({
  islandId,
  buildingId,
  snapshot,
  status,
  activeAgents,
  onClose,
  onSelectBuilding,
  selectedBuildingId
}: {
  islandId: string
  buildingId: string | null
  snapshot: HiveCityMapSnapshot
  status: string
  activeAgents: readonly HiveCityAgentHint[] | undefined
  onClose: () => void
  onSelectBuilding: (islandId: string, buildingId: string) => void
  selectedBuildingId: string | null
}): React.JSX.Element {
  const island = snapshot.capital.id === islandId
    ? snapshot.capital
    : snapshot.satellites.find((item) => item.id === islandId)
  if (!island) return <></>
  const building = buildingId === null
    ? null
    : island.buildings.find((item) => item.id === buildingId) ?? null
  const relatedEdges = building === null
    ? island.edges.slice(0, 12)
    : island.edges
        .filter((edge) => edge.from_id === building.id || edge.to_id === building.id)
        .slice(0, 12)
  const nameFor = (id: string): string =>
    island.buildings.find((item) => item.id === id)?.name ?? id
  const agents = building === null
    ? (activeAgents ?? []).slice(0, 8)
    : agentsForBuilding(activeAgents, island.name, building.name, building.relative_path)

  return (
    <section
      className="hive-archipelago__inspector"
      data-testid="hive-archipelago-inspector"
      role="dialog"
      aria-label={`Модуль ${building?.name ?? island.name}`}
    >
      <header className="hive-archipelago__inspector-head">
        <div>
          <p className="hive-archipelago__inspector-eyebrow">
            {building === null ? `Остров · ${island.name}` : SERVICE_LABELS[building.kind] ?? building.kind}
          </p>
          <h3>{building === null ? island.name : building.name}</h3>
        </div>
        <button
          type="button"
          className="hive-archipelago__inspector-close"
          aria-label="Закрыть инспектор"
          data-testid="hive-archipelago-inspector-close"
          onClick={onClose}
        >
          <UiIcon name="x" size={14} />
        </button>
      </header>
      {building !== null ? (
        <dl className="hive-archipelago__inspector-rows">
          <div><dt>Путь</dt><dd>{building.relative_path}</dd></div>
          <div><dt>Язык</dt><dd>{building.language}</dd></div>
          <div><dt>Файлы</dt><dd>{building.file_count}</dd></div>
          <div><dt>Строки</dt><dd>{building.line_count}</dd></div>
        </dl>
      ) : (
        <dl className="hive-archipelago__inspector-rows">
          <div><dt>Экосистема</dt><dd>{island.ecosystem}</dd></div>
          <div><dt>Файлы</dt><dd>{island.total_files}</dd></div>
          <div><dt>Строки</dt><dd>{island.total_lines}</dd></div>
        </dl>
      )}
      <div className="hive-archipelago__inspector-links">
        <p className="hive-archipelago__inspector-label">
          {building === null ? 'Связи острова' : 'Связи модуля'}
        </p>
        {relatedEdges.length === 0 ? (
          <p className="hive-archipelago__inspector-muted">Связей не найдено</p>
        ) : (
          <ul>
            {relatedEdges.map((edge, index) => {
              const incoming = edge.to_id === (building?.id ?? island.id)
              const other = incoming ? edge.from_id : edge.to_id
              return (
                <li key={`${edge.kind}-${other}-${index}`}>
                  <span aria-hidden="true">{incoming ? '←' : '→'}</span> {nameFor(other)}
                  {' '}
                  <span className="hive-archipelago__inspector-muted">({edge.kind})</span>
                </li>
              )
            })}
          </ul>
        )}
      </div>
      <div className="hive-archipelago__inspector-agents">
        <p className="hive-archipelago__inspector-label">Активные агенты</p>
        {agents.length === 0 ? (
          <p className="hive-archipelago__inspector-muted">Нет активных агентов</p>
        ) : (
          <ul>
            {agents.map((agent) => (
              <li key={agent.id}>{agent.role}: {agent.currentAction}</li>
            ))}
          </ul>
        )}
      </div>
      <div className="hive-archipelago__inspector-buildings">
        <p className="hive-archipelago__inspector-label">Навигация по острову</p>
        <ul>
          {island.buildings.slice(0, 24).map((item) => (
            <li key={item.id}>
              <button
                type="button"
                aria-pressed={selectedBuildingId === item.id}
                data-testid={`hive-archipelago-building-${item.id}`}
                aria-label={`${item.name}: ${item.file_count} файлов, ${item.line_count} строк`}
                onClick={() => onSelectBuilding(islandId, item.id)}
              >
                {item.name}
              </button>
            </li>
          ))}
        </ul>
      </div>
      <p className="hive-archipelago__inspector-status">{status}</p>
    </section>
  )
}

const HiveArchipelago3DBoundary = memo(function HiveArchipelago3DBoundary({
  onFallback,
  archipelago
}: {
  readonly onFallback: () => void
  readonly archipelago: React.JSX.Element
}): React.JSX.Element {
  return (
    <ArchipelagoErrorBoundary onFallback={onFallback}>
      {archipelago}
    </ArchipelagoErrorBoundary>
  )
})

/**
 * Полноэкранная 2.5D-сцена города Улья:
 * - без `cityMap` — чистый 16:9 арт-город с мягкой HUD-виньеткой (как раньше);
 * - с `cityMap.available === false` — та же панорама-силуэт плюс статус-индикатор
 *   недоступности демона;
 * - со снапшотом карты — кликабельный живой архипелаг (3D WebGL или 2D SVG),
 *   инспектор модуля, фокус на остров и реестр жизненного цикла островов;
 * - интерактивные стеклянные пины департаментов и центральная карточка Королевы
 *   сохраняются как HUD-оверлей во всех режимах.
 */
export function HiveCityScene({
  status,
  departments = [],
  selectedDepartment = null,
  onSelectDepartment,
  queenWorking,
  queenOverlay,
  idleDepartmentsLabel = 'Ожидание',
  activeDepartmentsLabel = 'Работает',
  departmentIconRenderer,
  cityMap = null,
  suppressLegacyPanorama = false,
  activeAgents,
  queenOptimized = true
}: HiveCitySceneProps): React.JSX.Element {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const [hoveredDepartment, setHoveredDepartment] = useState<string | null>(null)
  const [webglBroken, setWebglBroken] = useState(false)
  const [webglKey, setWebglKey] = useState(0)

  const handleContextLost = useCallback(() => {
    setTimeout(() => {
      setWebglBroken(false)
      setWebglKey((k) => k + 1)
    }, 250)
  }, [])

  const reducedMotion = useReducedMotionPreference()
  const documentHidden = useDocumentHidden()
  const animationsPaused = reducedMotion || documentHidden

  const normalizedStatus = status.trim().toLowerCase() || 'idle'
  const queenActive =
    (normalizedStatus === 'active' || normalizedStatus === 'observing') && queenWorking !== false
  const corePalette = useMemo(
    () => resolveCorePalette(normalizedStatus, queenWorking ?? false),
    [normalizedStatus, queenWorking]
  )

  const hasInteractiveOverlay = departments.length > 0 || Boolean(queenOverlay)

  /**
   * Дома работников департаментов на кольце вокруг башни Королевы в центре
   * столицы (план 199.3): у каждого
   * слота вместимости — свой дом; активные освещены. Разброс по окружности
   * детерминированный, порядок отсортирован по ключу, чтобы застройка не
   * «пересобиралась» при смене активных работников.
   */
  const departmentHouses = useMemo<readonly HiveCityHouseLot[]>(() => {
    const ordered = optimizeDepartmentOrder(departments, queenOptimized)
    const total = ordered.reduce((sum, item) => sum + Math.max(1, item.maxCapacity), 0)
    const lots: HiveCityHouseLot[] = []
    let slot = 0
    for (const department of ordered) {
      const capacity = Math.max(1, department.maxCapacity)
      const accent =
        DEPARTMENT_COORDS[department.key]?.accent ?? FALLBACK_COORDS[0]?.accent ?? '#8d82e8'
      for (let index = 0; index < capacity; index += 1) {
        lots.push({
          departmentKey: department.key,
          departmentLabel: department.label,
          slot,
          total: Math.max(1, total),
          active: index < department.activeCount,
          accent
        })
        slot += 1
      }
    }
    return lots
  }, [departments, queenOptimized])

  const positionedDepartments = useMemo(() => {
    let fallbackIndex = 0
    return departments.map((department) => {
      const mapped = DEPARTMENT_COORDS[department.key]
      if (mapped) {
        return { department, coord: mapped }
      }
      const fallback =
        FALLBACK_COORDS[fallbackIndex % FALLBACK_COORDS.length] ?? { x: 50, y: 66, accent: '#8d82e8' }
      fallbackIndex += 1
      return { department, coord: fallback }
    })
  }, [departments])

  // --- Живой архипелаг (plan 199.3) ---------------------------------------
  const snapshot = cityMap?.available === true ? cityMap.snapshot : null
  const daemonUnavailable = cityMap?.available === false

  const satelliteIds = useMemo(
    () => (snapshot ? snapshot.satellites.map((island) => island.id) : []),
    [snapshot]
  )
  const lifecycle = useIslandLifecycle(satelliteIds, animationsPaused)

  const [focusIslandId, setFocusIslandId] = useState<string | null>(null)
  const [inspector, setInspector] = useState<InspectorTarget | null>(null)
  const [hoveredBuildingId, setHoveredBuildingId] = useState<string | null>(null)

  useEffect(() => {
    setFocusIslandId((current) =>
      current !== null && satelliteIds.includes(current) ? current : null)
    setInspector(null)
  }, [satelliteIds])

  // Архив известных островов: умирающий спутник уже не в snapshot.satellites,
  // но должен дорисовываться на время анимации сворачивания (exiting-hold).
  const islandArchiveRef = useRef(new Map<string, CityIsland>())
  useEffect(() => {
    if (snapshot === null) return
    islandArchiveRef.current.set(snapshot.capital.id, snapshot.capital)
    for (const island of snapshot.satellites) {
      islandArchiveRef.current.set(island.id, island)
    }
  }, [snapshot])

  const islandViews = useMemo(() => {
    if (snapshot === null) return null
    const phases = phasesView(lifecycle)
    const heldSatellites: CityIsland[] = []
    for (const [islandId, entry] of lifecycle) {
      if (entry.phase !== 'exiting') continue
      if (snapshot.satellites.some((island) => island.id === islandId)) continue
      const archived = islandArchiveRef.current.get(islandId)
      if (archived) heldSatellites.push(archived)
    }
    const effectiveSnapshot =
      heldSatellites.length === 0
        ? snapshot
        : { ...snapshot, satellites: [...snapshot.satellites, ...heldSatellites] }
    return buildCityMapView(effectiveSnapshot, phases, { optimizeByQueen: queenOptimized })
  }, [snapshot, lifecycle, queenOptimized])

  const handleClearSelection = useCallback((): void => {
    setFocusIslandId(null)
    setInspector(null)
    setHoveredBuildingId(null)
    if (selectedDepartment != null) {
      onSelectDepartment?.(selectedDepartment)
    }
  }, [selectedDepartment, onSelectDepartment])

  const handleSelectIsland = useCallback((islandId: string): void => {
    if (islandId === '') {
      handleClearSelection()
      return
    }
    setFocusIslandId(islandId)
  }, [handleClearSelection])

  const handleSelectBuilding = useCallback((islandId: string, buildingId: string): void => {
    setFocusIslandId(islandId)
    setInspector({ islandId, buildingId })
  }, [])

  const inspectorIsland = useMemo(() => {
    if (inspector === null || snapshot === null) return null
    return snapshot.capital.id === inspector.islandId
      ? snapshot.capital
      : snapshot.satellites.find((item) => item.id === inspector.islandId) ?? null
  }, [inspector, snapshot])

  useEffect(() => {
    if (inspector !== null && inspectorIsland === null) setInspector(null)
  }, [inspector, inspectorIsland])

  const inspectorBuildingId = useMemo(() => {
    if (inspector === null || inspectorIsland === null) return null
    return inspectorIsland.buildings.some((item) => item.id === inspector.buildingId)
      ? inspector.buildingId
      : null
  }, [inspector, inspectorIsland])

  const effectiveDepartmentHouses = useMemo<readonly HiveCityHouseLot[]>(
    () => (queenActive ? departmentHouses : departmentHouses.map((lot) => ({ ...lot, active: false }))),
    [queenActive, departmentHouses]
  )

  const droneCount = useMemo(() => {
    if (!queenActive) return 0
    const count = activeAgents?.length ?? 0
    return Math.min(64, count * 3)
  }, [activeAgents, queenActive])

  // Клавиатура: Escape закрывает инспектор, затем возвращает обзор архипелага.
  const handleKeyDown = useCallback((event: React.KeyboardEvent): void => {
    if (event.key !== 'Escape') return
    if (inspector !== null) {
      setInspector(null)
      return
    }
    if (focusIslandId !== null) {
      setFocusIslandId(null)
    }
  }, [inspector, focusIslandId])

  const showArchipelago = islandViews !== null
  // WebGL доступен? Детект один раз на монтирование (jsdom → false → SVG-режим).
  const [webglAvailable] = useState(detectWebGl)
  const use3D = showArchipelago && !webglBroken && webglAvailable

  return (
    <div
      ref={containerRef}
      className="hive-city-scene"
      data-testid="hive-city-scene"
      data-status={normalizedStatus}
      data-paused={animationsPaused ? 'true' : 'false'}
      data-mode={showArchipelago ? (use3D ? '3d' : 'unavailable') : 'panorama'}
      aria-hidden={hasInteractiveOverlay ? undefined : true}
      onMouseDown={(event) => {
        if (event.button === 1) event.preventDefault()
      }}
      onKeyDown={handleKeyDown}
    >
      {showArchipelago && islandViews !== null && snapshot !== null ? (
        <>
        <div className="hive-city-scene__world hive-city-scene__world--archipelago">
          <div
            className="hive-archipelago__frame"
            data-testid="hive-archipelago-frame"
          >
            {use3D ? (
              <HiveArchipelago3DBoundary
                key={webglKey}
                onFallback={() => setWebglBroken(true)}
                archipelago={
                  <HiveArchipelago3D
                    capital={islandViews.capital}
                    satellites={islandViews.satellites}
                    paused={animationsPaused}
                    selectedIslandId={focusIslandId}
                    selectedBuildingId={inspectorBuildingId}
                    hoveredBuildingId={hoveredBuildingId}
                    droneCount={droneCount}
                    queenActive={queenActive}
                    departmentHouses={effectiveDepartmentHouses}
                    selectedDepartment={selectedDepartment}
                    onSelectDepartment={onSelectDepartment}
                    onSelectIsland={handleSelectIsland}
                    onSelectBuilding={handleSelectBuilding}
                    onHoverBuilding={setHoveredBuildingId}
                    onBackgroundClick={handleClearSelection}
                    onContextLost={handleContextLost}
                  />
                }
              />
            ) : (
              <div
                className="hive-archipelago__webgl-unavailable"
                data-testid="hive-archipelago-webgl-unavailable"
                style={{
                  display: 'flex',
                  flexDirection: 'column',
                  alignItems: 'center',
                  justifyContent: 'center',
                  width: '100%',
                  height: '100%',
                  gap: '12px',
                  color: '#c4cce6'
                }}
              >
                <span>3D-сцена Улья недоступна (WebGL отключён или потерян контекст)</span>
                {webglBroken ? (
                  <button
                    type="button"
                    className="hive-archipelago__chip"
                    onClick={() => {
                      setWebglBroken(false)
                      setWebglKey((k) => k + 1)
                    }}
                  >
                    Повторить WebGL
                  </button>
                ) : null}
              </div>
            )}
          </div>

          {inspectorIsland !== null ? (
            <ArchipelagoInspector
              islandId={inspectorIsland.id}
              buildingId={inspector?.buildingId ?? null}
              snapshot={snapshot}
              status={normalizedStatus}
              activeAgents={activeAgents}
              onClose={() => setInspector(null)}
              onSelectBuilding={handleSelectBuilding}
              selectedBuildingId={inspectorBuildingId}
            />
          ) : null}

          {daemonUnavailable ? null : null}
        </div>

        {/* Клавиатурные остановки: только острова (≤13), Enter/Space — выбор.
            Чипы живут в корне сцены НАД нижним доком: внутри 16:9-контейнера
            мира с `bottom: 12px` они попадали под dock-карточки и прятались. */}
        <div className="hive-archipelago__island-chips" role="list" aria-label="Острова архипелага">
          {[snapshot.capital, ...snapshot.satellites].slice(0, 13).map((island) => (
            <button
              key={island.id}
              type="button"
              role="listitem"
              className="hive-archipelago__chip"
              data-testid={`hive-archipelago-chip-${island.id}`}
              aria-pressed={focusIslandId === island.id}
              aria-label={`${island.name}: ${island.total_files} файлов, ${island.total_lines} строк`}
              onClick={() => handleSelectIsland(island.id)}
            >
              {island.name}
            </button>
          ))}
        </div>
        </>
      ) : suppressLegacyPanorama ? (
        <div className="hive-city-scene__world hive-city-scene__world--archipelago">
          {daemonUnavailable ? (
            <p
              className="hive-archipelago__unavailable-status"
              data-testid="hive-city-unavailable"
              role="status"
              aria-live="polite"
            >
              Карта архипелага временно недоступна: демон Улья не отвечает.
            </p>
          ) : null}
        </div>
      ) : (
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

          {daemonUnavailable ? (
            <p
              className="hive-archipelago__unavailable-status"
              data-testid="hive-city-unavailable"
              role="status"
              aria-live="polite"
            >
              Карта архипелага временно недоступна: демон Улья не отвечает.
            </p>
          ) : null}
        </div>
      )}

      <div className="hive-city-scene__pins">
        {/* В режиме живого архипелага департаменты показаны домами на кольце
            вокруг башни Королевы (клик по дому выбирает департамент);
            фиксированные пины
            остались только у панорамного силуэта, где город отрисован арт-фоном. */}
        {!showArchipelago && !suppressLegacyPanorama
          ? positionedDepartments.map(({ department, coord }) => {
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
        })
          : null}

        {queenOverlay ? (
          <div className="hive-city-scene__queen-anchor">
            {queenOverlay}
          </div>
        ) : null}
      </div>
    </div>
  )
}
