import { memo, useMemo } from 'react'

import {
  CITY_MAP_MAX_SATELLITES,
  capitalHouseRingRadius,
  displayIslandHue,
  type CityBuilding,
  type CityBuildingView,
  type CityEdge,
  type CityIslandView
} from '@shared/hive-city-map'
import type { HiveCityHouseLot } from './HiveCityScene'

/**
 * Лёгкий SVG-рендерер карты-архипелага (план 199.3).
 *
 * Используется автоматически, когда WebGL недоступен, контекст потерян или
 * включён `prefers-reduced-motion`, — и как полностью кликабельный режим.
 * Геометрия и раскладка приходят из того же общего билдера, что и для
 * 3D-режима: оба режима рисуют один и тот же город.
 */

/** Half-extent of the fixed isometric viewport in projected units. */
const VIEW_HALF_WIDTH = 1000
const VIEW_HALF_HEIGHT = 620

const ISO_COS = 0.866
const ISO_SIN = 0.5
/** World units → projected 16:9-units scale (view half ~100 units). */
const SCALE = 9.4
/** Display height multiplier for `height_weight_bp / 1000`. */
const HEIGHT_SCALE = 3.4

export interface HiveArchipelago2DProps {
  readonly capital: CityIslandView
  readonly satellites: readonly CityIslandView[]
  readonly paused: boolean
  readonly selectedIslandId: string | null
  readonly selectedBuildingId: string | null
  readonly hoveredBuildingId: string | null
  /** Слоты-дома работников департаментов на кольце вокруг башни Королевы. */
  readonly departmentHouses?: readonly HiveCityHouseLot[]
  /** Ключ выбранного департамента (подсвечивает его дома). */
  readonly selectedDepartment?: string | null
  /** Клик по дому работника (выбор департамента), опционально. */
  readonly onSelectDepartment?: ((departmentKey: string) => void) | undefined
  readonly onSelectIsland: (islandId: string) => void
  readonly onSelectBuilding: (islandId: string, buildingId: string) => void
  readonly onHoverBuilding: (buildingId: string | null) => void
}

interface IsoPoint {
  readonly x: number
  readonly y: number
}

function project(x: number, z: number, y: number): IsoPoint {
  return {
    x: (x - z) * ISO_COS * SCALE,
    y: (x + z) * ISO_SIN * SCALE - y * SCALE
  }
}

function pointsToString(points: readonly IsoPoint[]): string {
  return points.map((point) => `${point.x.toFixed(1)},${point.y.toFixed(1)}`).join(' ')
}

/** Возвращает три грани изометрического параллелепипеда (верх, левая, правая). */
function isoBoxFaces(
  cx: number,
  cz: number,
  half: number,
  heightWorld: number
): { top: string; left: string; right: string } {
  const a = half
  const top = pointsToString([
    project(cx - a, cz - a, heightWorld),
    project(cx + a, cz - a, heightWorld),
    project(cx + a, cz + a, heightWorld),
    project(cx - a, cz + a, heightWorld)
  ])
  const left = pointsToString([
    project(cx - a, cz + a, 0),
    project(cx + a, cz + a, 0),
    project(cx + a, cz + a, heightWorld),
    project(cx - a, cz + a, heightWorld)
  ])
  const right = pointsToString([
    project(cx + a, cz + a, 0),
    project(cx + a, cz - a, 0),
    project(cx + a, cz - a, heightWorld),
    project(cx + a, cz + a, heightWorld)
  ])
  return { top, left, right }
}

function buildingHalfWidth(kind: CityBuilding['kind']): number {
  if (kind === 'citadel') return 1.15
  if (kind === 'service') return 0.85
  return 0.7
}

function islandAccent(island: CityIslandView['island']): string {
  return `hsl(${displayIslandHue(island.id, island.role)}, 72%, 62%)`
}

function islandAccentDeep(island: CityIslandView['island']): string {
  return `hsl(${displayIslandHue(island.id, island.role)}, 68%, 34%)`
}

/** Проецирует центр острова в координаты 16:9-вьюпорта (для HUD-оверлея). */
export function islandScreenAnchor(island: CityIslandView): IsoPoint {
  return project(island.x, island.z, 0)
}

function GroundPlate({ view }: { view: CityIslandView }) {
  const half = view.radius
  const plate = pointsToString([
    project(view.x - half, view.z - half, 0),
    project(view.x + half, view.z - half, 0),
    project(view.x + half, view.z + half, 0),
    project(view.x - half, view.z + half, 0)
  ])
  const accent = islandAccent(view.island)
  return (
    <polygon
      points={plate}
      fill={accent}
      fillOpacity={view.phase === 'exiting' ? 0.05 : 0.08}
      stroke={accent}
      strokeOpacity={view.phase === 'entering' ? 0.35 : 0.5}
      strokeWidth={1.6}
      className={`hive-archipelago__plate hive-archipelago__plate--${view.phase}`}
    />
  )
}

function ConnectingBridge({
  view,
  capital,
  paused
}: {
  view: CityIslandView
  capital: CityIslandView
  paused: boolean
}) {
  const from = project(capital.x, capital.z, 0.6)
  const to = project(view.x, view.z, 0.6)
  const midX = (from.x + to.x) / 2
  const midY = (from.y + to.y) / 2 - 46
  const accent = islandAccent(view.island)
  return (
    <g className={`hive-archipelago__bridge hive-archipelago__bridge--${view.phase}`}>
      <path
        d={`M ${from.x.toFixed(1)} ${from.y.toFixed(1)} Q ${midX.toFixed(1)} ${midY.toFixed(1)} ${to.x.toFixed(1)} ${to.y.toFixed(1)}`}
        fill="none"
        stroke={accent}
        strokeOpacity={0.34}
        strokeWidth={2}
        className="hive-archipelago__bridge-base"
      />
      <path
        d={`M ${from.x.toFixed(1)} ${from.y.toFixed(1)} Q ${midX.toFixed(1)} ${midY.toFixed(1)} ${to.x.toFixed(1)} ${to.y.toFixed(1)}`}
        fill="none"
        stroke={accent}
        strokeOpacity={0.95}
        strokeWidth={2.4}
        strokeDasharray="10 26"
        className="hive-archipelago__bridge-pulse"
        style={paused ? { animation: 'none', opacity: 0.5 } : undefined}
      />
    </g>
  )
}

function ConnectingEdge({
  edge,
  buildingIndex,
  view,
  highlighted
}: {
  edge: CityEdge
  buildingIndex: ReadonlyMap<string, CityBuildingView>
  view: CityIslandView
  highlighted: boolean
}) {
  const from = buildingIndex.get(edge.from_id)
  const to = buildingIndex.get(edge.to_id)
  if (!from || !to) return null
  const start = project(view.x + from.x, view.z + from.z, 0.15)
  const end = project(view.x + to.x, view.z + to.z, 0.15)
  return (
    <line
      x1={start.x.toFixed(1)}
      y1={start.y.toFixed(1)}
      x2={end.x.toFixed(1)}
      y2={end.y.toFixed(1)}
      stroke={islandAccentDeep(view.island)}
      strokeOpacity={highlighted ? 0.95 : 0.3}
      strokeWidth={Math.max(1, edge.weight / 34)}
      className={`hive-archipelago__road${highlighted ? ' hive-archipelago__road--lit' : ''}`}
    />
  )
}

function Tower({
  view,
  item,
  isCitadel,
  highlighted,
  onSelect,
  onHover
}: {
  view: CityIslandView
  item: CityBuildingView
  isCitadel: boolean
  highlighted: boolean
  onSelect: () => void
  onHover: (hovered: boolean) => void
}) {
  const half = buildingHalfWidth(item.building.kind)
  // Цитадель Королевы — самое высокое здание города, всегда доминирует.
  const height =
    item.height * HEIGHT_SCALE + (isCitadel ? Math.max(2.6, 7.0 - item.height * HEIGHT_SCALE) : 0)
  const faces = isoBoxFaces(view.x + item.x, view.z + item.z, half, height)
  const accent = islandAccent(view.island)
  const spire = isCitadel ? project(view.x + item.x, view.z + item.z, height + 1.5) : null
  const base = project(view.x + item.x, view.z + item.z, height)
  return (
    <g
      data-testid={`hive-tower-${item.building.id}`}
      data-building-id={item.building.id}
      className={`hive-archipelago__tower${highlighted ? ' hive-archipelago__tower--lit' : ''}`}
      onPointerDown={onSelect}
      onPointerEnter={() => onHover(true)}
      onPointerLeave={() => onHover(false)}
      style={{ cursor: 'pointer' }}
    >
      <polygon points={faces.left} fill={islandAccentDeep(view.island)} fillOpacity={highlighted ? 0.95 : 0.7} />
      <polygon points={faces.right} fill={accent} fillOpacity={highlighted ? 0.9 : 0.55} />
      <polygon points={faces.top} fill={accent} fillOpacity={highlighted ? 1 : 0.85} stroke="#0b0d18" strokeWidth={0.6} />
      {spire !== null ? (
        <>
          <line
            x1={base.x.toFixed(1)}
            y1={base.y.toFixed(1)}
            x2={spire.x.toFixed(1)}
            y2={spire.y.toFixed(1)}
            stroke="#ffd57a"
            strokeWidth={1.6}
            strokeOpacity={0.9}
            className="hive-archipelago__spire"
          />
          <circle cx={spire.x.toFixed(1)} cy={spire.y.toFixed(1)} r={3.4} fill="#ffe9b0" fillOpacity={0.95} className="hive-archipelago__core" />
        </>
      ) : null}
    </g>
  )
}

/** Один дом-лот работника департамента на оконной кольцевой застройке города. */
function DepartmentHouse2D({
  view,
  lot,
  highlighted,
  onSelectDepartment
}: {
  view: CityIslandView
  lot: HiveCityHouseLot
  highlighted: boolean
  onSelectDepartment: ((departmentKey: string) => void) | undefined
}) {
  const angle = (lot.slot / lot.total) * Math.PI * 2
  // Дома работников — на кольце между башней Королевы (центр) и кварталами
  // проекта, зеркально 3D-режиму.
  const ringRadius = capitalHouseRingRadius(lot.total)
  const center = project(view.x + Math.cos(angle) * ringRadius, view.z + Math.sin(angle) * ringRadius, 0)
  // Плоский силуэт дома (фронтон к зрителю), anchored в центре лота.
  const w = 14
  const h = 12
  const roof = 7
  const x = center.x
  const y = center.y
  const fill = lot.active ? lot.accent : '#3c4359'
  const fillOpacity = lot.active ? 0.92 : 0.4
  return (
    <g
      data-testid={`hive-archipelago-house-${lot.departmentKey}-${lot.slot}`}
      data-dept={lot.departmentKey}
      data-active={lot.active ? 'true' : 'false'}
      onPointerDown={(event) => {
        if (onSelectDepartment === undefined) return
        event.stopPropagation()
        onSelectDepartment(lot.departmentKey)
      }}
      style={onSelectDepartment === undefined ? undefined : { cursor: 'pointer' }}
      className={highlighted ? 'hive-archipelago__house hive-archipelago__house--selected' : 'hive-archipelago__house'}
    >
      {lot.active ? (
        <circle cx={x} cy={y} r={13} fill={lot.accent} fillOpacity={0.14} />
      ) : null}
      <path
        d={`M ${x - w / 2} ${y} L ${x - w / 2} ${y - h / 2} L ${x} ${y - h / 2 - roof} L ${x + w / 2} ${y - h / 2} L ${x + w / 2} ${y} Z`}
        fill={fill}
        fillOpacity={fillOpacity}
        stroke={highlighted ? '#ffe9b0' : '#0b0d18'}
        strokeWidth={highlighted ? 1.8 : 0.7}
      />
    </g>
  )
}

/** Дома работников столицы: каждый слот вместимости департамента — свой дом. */
function CapitalHouses2D({
  view,
  lots,
  selectedDepartment,
  onSelectDepartment
}: {
  view: CityIslandView
  lots: readonly HiveCityHouseLot[]
  selectedDepartment: string | null
  onSelectDepartment: ((departmentKey: string) => void) | undefined
}) {
  if (lots.length === 0) return null
  return (
    <g className="hive-archipelago__houses">
      {lots.map((lot) => (
        <DepartmentHouse2D
          key={`${lot.departmentKey}-${lot.slot}`}
          view={view}
          lot={lot}
          highlighted={selectedDepartment === lot.departmentKey}
          onSelectDepartment={onSelectDepartment}
        />
      ))}
    </g>
  )
}

function IslandCity({
  view,
  selectedIsland,
  selectedBuildingId,
  hoveredBuildingId,
  onSelectIsland,
  onSelectBuilding,
  onHoverBuilding
}: {
  view: CityIslandView
  selectedIsland: boolean
  selectedBuildingId: string | null
  hoveredBuildingId: string | null
  onSelectIsland: (islandId: string) => void
  onSelectBuilding: (islandId: string, buildingId: string) => void
  onHoverBuilding: (buildingId: string | null) => void
}) {
  const buildingIndex = useMemo(() => {
    const map = new Map<string, CityBuildingView>()
    for (const item of view.buildings) map.set(item.building.id, item)
    return map
  }, [view.buildings])

  if (view.island.status === 'unavailable') {
    const anchor = project(view.x, view.z, 0)
    return (
      <g className="hive-archipelago__unavailable">
        <GroundPlate view={view} />
        <text
          x={anchor.x}
          y={anchor.y}
          textAnchor="middle"
          fill="#93a0c7"
          fontSize={15}
          className="hive-archipelago__unavailable-label"
        >
          {view.island.name} · недоступен
        </text>
      </g>
    )
  }

  return (
    <g
      data-island={view.island.id}
      className={`hive-archipelago__island hive-archipelago__island--${view.phase}${selectedIsland ? ' hive-archipelago__island--selected' : ''}`}
      onPointerDown={() => onSelectIsland(view.island.id)}
    >
      <GroundPlate view={view} />
      {view.island.edges.map((edge, index) => {
        const highlighted =
          hoveredBuildingId === edge.from_id || hoveredBuildingId === edge.to_id
        return (
          <ConnectingEdge
            key={`${edge.from_id}>${edge.to_id}>${edge.kind}>${index}`}
            edge={edge}
            buildingIndex={buildingIndex}
            view={view}
            highlighted={highlighted}
          />
        )
      })}
      {view.buildings.map((item) => (
        <Tower
          key={item.building.id}
          view={view}
          item={item}
          isCitadel={item.building.kind === 'citadel'}
          highlighted={
            selectedBuildingId === item.building.id || hoveredBuildingId === item.building.id
          }
          onSelect={() => onSelectBuilding(view.island.id, item.building.id)}
          onHover={(hovered) => onHoverBuilding(hovered ? item.building.id : null)}
        />
      ))}
      <text
        x={project(view.x, view.z, 0).x}
        y={project(view.x, view.z + view.radius, 0).y + 26}
        textAnchor="middle"
        fill="#dfe6f7"
        fontSize={17}
        className="hive-archipelago__label"
      >
        {view.island.name}
      </text>
    </g>
  )
}

/**
 * Full 2D archipelago canvas. Deterministic layout comes from
 * `buildCityMapView`; this component only draws it isometrically.
 */
export const HiveArchipelago2D = memo(function HiveArchipelago2D({
  capital,
  satellites,
  paused,
  selectedIslandId,
  selectedBuildingId,
  hoveredBuildingId,
  departmentHouses,
  selectedDepartment,
  onSelectDepartment,
  onSelectIsland,
  onSelectBuilding,
  onHoverBuilding
}: HiveArchipelago2DProps): React.JSX.Element {
  const satellitesInRange = satellites.slice(0, CITY_MAP_MAX_SATELLITES)

  const islandAnchors = useMemo(() => {
    const anchors = new Map<string, IsoPoint>()
    anchors.set(capital.island.id, project(capital.x, capital.z, 0))
    for (const view of satellitesInRange) {
      anchors.set(view.island.id, project(view.x, view.z, 0))
    }
    return anchors
  }, [capital, satellitesInRange])

  return (
    <svg
      className="hive-archipelago"
      viewBox={`${-VIEW_HALF_WIDTH} ${-VIEW_HALF_HEIGHT} ${VIEW_HALF_WIDTH * 2} ${VIEW_HALF_HEIGHT * 2}`}
      preserveAspectRatio="xMidYMid meet"
      data-testid="hive-archipelago-2d"
      role="presentation"
      onPointerDown={(event) => {
        // Клик по фону снимает фокус с острова (бабблит только по «пустоте»).
        const target = event.target as Element | null
        if (
          target !== null &&
          (target === event.currentTarget || !target.closest('[data-island]'))
        ) {
          onSelectIsland('')
        }
      }}
    >
      <defs>
        <radialGradient id="hive-archipelago-water" cx="50%" cy="42%" r="72%">
          <stop offset="0%" stopColor="#131a30" />
          <stop offset="100%" stopColor="#070812" />
        </radialGradient>
      </defs>
      <rect
        x={-VIEW_HALF_WIDTH}
        y={-VIEW_HALF_HEIGHT}
        width={VIEW_HALF_WIDTH * 2}
        height={VIEW_HALF_HEIGHT * 2}
        fill="url(#hive-archipelago-water)"
      />

      {satellitesInRange.map((view) => (
        <ConnectingBridge
          key={`bridge-${view.island.id}`}
          view={view}
          capital={capital}
          paused={paused}
        />
      ))}

      <IslandCity
        view={capital}
        selectedIsland={selectedIslandId === capital.island.id}
        selectedBuildingId={selectedIslandId === capital.island.id ? selectedBuildingId : null}
        hoveredBuildingId={hoveredBuildingId}
        onSelectIsland={onSelectIsland}
        onSelectBuilding={onSelectBuilding}
        onHoverBuilding={onHoverBuilding}
      />
      {departmentHouses !== undefined && departmentHouses.length > 0 ? (
        <CapitalHouses2D
          view={capital}
          lots={departmentHouses}
          selectedDepartment={selectedDepartment ?? null}
          onSelectDepartment={onSelectDepartment}
        />
      ) : null}
      {satellitesInRange.map((view) => (
        <IslandCity
          key={view.island.id}
          view={view}
          selectedIsland={selectedIslandId === view.island.id}
          selectedBuildingId={selectedIslandId === view.island.id ? selectedBuildingId : null}
          hoveredBuildingId={hoveredBuildingId}
          onSelectIsland={onSelectIsland}
          onSelectBuilding={onSelectBuilding}
          onHoverBuilding={onHoverBuilding}
        />
      ))}

      {/* Светящиеся подписи-маяки для HUD-оверлея отрисованы отдельным слоем. */}
      {Array.from(islandAnchors.entries()).map(([islandId, anchor]) => (
        <circle
          key={`anchor-${islandId}`}
          cx={anchor.x}
          cy={anchor.y}
          r={0}
          fill="none"
          data-anchor={islandId}
        />
      ))}
    </svg>
  )
})

