import {
  memo,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type MutableRefObject
} from 'react'
import { Canvas, useFrame, useThree, type ThreeEvent } from '@react-three/fiber'
import * as THREE from 'three'

import {
  CITY_MAP_MAX_SATELLITES,
  SATELLITE_MAIN_AVENUE_RADIUS,
  capitalHouseAngle,
  capitalHouseRingRadius,
  capitalMainAvenueRadius,
  displayIslandHue,
  fnv1a32,
  type CityBuildingView,
  type CityIslandView
} from '@shared/hive-city-map'
import type { HiveCityHouseLot } from './HiveCityScene'
import {
  CitySkinProvider,
  GroundLabel3D,
  LabelSprite3D,
  buildingKindColor,
  buildingKindLabel,
  createStreetsTexture,
  towerDisplayHeight,
  useCitySkin,
  useIslandAccent,
  type CitySkin
} from './hive-city-skin'
import {
  AVENUE_RING_STEP,
  MAIN_AVENUES_RADIUS,
  QUEEN_GATE_ANGLE,
  RegulatedIntersection3D,
  RING_CONNECTOR_ANGLES,
  RoadSegment3D,
  computeQueenGateStopAngle,
  computeRingEntryStopProgress
} from './hive-traffic'
import { QueenRoads3D } from './hive-agent-simulation'

export { QUEEN_GATE_ANGLE, computeQueenGateStopAngle, computeRingEntryStopProgress }

/**
 * WebGL-архипелаг на `three` + `@react-three/fiber` (план 199.3).
 *
 * Камера фиксированная — строго вид сверху (ортокамера): фокус — это
 * плавный перенос цели взгляда и зум ортокамеры. Все раскладки приходят из
 * общего детерминированного билдера, как и в SVG-режиме. Геометрии,
 * материалы и CanvasTexture явно освобождаются (`dispose`) при размонтировании.
 */

const TOP_CAMERA_OFFSET = new THREE.Vector3(0, 56, 0.01)

const OVERVIEW_ZOOM_SOLO = 22
const OVERVIEW_ZOOM_ARCHIPELAGO = 14
const MIN_ZOOM = 8
const MAX_ZOOM = 70

export interface HiveArchipelago3DProps {
  readonly capital: CityIslandView
  readonly satellites: readonly CityIslandView[]
  readonly paused: boolean
  readonly selectedIslandId: string | null
  readonly selectedBuildingId: string | null
  readonly hoveredBuildingId?: string | null | undefined
  readonly droneCount: number
  readonly queenActive?: boolean | undefined
  readonly departmentHouses?: readonly HiveCityHouseLot[] | undefined
  readonly selectedDepartment?: string | null | undefined
  readonly onSelectDepartment?: ((departmentKey: string) => void) | undefined
  readonly onSelectIsland: (islandId: string) => void
  readonly onSelectBuilding: (islandId: string, buildingId: string) => void
  readonly onHoverBuilding?: ((buildingId: string | null) => void) | undefined
  readonly onBackgroundClick: () => void
  readonly onContextLost: () => void
  readonly timeScale?: number | undefined
}

const NO_RAYCAST = (): void => {}

const HITBOX_MATERIAL = new THREE.MeshBasicMaterial({
  transparent: true,
  opacity: 0,
  depthWrite: false
})

function useDisableRaycast(ref: MutableRefObject<THREE.Object3D | null>, dep?: unknown): void {
  useLayoutEffect(() => {
    const root = ref.current
    if (!root) return
    root.traverse((obj) => {
      obj.raycast = NO_RAYCAST
    })
  }, [ref, dep])
}

const IslandGround = memo(function IslandGround({
  view,
  selected,
  onSelect
}: {
  view: CityIslandView
  selected: boolean
  onSelect: () => void
}) {
  const ref = useRef<THREE.Group>(null)
  const decorRef = useRef<THREE.Group>(null)
  useDisableRaycast(decorRef, view.radius)
  useLayoutEffect(() => {
    const group = ref.current
    if (!group) return
    group.position.set(view.x, 0, view.z)
  }, [view])

  const accentHue = displayIslandHue(view.island.id, view.island.role)
  const streets = useMemo(() => createStreetsTexture(view), [view])
  useEffect(() => () => streets.dispose(), [streets])

  const top = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        map: streets
      }),
    [streets]
  )
  const rim = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: new THREE.Color().setHSL(accentHue / 360, selected ? 0.85 : 0.68, selected ? 0.68 : 0.56),
        transparent: true,
        opacity: selected ? 0.95 : 0.82
      }),
    [accentHue, selected]
  )
  useEffect(() => () => {
    top.dispose()
    rim.dispose()
  }, [top, rim])

  const rimSegments = useMemo(() => {
    const r = view.radius
    return Array.from({ length: 6 }, (_, index) => {
      const a0 = (index * Math.PI) / 3 + Math.PI / 6
      const a1 = ((index + 1) * Math.PI) / 3 + Math.PI / 6
      const x0 = Math.sin(a0) * r
      const z0 = Math.cos(a0) * r
      const x1 = Math.sin(a1) * r
      const z1 = Math.cos(a1) * r
      const dx = x1 - x0
      const dz = z1 - z0
      return {
        x: (x0 + x1) / 2,
        z: (z0 + z1) / 2,
        length: Math.hypot(dx, dz) + 0.08,
        angle: Math.atan2(dz, dx)
      }
    })
  }, [view.radius])

  return (
    <group ref={ref}>
      <mesh
        position={[0, -0.07, 0]}
        rotation={[0, Math.PI / 6, 0]}
        material={top}
        onPointerDown={(event: ThreeEvent<PointerEvent>) => {
          if (event.button !== 0) return
          event.stopPropagation()
          onSelect()
        }}
      >
        <cylinderGeometry args={[view.radius, view.radius, 0.14, 6]} />
      </mesh>
      <group ref={decorRef}>
        {rimSegments.map((seg, index) => (
          <mesh
            key={`rim-${index}`}
            position={[seg.x, 0.02, seg.z]}
            rotation={[0, -seg.angle, 0]}
            material={rim}
          >
            <boxGeometry args={[seg.length, 0.09, 0.14]} />
          </mesh>
        ))}
      </group>
    </group>
  )
})

interface BridgeRoadSpec {
  readonly islandId: string
  readonly phase: CityIslandView['phase']
  readonly angle: number
  readonly from: readonly [number, number]
  readonly to: readonly [number, number]
  readonly junctionCap: readonly [number, number]
  readonly junctionSat: readonly [number, number]
  readonly deckCenter: readonly [number, number]
  readonly deckLen: number
  readonly color: string
}

function BridgeSpan3D({
  road,
  skin,
  deckMaterial,
  centerLineMaterial,
  paused
}: {
  road: BridgeRoadSpec
  skin: CitySkin
  deckMaterial: THREE.Material
  centerLineMaterial: THREE.Material
  paused: boolean
}) {
  const groupRef = useRef<THREE.Group>(null)
  const scaleRef = useRef<number>(paused ? 1 : 0.04)

  useLayoutEffect(() => {
    const group = groupRef.current
    if (!group) return
    const init = paused ? 1 : 0.04
    scaleRef.current = init
    group.position.set(road.from[0], 0, road.from[1])
    group.rotation.set(0, -road.angle, 0)
    group.scale.set(init, 1, 1)
    group.visible = true
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [road.islandId])

  useFrame((_, delta) => {
    const group = groupRef.current
    if (!group) return
    const target = road.phase === 'exiting' ? 0.02 : 1
    if (Math.abs(scaleRef.current - target) < 1e-4) return
    const factor = paused ? 1 : 1 - Math.exp(-7.5 * Math.min(delta, 0.1))
    const next = Math.abs(target - scaleRef.current) * factor < 1e-4
      ? target
      : Math.max(0.02, scaleRef.current + (target - scaleRef.current) * factor)
    scaleRef.current = next
    group.position.set(road.from[0], 0, road.from[1])
    group.rotation.set(0, -road.angle, 0)
    group.scale.set(next, 1, 1)
    group.visible = next > 0.03
  })

  const totalLen = Math.hypot(road.to[0] - road.from[0], road.to[1] - road.from[1])
  const startR = Math.hypot(road.from[0], road.from[1])
  const deckMidR = Math.hypot(road.deckCenter[0], road.deckCenter[1])
  const localDeckX = Math.max(0, deckMidR - startR)

  return (
    <group ref={groupRef}>
      <mesh position={[localDeckX, -0.09, 0]} material={deckMaterial}>
        <boxGeometry args={[road.deckLen, 0.18, 1.16]} />
      </mesh>
      <mesh position={[localDeckX, 0.015, 0]}>
        <boxGeometry args={[road.deckLen, 0.025, 0.98]} />
        <meshBasicMaterial color={road.color} transparent opacity={0.45} />
      </mesh>
      <RoadSegment3D
        from={[0, 0]}
        to={[totalLen, 0]}
        bedMaterial={skin.roadBed}
        curbMaterial={skin.roadCurb}
        lineMaterial={centerLineMaterial}
        width={0.84}
        lanes={4}
        y={0.02}
      />
      <mesh position={[0, 0.032, 0]} rotation={[-Math.PI / 2, 0, Math.PI / 2]} material={skin.roadBed}>
        <planeGeometry args={[0.78, 0.08]} />
      </mesh>
      <mesh position={[totalLen, 0.032, 0]} rotation={[-Math.PI / 2, 0, Math.PI / 2]} material={skin.roadBed}>
        <planeGeometry args={[0.78, 0.08]} />
      </mesh>
    </group>
  )
}

/**
 * Наземные 4-полосные магистрали между Столицей и Городами-спутниками.
 */
const ConnectingBridges = memo(function ConnectingBridges({
  capital,
  satellites,
  paused
}: {
  capital: CityIslandView
  satellites: readonly CityIslandView[]
  paused: boolean
}) {
  const groupRef = useRef<THREE.Group>(null)
  useDisableRaycast(groupRef, satellites.length)
  const skin = useCitySkin()
  const centerLineMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#ffd980',
        transparent: true,
        opacity: 0.78,
        depthWrite: false
      }),
    []
  )
  const deckMaterial = useMemo(
    () =>
      new THREE.MeshStandardMaterial({
        color: '#151d36',
        roughness: 0.75,
        metalness: 0.2
      }),
    []
  )
  useEffect(() => () => {
    centerLineMaterial.dispose()
    deckMaterial.dispose()
  }, [centerLineMaterial, deckMaterial])

  const roadData = useMemo<readonly BridgeRoadSpec[]>(() => {
    const hexApothem = Math.cos(Math.PI / 6)
    const capOuterRingR = capital.radius * 0.78
    const capEdgeR = capital.radius * hexApothem
    return satellites
      .map((view) => {
        const dist = Math.hypot(view.x, view.z)
        if (dist < 1) return null
        const dirX = view.x / dist
        const dirZ = view.z / dist
        const angle = Math.atan2(dirZ, dirX)
        const satOuterRingR = view.radius * 0.78
        const satEdgeR = view.radius * hexApothem
        const startR = capOuterRingR + 0.41
        const endR = Math.max(startR + 0.4, dist - satOuterRingR - 0.41)
        const deckStart = Math.max(startR, capEdgeR - 0.1)
        const deckEnd = Math.max(deckStart + 0.2, dist - satEdgeR + 0.1)
        const hue = displayIslandHue(view.island.id, view.island.role)
        return {
          islandId: view.island.id,
          phase: view.phase,
          angle,
          from: [dirX * startR, dirZ * startR] as readonly [number, number],
          to: [dirX * endR, dirZ * endR] as readonly [number, number],
          junctionCap: [dirX * startR, dirZ * startR] as readonly [number, number],
          junctionSat: [dirX * endR, dirZ * endR] as readonly [number, number],
          deckCenter: [
            dirX * ((deckStart + deckEnd) / 2),
            dirZ * ((deckStart + deckEnd) / 2)
          ] as readonly [number, number],
          deckLen: Math.max(0.2, deckEnd - deckStart),
          color: `hsl(${hue}, 82%, 68%)`
        }
      })
      .filter((item): item is NonNullable<typeof item> => item !== null)
  }, [capital.radius, satellites])

  if (roadData.length === 0) return <></>

  return (
    <group ref={groupRef}>
      {roadData.map((road) => (
        <BridgeSpan3D
          key={`bridge-group-${road.islandId}`}
          road={road}
          skin={skin}
          deckMaterial={deckMaterial}
          centerLineMaterial={centerLineMaterial}
          paused={paused}
        />
      ))}
    </group>
  )
})

/**
 * Артикулированная башня одного модуля: стилобат с цветом категории,
 * разнообразные архитектурные силуэты и 3D-табличка при наведении/выборе.
 */
const BuildingComplex3D = memo(function BuildingComplex3D({
  view,
  item,
  skin,
  accent,
  selected,
  hovered,
  onSelectBuilding
}: {
  view: CityIslandView
  item: CityBuildingView
  skin: CitySkin
  accent: THREE.MeshStandardMaterial
  selected: boolean
  hovered: boolean
  onSelectBuilding: (islandId: string, buildingId: string) => void
}) {
  const [localHovered, setLocalHovered] = useState(false)
  const visualRef = useRef<THREE.Group>(null)
  useDisableRaycast(visualRef, item.building.id)

  const variant = useMemo(() => {
    switch (item.building.kind) {
      case 'gateway':
        return 2
      case 'storage':
        return 3
      case 'ui':
        return 0
      case 'worker':
        return 1
      default:
        return fnv1a32(`${view.island.id}/${item.building.id}/silhouette`) % 4
    }
  }, [view.island.id, item.building.id, item.building.kind])

  const depthFactor = THREE.MathUtils.clamp(
    0.92 - (item.z / Math.max(6, view.radius)) * 0.36,
    0.58,
    1.32
  )
  const height = towerDisplayHeight(item.height) * depthFactor
  const width = 1.0
  const x = view.x + item.x
  const z = view.z + item.z
  const kindColor = buildingKindColor(item.building.kind)

  const annex = useMemo(() => {
    const seed = fnv1a32(`${view.island.id}/${item.building.id}/annex`)
    const side1 = seed % 2 === 0 ? 1 : -1
    const side2 = (seed >>> 3) % 2 === 0 ? 1 : -1
    return {
      dx: side1 * 0.44,
      dz: -side2 * 0.42,
      w: 0.44 + ((seed >>> 5) % 14) / 100,
      d: 0.42 + ((seed >>> 8) % 14) / 100,
      h: Math.max(0.32, height * 0.45)
    }
  }, [view.island.id, item.building.id, height])

  const topDeck = useMemo(() => {
    switch (variant) {
      case 0:
        return { w: width * 0.78, dx: 0 }
      case 1:
        return { w: width * 0.54, dx: -width * 0.28 }
      case 2:
        return { w: width * 0.68, dx: 0 }
      default:
        return { w: width * 0.84, dx: 0 }
    }
  }, [variant])

  const handlePointerDown = (event: ThreeEvent<PointerEvent>): void => {
    if (event.button !== 0) return
    event.stopPropagation()
    onSelectBuilding(view.island.id, item.building.id)
  }
  const canHover = view.island.status === 'ready'
  const lit = selected || hovered || (canHover && localHovered)
  const radialRotY = item.x !== 0 || item.z !== 0 ? -Math.atan2(item.z, item.x) : 0

  return (
    <group
      position={[x, 0, z]}
      rotation={[0, radialRotY, 0]}
      userData={{ testid: `hive-archipelago-tower-${item.building.id}`, buildingId: item.building.id }}
    >
      <mesh
        position={[0, height * 0.5 + 0.05, 0]}
        material={HITBOX_MATERIAL}
        onPointerDown={handlePointerDown}
        onPointerOver={(event: ThreeEvent<PointerEvent>) => {
          if (!canHover || (event.buttons & 4) !== 0) return
          event.stopPropagation()
          setLocalHovered(true)
        }}
        onPointerOut={(event: ThreeEvent<PointerEvent>) => {
          if (!canHover) return
          event.stopPropagation()
          setLocalHovered(false)
        }}
      >
        <boxGeometry args={[1.6, height + 0.25, 1.6]} />
      </mesh>

      <group ref={visualRef}>
        <mesh position={[0, 0.05, 0]} material={skin.plinth}>
          <boxGeometry args={[1.56, 0.1, 1.56]} />
        </mesh>
        <mesh position={[0, 0.105, 0]}>
          <boxGeometry args={[1.5, 0.02, 1.5]} />
          <meshBasicMaterial color={kindColor} transparent opacity={lit ? 0.9 : 0.42} />
        </mesh>
        <mesh position={[annex.dx, 0.11 + annex.h / 2, annex.dz]} material={skin.roof}>
          <boxGeometry args={[annex.w, annex.h, annex.d]} />
        </mesh>
        <mesh position={[0, 0.12 + height * 0.36, 0]} material={skin.roof}>
          <boxGeometry args={[width * 1.12, height * 0.72, width * 1.12]} />
        </mesh>
        <mesh position={[topDeck.dx, 0.12 + height * 0.8, 0]} material={accent}>
          <boxGeometry args={[topDeck.w, height * 0.16, topDeck.w]} />
        </mesh>
        <mesh position={[-0.18, 0.12 + height * 0.9, 0.18]}>
          <boxGeometry args={[0.16, 0.1, 0.16]} />
          <meshBasicMaterial color={kindColor} />
        </mesh>
      </group>

      {lit ? (
        <>
          <mesh position={[0, height / 2 + 0.1, 0]} raycast={NO_RAYCAST}>
            <boxGeometry args={[1.64, height + 0.4, 1.64]} />
            <meshBasicMaterial color="#ffd98a" wireframe transparent opacity={0.45} />
          </mesh>
          <LabelSprite3D
            text={item.building.name}
            subtitle={`${buildingKindLabel(item.building.kind)} · ${item.building.file_count} ф.`}
            accentColor={kindColor}
            position={[0, height + 1.25, 0]}
            scale={[4.8, 1.26, 1]}
            fontSize={22}
          />
        </>
      ) : null}
    </group>
  )
})

/** Башни одного острова: артикулированные комплексы по детерминированным силуэтам. */
const IslandTowers = memo(function IslandTowers({
  view,
  skin,
  accent,
  selectedBuildingId,
  hoveredBuildingId,
  onSelectBuilding
}: {
  view: CityIslandView
  skin: CitySkin
  accent: THREE.MeshStandardMaterial
  selectedBuildingId: string | null
  hoveredBuildingId: string | null
  onSelectBuilding: (islandId: string, buildingId: string) => void
  onHoverBuilding?: ((buildingId: string | null) => void) | undefined
}) {
  const towerBuildings = view.buildings.filter((item) => item.building.kind !== 'citadel')
  if (towerBuildings.length === 0) return <></>
  return (
    <group>
      {towerBuildings.map((item) => (
        <BuildingComplex3D
          key={`${view.island.id}/${item.building.id}`}
          view={view}
          item={item}
          skin={skin}
          accent={accent}
          selected={selectedBuildingId === item.building.id}
          hovered={hoveredBuildingId === item.building.id}
          onSelectBuilding={onSelectBuilding}
        />
      ))}
    </group>
  )
})

/** Плоская табличка названия острова у передней грани плиты (смещается в сторону, если грань занята дорогой). */
const IslandNameplate = memo(function IslandNameplate({
  view,
  hasSouthBridge = false
}: {
  view: CityIslandView
  hasSouthBridge?: boolean
}) {
  const groupRef = useRef<THREE.Group>(null)
  useDisableRaycast(groupRef, view.island.id)
  const hue = displayIslandHue(view.island.id, view.island.role)
  const accent = `hsl(${hue}, 78%, 64%)`
  const subtitle =
    view.island.status === 'unavailable'
      ? 'Недоступен'
      : `${view.buildings.length} мод. · ${view.island.total_files} ф.`
  const isNorthSatellite = view.island.role !== 'capital' && view.z < -view.radius * 0.4 && Math.abs(view.x) < view.radius * 1.4
  const labelX = view.island.role === 'capital' && hasSouthBridge ? view.x + 3.2 : view.x
  const labelZ = isNorthSatellite ? view.z - view.radius - 0.95 : view.z + view.radius + 0.95
  return (
    <group ref={groupRef}>
      <GroundLabel3D
        text={view.island.name}
        subtitle={subtitle}
        accentColor={accent}
        position={[labelX, 0.025, labelZ]}
        size={[4.8, 1.2]}
        fontSize={34}
      />
    </group>
  )
})

/**
 * Городская уличная сеть острова: кольцевые проспекты, радиальные авеню,
 * уличные фонари с тёплыми огнями и зелёные скверы/деревья на бульварах.
 */
const CityStreets3D = memo(function CityStreets3D({
  view,
  skin,
  mainAvenueRadius = MAIN_AVENUES_RADIUS,
  paused = false,
  simClockRef
}: {
  view: CityIslandView
  skin: CitySkin
  mainAvenueRadius?: number
  paused?: boolean
  simClockRef?: MutableRefObject<number> | undefined
}) {
  const groupRef = useRef<THREE.Group>(null)
  useDisableRaycast(groupRef, view.buildings.length)
  const isCapital = view.island.role === 'capital'
  const laneMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#ffd980',
        transparent: true,
        opacity: 0.72,
        depthWrite: false
      }),
    []
  )
  useEffect(() => () => {
    laneMaterial.dispose()
  }, [laneMaterial])

  const ringRadii = useMemo(() => {
    const outer = view.radius * 0.78
    const rings: number[] = []
    const baseRingR = isCapital
      ? Math.max(MAIN_AVENUES_RADIUS, mainAvenueRadius)
      : SATELLITE_MAIN_AVENUE_RADIUS
    for (let r = baseRingR; r <= view.radius * 0.76; r += AVENUE_RING_STEP) {
      rings.push(Number(r.toFixed(2)))
    }
    if (rings.length === 0 || outer - (rings[rings.length - 1] ?? 0) > 1.4) {
      rings.push(Number(outer.toFixed(2)))
    }
    return rings
  }, [isCapital, mainAvenueRadius, view.radius])

  const moduleDriveways = useMemo(() => {
    const list: Array<{
      readonly from: readonly [number, number]
      readonly to: readonly [number, number]
      readonly junction: readonly [number, number]
      readonly angle: number
    }> = []
    if (ringRadii.length === 0) return list
    for (const item of view.buildings) {
      if (item.building.kind === 'citadel') continue
      const r = Math.hypot(item.x, item.z)
      if (r < 0.8) continue
      const angle = Math.atan2(item.z, item.x)
      const podiumHalf = 0.82
      const candidates = [...ringRadii].sort((a, b) => Math.abs(r - a) - Math.abs(r - b))
      let chosenRing = candidates[0] ?? 6.0
      for (const cand of candidates) {
        const entryCand = r >= cand ? Math.max(cand + 0.36, r - podiumHalf) : Math.min(cand - 0.36, r + podiumHalf)
        const midR = (cand + entryCand) / 2
        const mx = Math.cos(angle) * midR
        const mz = Math.sin(angle) * midR
        const hitsOther = view.buildings.some(
          (other) => other.building.id !== item.building.id && Math.hypot(other.x - mx, other.z - mz) < 1.15
        )
        if (!hitsOther) {
          chosenRing = cand
          break
        }
      }
      const edgeR = r >= chosenRing ? chosenRing + 0.41 : chosenRing - 0.41
      const entryR = r >= chosenRing ? Math.max(edgeR + 0.14, r - podiumHalf) : Math.min(edgeR - 0.14, r + podiumHalf)
      if (Math.abs(entryR - edgeR) < 0.12) continue
      list.push({
        from: [view.x + Math.cos(angle) * edgeR, view.z + Math.sin(angle) * edgeR],
        to: [view.x + Math.cos(angle) * entryR, view.z + Math.sin(angle) * entryR],
        junction: [view.x + Math.cos(angle) * edgeR, view.z + Math.sin(angle) * edgeR],
        angle
      })
    }
    return list
  }, [ringRadii, view.buildings, view.x, view.z])

  const ringConnectors = useMemo(() => {
    const list: Array<{
      readonly from: readonly [number, number]
      readonly to: readonly [number, number]
      readonly angle: number
      readonly r0: number
      readonly r1: number
    }> = []
    if (ringRadii.length < 2) return list
    for (let rIdx = 0; rIdx < ringRadii.length - 1; rIdx += 1) {
      const r0 = ringRadii[rIdx]
      const r1 = ringRadii[rIdx + 1]
      if (r0 === undefined || r1 === undefined) continue
      const startR = r0 + 0.41
      const endR = r1 - 0.41
      if (endR - startR < 0.25) continue
      for (const connAngle of RING_CONNECTOR_ANGLES) {
        list.push({
          from: [view.x + Math.cos(connAngle) * startR, view.z + Math.sin(connAngle) * startR],
          to: [view.x + Math.cos(connAngle) * endR, view.z + Math.sin(connAngle) * endR],
          angle: connAngle,
          r0: startR,
          r1: endR
        })
      }
    }
    return list
  }, [ringRadii, view.x, view.z])

  return (
    <group ref={groupRef}>
      {ringRadii.map((r, idx) => (
        <group key={`avenue-ring-${idx}`}>
          <mesh position={[view.x, 0.015, view.z]} rotation={[-Math.PI / 2, 0, 0]} material={skin.roadBed}>
            <ringGeometry args={[r - 0.42, r + 0.42, 72]} />
          </mesh>
          <mesh position={[view.x, 0.024, view.z]} rotation={[Math.PI / 2, 0, 0]} material={skin.roadCurb}>
            <torusGeometry args={[r - 0.41, 0.02, 6, 72]} />
          </mesh>
          <mesh position={[view.x, 0.024, view.z]} rotation={[Math.PI / 2, 0, 0]} material={skin.roadCurb}>
            <torusGeometry args={[r + 0.41, 0.02, 6, 72]} />
          </mesh>
          <mesh position={[view.x, 0.025, view.z]} rotation={[Math.PI / 2, 0, 0]} material={skin.roadCurb}>
            <torusGeometry args={[r - 0.205, 0.012, 6, 72]} />
          </mesh>
          <mesh position={[view.x, 0.025, view.z]} rotation={[Math.PI / 2, 0, 0]} material={skin.roadCurb}>
            <torusGeometry args={[r + 0.205, 0.012, 6, 72]} />
          </mesh>
          <mesh position={[view.x, 0.026, view.z]} rotation={[Math.PI / 2, 0, 0]} material={laneMaterial}>
            <torusGeometry args={[r, 0.03, 6, 72]} />
          </mesh>
          {RING_CONNECTOR_ANGLES.map((connAngle, cIdx) => (
            <RegulatedIntersection3D
              key={`tl-${idx}-${cIdx}`}
              x={view.x}
              z={view.z}
              radius={r}
              angle={connAngle}
              bedMaterial={skin.roadBed}
              paused={paused}
              simClockRef={simClockRef}
              hasOuterRadial={idx < ringRadii.length - 1}
            />
          ))}
        </group>
      ))}
      {ringConnectors.map((conn, idx) => (
        <group key={`ring-conn-${idx}`}>
          <RoadSegment3D
            from={conn.from}
            to={conn.to}
            bedMaterial={skin.roadBed}
            curbMaterial={skin.roadCurb}
            lineMaterial={laneMaterial}
            width={0.84}
            lanes={4}
            y={0.016}
          />
          <mesh
            position={[view.x + Math.cos(conn.angle) * conn.r0, 0.026, view.z + Math.sin(conn.angle) * conn.r0]}
            rotation={[-Math.PI / 2, 0, -conn.angle + Math.PI / 2]}
            material={skin.roadBed}
          >
            <planeGeometry args={[0.78, 0.08]} />
          </mesh>
          <mesh
            position={[view.x + Math.cos(conn.angle) * conn.r1, 0.026, view.z + Math.sin(conn.angle) * conn.r1]}
            rotation={[-Math.PI / 2, 0, -conn.angle + Math.PI / 2]}
            material={skin.roadBed}
          >
            <planeGeometry args={[0.78, 0.08]} />
          </mesh>
        </group>
      ))}
      {moduleDriveways.map((drive, idx) => (
        <group key={`module-drive-${idx}`}>
          <RoadSegment3D
            from={drive.from}
            to={drive.to}
            bedMaterial={skin.drivewayBed}
            curbMaterial={skin.roadCurb}
            lineMaterial={laneMaterial}
            width={0.46}
            y={0.018}
          />
          <mesh
            position={[drive.junction[0], 0.026, drive.junction[1]]}
            rotation={[-Math.PI / 2, 0, -drive.angle + Math.PI / 2]}
            material={skin.roadBed}
          >
            <planeGeometry args={[0.38, 0.08]} />
          </mesh>
        </group>
      ))}
    </group>
  )
})

/** Один дом работника департамента на кольце вокруг Цитадели Королевы. */
function DepartmentHouse3D({
  view,
  lot,
  houseMaterials,
  deliveredMapRef,
  highlighted,
  paused,
  onSelectHouse
}: {
  view: CityIslandView
  lot: HiveCityHouseLot
  houseMaterials: {
    readonly activeRoof: THREE.MeshStandardMaterial
    readonly idleRoof: THREE.MeshStandardMaterial
  }
  deliveredMapRef: React.MutableRefObject<Map<number, boolean>>
  highlighted: boolean
  paused: boolean
  onSelectHouse: (() => void) | undefined
}) {
  const angle = capitalHouseAngle(lot.slot, lot.total)
  const ringRadius = capitalHouseRingRadius(lot.total)
  const position = useMemo<[number, number, number]>(() => {
    const x = view.x + Math.cos(angle) * ringRadius
    const z = view.z + Math.sin(angle) * ringRadius
    return [x, 0, z]
  }, [angle, ringRadius, view.x, view.z])

  const visualRef = useRef<THREE.Group>(null)
  useDisableRaycast(visualRef, lot.slot)
  const roof = lot.active ? houseMaterials.activeRoof : houseMaterials.idleRoof
  const beaconRef = useRef<THREE.Mesh>(null)
  const coreRingMatRef = useRef<THREE.MeshBasicMaterial>(null)
  const garageLampMatRef = useRef<THREE.MeshBasicMaterial>(null)
  const lastStateRef = useRef<{ working: boolean; active: boolean; accent: string } | null>(null)

  useFrame((state) => {
    const delivered = paused || deliveredMapRef.current.get(lot.slot) === true
    const working = lot.active && delivered
    const prev = lastStateRef.current
    if (
      prev === null ||
      prev.working !== working ||
      prev.active !== lot.active ||
      prev.accent !== lot.accent
    ) {
      lastStateRef.current = { working, active: lot.active, accent: lot.accent }
      if (coreRingMatRef.current) {
        coreRingMatRef.current.color.set(working ? '#ffe6a3' : lot.accent)
        coreRingMatRef.current.opacity = working ? 0.96 : lot.active ? 0.65 : 0.28
      }
      if (garageLampMatRef.current) {
        garageLampMatRef.current.color.set(working ? '#7ef2c4' : '#3a4668')
      }
      if (beaconRef.current) {
        beaconRef.current.visible = working
      }
    }
    if (!working || !beaconRef.current) return
    if (paused) {
      beaconRef.current.position.y = 0.72 + Math.sin(lot.slot) * 0.06
      beaconRef.current.rotation.y = 0
      return
    }
    const t = state.clock.elapsedTime
    beaconRef.current.position.y = 0.72 + Math.sin(t * 2.4 + lot.slot) * 0.06
    beaconRef.current.rotation.y = t * 1.4
  })

  return (
    <group
      position={position}
      rotation={[0, -angle, 0]}
      userData={{
        testid: `hive-archipelago-house-${lot.departmentKey}-${lot.slot}`,
        dept: lot.departmentKey,
        active: lot.active
      }}
    >
      <mesh
        position={[0, 0.25, 0]}
        material={HITBOX_MATERIAL}
        onPointerDown={(event: ThreeEvent<PointerEvent>) => {
          if (event.button !== 0 || onSelectHouse === undefined) return
          event.stopPropagation()
          onSelectHouse()
        }}
      >
        <cylinderGeometry args={[0.62, 0.62, 0.5, 6]} />
      </mesh>
      <group ref={visualRef}>
        <mesh position={[0, 0.03, 0]} rotation={[0, Math.PI / 6, 0]}>
          <cylinderGeometry args={[0.56, 0.6, 0.06, 6]} />
          <meshBasicMaterial color="#151d36" />
        </mesh>
        <mesh position={[0, 0.065, 0]} rotation={[0, Math.PI / 6, 0]}>
          <cylinderGeometry args={[0.52, 0.54, 0.018, 6]} />
          <meshBasicMaterial color={lot.accent} transparent opacity={lot.active ? 0.78 : 0.34} />
        </mesh>

        <mesh position={[-0.44, 0.12, 0]}>
          <boxGeometry args={[0.24, 0.05, 0.34]} />
          <meshBasicMaterial ref={garageLampMatRef} color="#3a4668" />
        </mesh>

        <mesh position={[0, 0.22, 0]} rotation={[0, Math.PI / 6, 0]} material={roof}>
          <cylinderGeometry args={[0.25, 0.45, 0.26, 6]} />
        </mesh>
        <mesh position={[0, 0.36, 0]} rotation={[0, Math.PI / 6, 0]}>
          <cylinderGeometry args={[0.2, 0.22, 0.025, 6]} />
          <meshBasicMaterial color={lot.accent} transparent opacity={lot.active ? 0.96 : 0.48} />
        </mesh>
        <mesh position={[0, 0.38, 0]} rotation={[Math.PI / 2, 0, Math.PI / 6]}>
          <torusGeometry args={[0.23, 0.028, 6, 6]} />
          <meshBasicMaterial ref={coreRingMatRef} color={lot.accent} transparent opacity={0.5} />
        </mesh>
        {lot.active ? (
          <mesh ref={beaconRef} position={[0, 0.72, 0]} visible={paused}>
            <octahedronGeometry args={[0.13, 0]} />
            <meshBasicMaterial color={lot.accent} />
          </mesh>
        ) : null}
        {highlighted ? (
          <mesh position={[0, 0.08, 0]} rotation={[Math.PI / 2, 0, Math.PI / 6]}>
            <torusGeometry args={[0.63, 0.032, 6, 6]} />
            <meshBasicMaterial color="#ffe9b0" transparent opacity={0.95} />
          </mesh>
        ) : null}
      </group>
    </group>
  )
}

/** Маяк спутника: мягкое дышащее гало над центром острова без цитадели. */
const BeaconGlow3D = memo(function BeaconGlow3D({ view, paused }: { view: CityIslandView; paused: boolean }) {
  const groupRef = useRef<THREE.Group>(null)
  const coreRef = useRef<THREE.Mesh>(null)
  const haloRef = useRef<THREE.Mesh>(null)
  const pausedAppliedRef = useRef<boolean>(false)
  useDisableRaycast(groupRef, view.island.id)
  const hue = displayIslandHue(view.island.id, view.island.role)
  const color = `hsl(${hue}, 82%, 66%)`

  useFrame((state) => {
    if (paused) {
      if (pausedAppliedRef.current) return
      pausedAppliedRef.current = true
      if (coreRef.current) coreRef.current.scale.setScalar(0.38)
      if (haloRef.current) haloRef.current.scale.setScalar(1)
      return
    }
    pausedAppliedRef.current = false
    const t = state.clock.elapsedTime
    if (coreRef.current) coreRef.current.scale.setScalar(0.38 + 0.06 * Math.sin(t * 1.9))
    if (haloRef.current) haloRef.current.scale.setScalar(1 + 0.08 * Math.sin(t * 1.2))
  })
  return (
    <group ref={groupRef} position={[view.x, 1.8, view.z]}>
      <mesh ref={coreRef}>
        <octahedronGeometry args={[0.45, 0]} />
        <meshBasicMaterial color="#ffe9b0" />
      </mesh>
      <mesh ref={haloRef}>
        <sphereGeometry args={[1.0, 12, 12]} />
        <meshBasicMaterial color={color} transparent opacity={0.18} />
      </mesh>
    </group>
  )
})

/** Starfield: static deterministic points on a far shell; no motion cost. */
const Starfield = memo(function Starfield(): React.JSX.Element | null {
  const geometry = useMemo(() => {
    const positions = new Float32Array(300 * 3)
    for (let index = 0; index < 300; index += 1) {
      const seedA = ((fnv1a32(`star-a-${index}`) % 3600) / 3600) * Math.PI * 2
      const seedB = (fnv1a32(`star-b-${index}`) % 1000) / 1000
      const radius = 130 + seedB * 60
      const y = ((fnv1a32(`star-y-${index}`) % 1000) / 1000) * 90 + 6
      positions[index * 3] = Math.cos(seedA) * radius
      positions[index * 3 + 1] = y
      positions[index * 3 + 2] = Math.sin(seedA) * radius
    }
    const buffer = new THREE.BufferGeometry()
    buffer.setAttribute('position', new THREE.BufferAttribute(positions, 3))
    return buffer
  }, [])
  useEffect(() => () => geometry.dispose(), [geometry])
  return (
    <points geometry={geometry} raycast={NO_RAYCAST}>
      <pointsMaterial color="#9fb4ff" size={1.4} sizeAttenuation={false} transparent opacity={0.6} />
    </points>
  )
})

/** Дома работников департаментов и подписи их секторов на кольце столицы. */
const DepartmentHouses3D = memo(function DepartmentHouses3D({
  view,
  lots,
  deliveredMapRef,
  selectedDepartment,
  paused,
  onSelectDepartment
}: {
  view: CityIslandView
  lots: readonly HiveCityHouseLot[]
  deliveredMapRef: React.MutableRefObject<Map<number, boolean>>
  selectedDepartment: string | null
  paused: boolean
  onSelectDepartment: ((departmentKey: string) => void) | undefined
}) {
  const [selectedHouseSlot, setSelectedHouseSlot] = useState<number | null>(null)
  useEffect(() => {
    if (selectedDepartment === null) {
      setSelectedHouseSlot(null)
      return
    }
    const stillMatches = lots.some(
      (lot) => lot.slot === selectedHouseSlot && lot.departmentKey === selectedDepartment
    )
    if (!stillMatches) {
      setSelectedHouseSlot(null)
    }
  }, [selectedDepartment, lots, selectedHouseSlot])
  const houseMaterials = useMemo(
    () => ({
      activeRoof: new THREE.MeshStandardMaterial({
        color: new THREE.Color('#ebc052'),
        emissive: new THREE.Color('#d89b2a'),
        emissiveIntensity: 0.65,
        roughness: 0.35
      }),
      idleRoof: new THREE.MeshStandardMaterial({
        color: new THREE.Color('#475275'),
        roughness: 0.65
      })
    }),
    []
  )

  useEffect(
    () => () => {
      houseMaterials.activeRoof.dispose()
      houseMaterials.idleRoof.dispose()
    },
    [houseMaterials]
  )

  const sectorLabels = useMemo(() => {
    const groups = new Map<string, { label: string; accent: string; slots: number[]; activeCount: number }>()
    for (const lot of lots) {
      if (!lot.departmentLabel) continue
      const existing = groups.get(lot.departmentKey)
      if (existing) {
        existing.slots.push(lot.slot)
        if (lot.active) existing.activeCount += 1
      } else {
        groups.set(lot.departmentKey, {
          label: lot.departmentLabel,
          accent: lot.accent,
          slots: [lot.slot],
          activeCount: lot.active ? 1 : 0
        })
      }
    }
    const ringRadius = capitalHouseRingRadius(lots[0]?.total ?? 0)
    const mainAvenueR = capitalMainAvenueRadius(lots[0]?.total ?? lots.length)
    const labelR = Math.min(mainAvenueR - 0.85, ringRadius + 1.32)
    const total = Math.max(1, lots[0]?.total ?? lots.length)
    return Array.from(groups.entries()).map(([key, info]) => {
      const minSlot = Math.min(...info.slots)
      const maxSlot = Math.max(...info.slots)
      const angle = capitalHouseAngle((minSlot + maxSlot) / 2, total)
      const rotZ = Math.sin(angle) < 0 ? -angle - Math.PI / 2 : Math.PI / 2 - angle
      return {
        key,
        label: info.label,
        accent: info.accent,
        x: view.x + Math.cos(angle) * labelR,
        y: 0.052,
        z: view.z + Math.sin(angle) * labelR,
        rotZ
      }
    })
  }, [lots, view.x, view.z])

  const sectors = useMemo(() => {
    const groups = new Map<string, { accent: string; minSlot: number; maxSlot: number }>()
    const total = Math.max(1, lots[0]?.total ?? lots.length)
    for (const lot of lots) {
      const existing = groups.get(lot.departmentKey)
      if (existing) {
        existing.minSlot = Math.min(existing.minSlot, lot.slot)
        existing.maxSlot = Math.max(existing.maxSlot, lot.slot)
      } else {
        groups.set(lot.departmentKey, {
          accent: lot.accent,
          minSlot: lot.slot,
          maxSlot: lot.slot
        })
      }
    }
    const startOrigin = Math.PI * 0.5
    const fullArc = Math.PI * 2
    const gap = 0.025
    return Array.from(groups.entries()).map(([key, info]) => {
      const a0 = startOrigin + (info.minSlot / total) * fullArc + gap
      const a1 = startOrigin + ((info.maxSlot + 1) / total) * fullArc - gap
      return {
        key,
        accent: info.accent,
        startAngle: a0,
        endAngle: Math.max(a0 + 0.04, a1)
      }
    })
  }, [lots])

  if (lots.length === 0) return <></>
  const ringRadius = capitalHouseRingRadius(lots[0]?.total ?? 0)
  const mainAvenueR = capitalMainAvenueRadius(lots[0]?.total ?? lots.length)
  const innerBoulevardR = Math.max(2.9, ringRadius - 1.25)
  const sectorInnerR = innerBoulevardR + 0.44
  const sectorOuterR = Math.max(ringRadius + 2.0, mainAvenueR - 0.44)

  return (
    <group>
      {sectors.map((sector) => {
        const isSelected = selectedDepartment === sector.key
        const thetaStart = -sector.endAngle
        const thetaLength = sector.endAngle - sector.startAngle
        return (
          <group key={`dept-sector-${sector.key}`}>
            <mesh position={[view.x, 0.008, view.z]} rotation={[-Math.PI / 2, 0, 0]}>
              <ringGeometry args={[sectorInnerR, sectorOuterR, 24, 1, thetaStart, thetaLength]} />
              <meshBasicMaterial
                color={sector.accent}
                transparent
                opacity={isSelected ? 0.24 : 0.11}
                depthWrite={false}
              />
            </mesh>
            <mesh position={[view.x, 0.014, view.z]} rotation={[-Math.PI / 2, 0, 0]}>
              <ringGeometry args={[sectorOuterR - 0.1, sectorOuterR, 24, 1, thetaStart, thetaLength]} />
              <meshBasicMaterial
                color={sector.accent}
                transparent
                opacity={isSelected ? 0.92 : 0.62}
                depthWrite={false}
              />
            </mesh>
            {[sector.startAngle, sector.endAngle].map((boundAngle, bIdx) => {
              const rMid = (sectorInnerR + sectorOuterR) / 2
              const len = Math.max(0.4, sectorOuterR - sectorInnerR)
              return (
                <mesh
                  key={`sector-bound-${sector.key}-${bIdx}`}
                  position={[view.x + Math.cos(boundAngle) * rMid, 0.014, view.z + Math.sin(boundAngle) * rMid]}
                  rotation={[0, -boundAngle, 0]}
                >
                  <boxGeometry args={[len, 0.012, 0.035]} />
                  <meshBasicMaterial
                    color={sector.accent}
                    transparent
                    opacity={isSelected ? 0.85 : 0.5}
                    depthWrite={false}
                  />
                </mesh>
              )
            })}
          </group>
        )
      })}
      {lots.map((lot) => (
        <DepartmentHouse3D
          key={`${lot.departmentKey}-${lot.slot}`}
          view={view}
          lot={lot}
          houseMaterials={houseMaterials}
          deliveredMapRef={deliveredMapRef}
          highlighted={selectedDepartment === lot.departmentKey && selectedHouseSlot === lot.slot}
          paused={paused}
          onSelectHouse={
            onSelectDepartment === undefined
              ? undefined
              : () => {
                  const isSameHouse =
                    selectedDepartment === lot.departmentKey && selectedHouseSlot === lot.slot
                  if (isSameHouse) {
                    setSelectedHouseSlot(null)
                    onSelectDepartment(lot.departmentKey)
                    return
                  }
                  setSelectedHouseSlot(lot.slot)
                  if (selectedDepartment !== lot.departmentKey) {
                    onSelectDepartment(lot.departmentKey)
                  }
                }
          }
        />
      ))}
      {sectorLabels.map((sector) => (
        <GroundLabel3D
          key={`dept-label-${sector.key}`}
          text={sector.label}
          accentColor={sector.accent}
          position={[sector.x, sector.y, sector.z]}
          size={[2.6, 0.58]}
          rotationZ={sector.rotZ}
          fontSize={46}
        />
      ))}
    </group>
  )
})

/**
 * Шпиль Королевы: восьмигранные ярусы со светящимися фасадами, золотые пояса,
 * конус-корона, кристалл-маяк с пульсом и вертикальный луч света.
 */
const QueenTower3D = memo(function QueenTower3D({ view, skin, paused }: { view: CityIslandView; skin: CitySkin; paused: boolean }) {
  const groupRef = useRef<THREE.Group>(null)
  const beaconRef = useRef<THREE.Mesh>(null)
  const haloRef = useRef<THREE.Mesh>(null)
  const pausedAppliedRef = useRef<boolean>(false)
  useDisableRaycast(groupRef, view.island.id)

  const evaIconTexture = useMemo(() => {
    const canvas = document.createElement('canvas')
    canvas.width = 256
    canvas.height = 256
    const ctx = canvas.getContext('2d')
    if (ctx) {
      ctx.beginPath()
      ctx.arc(128, 128, 114, 0, Math.PI * 2)
      ctx.fillStyle = 'rgba(11, 15, 32, 0.9)'
      ctx.fill()
      ctx.lineWidth = 6
      ctx.strokeStyle = 'rgba(255, 217, 138, 0.82)'
      ctx.stroke()

      if (typeof Path2D === 'function') {
        ctx.save()
        const s = 192 / 48
        ctx.translate(32, 34)
        ctx.scale(s, s)

        const centerGrad = ctx.createLinearGradient(24, 7, 24, 39)
        centerGrad.addColorStop(0, '#ffffff')
        centerGrad.addColorStop(0.55, '#ddd6fe')
        centerGrad.addColorStop(1, '#93c5fd')

        const wingGrad = ctx.createLinearGradient(8, 16, 40, 38)
        wingGrad.addColorStop(0, 'rgba(233, 213, 255, 0.95)')
        wingGrad.addColorStop(1, 'rgba(167, 243, 208, 0.78)')

        ctx.fillStyle = centerGrad
        ctx.fill(new Path2D('M24 7C28.5 14 30 22 24 34C18 22 19.5 14 24 7Z'))

        ctx.fillStyle = wingGrad
        ctx.fill(new Path2D('M12 16C18.5 18 22 24.5 23.5 34.5C15.5 31.5 11.5 24.5 12 16Z'))
        ctx.fill(new Path2D('M36 16C29.5 18 26 24.5 24.5 34.5C32.5 31.5 36.5 24.5 36 16Z'))

        ctx.fillStyle = 'rgba(221, 214, 254, 0.78)'
        ctx.fill(new Path2D('M7 25.5C13.5 25.5 18.5 29.5 23 35.5C15 36.5 9.5 32.5 7 25.5Z'))

        ctx.fillStyle = 'rgba(147, 197, 253, 0.78)'
        ctx.fill(new Path2D('M41 25.5C34.5 25.5 29.5 29.5 25 35.5C33 36.5 38.5 32.5 41 25.5Z'))
        ctx.restore()
      }
    }
    const tex = new THREE.CanvasTexture(canvas)
    tex.colorSpace = THREE.SRGBColorSpace
    return tex
  }, [])

  useEffect(() => () => evaIconTexture.dispose(), [evaIconTexture])

  const plan = useMemo(() => {
    const citadel = view.buildings.find((item) => item.building.kind === 'citadel')
    return { x: view.x + (citadel?.x ?? 0), z: view.z + (citadel?.z ?? 0) }
  }, [view])

  useFrame((state) => {
    if (paused) {
      if (pausedAppliedRef.current) return
      pausedAppliedRef.current = true
      if (beaconRef.current) beaconRef.current.scale.setScalar(1)
      if (haloRef.current) haloRef.current.scale.setScalar(1)
      return
    }
    pausedAppliedRef.current = false
    const t = state.clock.elapsedTime
    if (beaconRef.current) {
      beaconRef.current.scale.setScalar(1 + 0.14 * Math.sin(t * 2.1))
    }
    if (haloRef.current) {
      haloRef.current.scale.setScalar(1 + 0.08 * Math.sin(t * 1.3))
    }
  })

  return (
    <group ref={groupRef} position={[plan.x, 0, plan.z]} scale={[1, 0.78, 1]}>
      <mesh position={[0, 0.15, 0]} rotation={[0, Math.PI / 6, 0]} material={skin.plinth}>
        <cylinderGeometry args={[2.3, 2.35, 0.3, 6]} />
      </mesh>
      <mesh position={[0, 0.5, 0]} rotation={[0, Math.PI / 6, 0]} material={skin.gold}>
        <coneGeometry args={[1.25, 0.7, 6]} />
      </mesh>
      <mesh ref={beaconRef} position={[0, 1.1, 0]}>
        <icosahedronGeometry args={[0.32, 0]} />
        <meshBasicMaterial color="#ffe9b0" />
      </mesh>
      <mesh ref={haloRef} position={[0, 1.1, 0]}>
        <sphereGeometry args={[0.85, 14, 14]} />
        <meshBasicMaterial color="#ffe9b0" transparent opacity={0.18} depthWrite={false} />
      </mesh>
      <mesh position={[0, 1.6, 0]} rotation={[-Math.PI / 2, 0, 0]}>
        <planeGeometry args={[1.65, 1.65]} />
        <meshBasicMaterial map={evaIconTexture} transparent depthWrite={false} />
      </mesh>
    </group>
  )
})

/** Fixed top-down rig: pans the look target and lerps ortho zoom without resetting on selection. */
function CameraRig({
  overviewZoom,
  worldDiameter,
  paused
}: {
  overviewZoom: number
  worldDiameter: number
  paused: boolean
}) {
  const { camera, gl, invalidate, size } = useThree()
  const compact = size.width <= 1180
  const leftReserve = compact ? 248 : 276
  const rightReserve = compact ? 320 : 352
  const topReserve = compact ? 28 : 34
  const bottomReserve = compact ? 168 : 196
  const availWidth = Math.max(240, size.width - leftReserve - rightReserve)
  const availHeight = Math.max(200, size.height - topReserve - bottomReserve)
  const fitZoom = Math.min(availWidth / worldDiameter, availHeight / worldDiameter)
  const responsiveZoom = Math.max(MIN_ZOOM, Math.min(overviewZoom, fitZoom))
  const defaultCenterX = size.width < 1350 && availWidth < worldDiameter * overviewZoom
    ? (rightReserve - leftReserve) / (2 * responsiveZoom)
    : 0
  const defaultCenterZ = (bottomReserve - topReserve) / (2 * responsiveZoom)

  const look = useRef(new THREE.Vector3(defaultCenterX, 0, defaultCenterZ))
  const desired = useRef(new THREE.Vector3(defaultCenterX, 0, defaultCenterZ))
  const desiredZoom = useRef(responsiveZoom)
  const userInteracted = useRef(false)
  const panRef = useRef<{ active: boolean; lastX: number; lastY: number }>({
    active: false,
    lastX: 0,
    lastY: 0
  })

  useEffect(() => {
    if (userInteracted.current) return
    desiredZoom.current = responsiveZoom
    desired.current.set(defaultCenterX, 0, defaultCenterZ)
    invalidate()
  }, [responsiveZoom, defaultCenterX, defaultCenterZ, invalidate])

  useEffect(() => {
    const dom = gl.domElement
    const host = dom.parentElement ?? dom
    const onWheel = (event: WheelEvent): void => {
      event.preventDefault()
      userInteracted.current = true
      const scale = event.deltaY < 0 ? 1.12 : 1 / 1.12
      desiredZoom.current = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, desiredZoom.current * scale))
      invalidate()
    }
    const onMouseDown = (event: MouseEvent): void => {
      if (event.button === 1) event.preventDefault()
    }
    const onAuxClick = (event: MouseEvent): void => {
      if (event.button === 1) {
        event.preventDefault()
        event.stopPropagation()
      }
    }
    const onPointerDown = (event: PointerEvent): void => {
      if (event.button !== 1) return
      event.preventDefault()
      event.stopPropagation()
      userInteracted.current = true
      panRef.current = { active: true, lastX: event.clientX, lastY: event.clientY }
      dom.style.cursor = 'grabbing'
      try {
        dom.setPointerCapture?.(event.pointerId)
      } catch {
        // Игнорируем в средах без pointer capture.
      }
    }
    const onPointerMove = (event: PointerEvent): void => {
      if (!panRef.current.active) return
      event.stopPropagation()
      const dx = event.clientX - panRef.current.lastX
      const dy = event.clientY - panRef.current.lastY
      panRef.current.lastX = event.clientX
      panRef.current.lastY = event.clientY
      const ortho = camera as THREE.OrthographicCamera
      const zoom = typeof ortho.zoom === 'number' && ortho.zoom > 0 ? ortho.zoom : desiredZoom.current
      const worldPerPx = 1 / Math.max(1, zoom)
      desired.current.x = Math.max(-120, Math.min(120, desired.current.x - dx * worldPerPx))
      desired.current.z = Math.max(-120, Math.min(120, desired.current.z - dy * worldPerPx))
      invalidate()
    }
    const stopPan = (event: PointerEvent): void => {
      if (!panRef.current.active && event.button !== 1) return
      if (event.button === 1) {
        event.preventDefault()
        event.stopPropagation()
      }
      if (!panRef.current.active) return
      panRef.current.active = false
      dom.style.cursor = ''
      try {
        dom.releasePointerCapture?.(event.pointerId)
      } catch {
        // Игнорируем, если захват уже снят.
      }
    }

    dom.addEventListener('wheel', onWheel, { passive: false })
    host.addEventListener('mousedown', onMouseDown, { capture: true })
    host.addEventListener('auxclick', onAuxClick, { capture: true })
    host.addEventListener('pointerdown', onPointerDown, { capture: true })
    host.addEventListener('pointermove', onPointerMove, { capture: true })
    host.addEventListener('pointerup', stopPan, { capture: true })
    host.addEventListener('pointercancel', stopPan, { capture: true })
    window.addEventListener('pointerup', stopPan)
    return () => {
      dom.removeEventListener('wheel', onWheel)
      host.removeEventListener('mousedown', onMouseDown, { capture: true })
      host.removeEventListener('auxclick', onAuxClick, { capture: true })
      host.removeEventListener('pointerdown', onPointerDown, { capture: true })
      host.removeEventListener('pointermove', onPointerMove, { capture: true })
      host.removeEventListener('pointerup', stopPan, { capture: true })
      host.removeEventListener('pointercancel', stopPan, { capture: true })
      window.removeEventListener('pointerup', stopPan)
      dom.style.cursor = ''
    }
  }, [camera, gl, invalidate])

  useFrame((_, delta) => {
    const ortho = camera as THREE.OrthographicCamera
    const zoomDiff = typeof ortho.zoom === 'number' ? Math.abs(desiredZoom.current - ortho.zoom) : 0
    if (look.current.distanceToSquared(desired.current) < 1e-8 && zoomDiff < 1e-5) {
      return
    }
    const factor = paused ? 1 : 1 - Math.exp(-7 * Math.min(delta, 0.1))
    if (factor >= 1 || look.current.distanceToSquared(desired.current) * factor * factor < 1e-8) {
      look.current.copy(desired.current)
    } else {
      look.current.lerp(desired.current, factor)
    }
    camera.position.copy(look.current).add(TOP_CAMERA_OFFSET)
    camera.lookAt(look.current)
    if (typeof ortho.zoom === 'number') {
      ortho.zoom = factor >= 1 || zoomDiff * factor < 1e-5
        ? desiredZoom.current
        : ortho.zoom + (desiredZoom.current - ortho.zoom) * factor
      ortho.updateProjectionMatrix()
    }
    invalidate()
  })

  return null
}

function scaleForPhase(phase: string): number {
  if (phase === 'exiting') return 0.02
  return 1
}

function IslandGroup3D({
  view,
  paused,
  selectedIsland,
  selectedBuildingId,
  hoveredBuildingId,
  droneCount: _droneCount,
  queenActive = true,
  departmentHouses,
  selectedDepartment,
  onSelectDepartment,
  hasSouthBridge = false,
  onSelectIsland,
  onSelectBuilding,
  onHoverBuilding,
  onBackgroundClick,
  timeScale = 1,
  simClockRef
}: {
  view: CityIslandView
  paused: boolean
  selectedIsland: boolean
  selectedBuildingId: string | null
  hoveredBuildingId: string | null
  droneCount?: number
  queenActive?: boolean
  departmentHouses?: readonly HiveCityHouseLot[]
  selectedDepartment?: string | null
  onSelectDepartment?: ((departmentKey: string) => void) | undefined
  hasSouthBridge?: boolean
  onSelectIsland: (islandId: string) => void
  onSelectBuilding: (islandId: string, buildingId: string) => void
  onHoverBuilding?: ((buildingId: string | null) => void) | undefined
  onBackgroundClick: () => void
  timeScale?: number | undefined
  simClockRef?: MutableRefObject<number> | undefined
}) {
  const groupRef = useRef<THREE.Group>(null)
  const scaleRef = useRef<number>(view.island.role === 'capital' || paused ? 1 : 0.04)
  const deliveredMapRef = useRef<Map<number, boolean>>(new Map())
  const skin = useCitySkin()
  const accent = useIslandAccent(displayIslandHue(view.island.id, view.island.role))

  const targetScale = scaleForPhase(view.phase)
  useFrame((_, delta) => {
    const group = groupRef.current
    if (!group) return
    if (Math.abs(scaleRef.current - targetScale) < 1e-4) return
    const factor = paused ? 1 : 1 - Math.exp(-7.5 * Math.min(delta, 0.1))
    const current = scaleRef.current
    const next = Math.abs(targetScale - current) * factor < 1e-4
      ? targetScale
      : Math.max(0.02, current + (targetScale - current) * factor)
    scaleRef.current = next
    // Масштабируем строго вокруг собственного центра острова (view.x, view.z),
    // чтобы город вырастал на своём месте, а не летел из центра столицы (0,0,0)!
    group.position.set(view.x * (1 - next), 0, view.z * (1 - next))
    group.scale.setScalar(next)
    group.visible = next > 0.03
  })

  useLayoutEffect(() => {
    const group = groupRef.current
    if (!group) return
    const initialScale = view.island.role === 'capital' || paused ? 1 : 0.04
    scaleRef.current = initialScale
    group.position.set(view.x * (1 - initialScale), 0, view.z * (1 - initialScale))
    group.scale.setScalar(initialScale)
    group.visible = true
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view.island.id])

  return (
    <group ref={groupRef}>
      <IslandGround
        view={view}
        selected={selectedIsland}
        onSelect={() => {
          if (selectedIsland || selectedBuildingId !== null || (selectedDepartment ?? null) !== null) {
            onBackgroundClick()
            return
          }
          onSelectIsland(view.island.id)
        }}
      />
      <CityStreets3D
        view={view}
        skin={skin}
        mainAvenueRadius={
          departmentHouses !== undefined && departmentHouses.length > 0
            ? capitalMainAvenueRadius(departmentHouses.length)
            : MAIN_AVENUES_RADIUS
        }
        paused={paused}
        simClockRef={simClockRef}
      />
      {departmentHouses !== undefined && departmentHouses.length > 0 ? (
        <QueenRoads3D
          view={view}
          lots={departmentHouses}
          skin={skin}
          deliveredMapRef={deliveredMapRef}
          active={queenActive}
          paused={paused}
          timeScale={timeScale}
          simClockRef={simClockRef}
        />
      ) : null}
      <IslandTowers
        view={view}
        skin={skin}
        accent={accent}
        selectedBuildingId={selectedIsland ? selectedBuildingId : null}
        hoveredBuildingId={view.island.status === 'ready' ? hoveredBuildingId : null}
        onSelectBuilding={onSelectBuilding}
        onHoverBuilding={onHoverBuilding}
      />
      {view.buildings.some((item) => item.building.kind === 'citadel') ? (
        <QueenTower3D view={view} skin={skin} paused={paused || !queenActive} />
      ) : (
        <BeaconGlow3D view={view} paused={paused || !queenActive} />
      )}
      {departmentHouses !== undefined && departmentHouses.length > 0 ? (
        <DepartmentHouses3D
          view={view}
          lots={departmentHouses}
          deliveredMapRef={deliveredMapRef}
          selectedDepartment={selectedDepartment ?? null}
          paused={paused}
          onSelectDepartment={onSelectDepartment}
        />
      ) : null}
      <IslandNameplate view={view} hasSouthBridge={hasSouthBridge} />
    </group>
  )
}

/**
 * Полный 3D-режим. Родитель оборачивает его в ErrorBoundary: при любой
 * ошибке контекста рендер переключается на SVG-режим без перезагрузки окна.
 */
export const HiveArchipelago3D = memo(function HiveArchipelago3D({
  capital,
  satellites,
  paused,
  selectedIslandId,
  selectedBuildingId,
  hoveredBuildingId = null,
  droneCount,
  queenActive = true,
  departmentHouses,
  selectedDepartment,
  onSelectDepartment,
  onSelectIsland,
  onSelectBuilding,
  onHoverBuilding,
  onBackgroundClick,
  onContextLost,
  timeScale = 1
}: HiveArchipelago3DProps): React.JSX.Element {
  const simClockRef = useRef<number>(0)
  const visibleSatellites = satellites.slice(0, CITY_MAP_MAX_SATELLITES)
  const soloZoom = Math.max(8, Math.min(24, (16.5 / Math.max(10, capital.radius)) * OVERVIEW_ZOOM_SOLO))
  const overviewZoom = visibleSatellites.length > 0 ? OVERVIEW_ZOOM_ARCHIPELAGO : soloZoom
  const worldDiameter = visibleSatellites.length > 0
    ? Math.max(48, capital.radius * 3.6)
    : Math.max(22, capital.radius * 2.15)

  const glRef = useRef<THREE.WebGLRenderer | null>(null)
  const handleContextLost = useMemo(() => {
    return (event: Event): void => {
      event.preventDefault()
      onContextLost()
    }
  }, [onContextLost])

  useEffect(() => {
    return () => {
      const gl = glRef.current
      if (gl) {
        gl.domElement.removeEventListener('webglcontextlost', handleContextLost, false)
        gl.dispose()
        glRef.current = null
      }
    }
  }, [handleContextLost])

  return (
    <div
      className="hive-archipelago hive-archipelago--3d"
      data-testid="hive-archipelago-3d"
    >
      <Canvas
        frameloop={paused || !queenActive ? 'demand' : 'always'}
        orthographic
        camera={{
          position: TOP_CAMERA_OFFSET.toArray(),
          zoom: overviewZoom,
          near: 0.1,
          far: 600
        }}
        dpr={[1, 1.5]}
        gl={{ antialias: true, powerPreference: 'low-power' }}
        style={{ width: '100%', height: '100%' }}
        onCreated={({ gl }) => {
          glRef.current = gl
          gl.domElement.addEventListener('webglcontextlost', handleContextLost, false)
        }}
        onPointerMissed={(event: unknown) => {
          if (event instanceof MouseEvent && event.button !== 0) return
          onBackgroundClick()
        }}
      >
        <color attach="background" args={['#080b16']} />
        <ambientLight intensity={0.58} />
        <hemisphereLight args={['#7c96e8', '#14192e', 0.85]} />
        <directionalLight position={[26, 40, 18]} intensity={1.25} color="#eef3ff" />
        <directionalLight position={[-22, 18, -20]} intensity={0.45} color="#78a2ff" />
        <pointLight position={[0, 12, 0]} intensity={55} distance={60} decay={1.5} color="#ffd98a" />
        <CameraRig overviewZoom={overviewZoom} worldDiameter={worldDiameter} paused={paused} />
        <Starfield />

        <CitySkinProvider>
          <FloatingCityGroup
            capital={capital}
            satellites={visibleSatellites}
            paused={paused}
            selectedIslandId={selectedIslandId}
            selectedBuildingId={selectedBuildingId}
            hoveredBuildingId={hoveredBuildingId}
            droneCount={droneCount}
            queenActive={queenActive}
            departmentHouses={departmentHouses ?? []}
            selectedDepartment={selectedDepartment ?? null}
            onSelectDepartment={onSelectDepartment}
            onSelectIsland={onSelectIsland}
            onSelectBuilding={onSelectBuilding}
            onHoverBuilding={onHoverBuilding}
            onBackgroundClick={onBackgroundClick}
            timeScale={timeScale}
            simClockRef={simClockRef}
          />
          <ConnectingBridges capital={capital} satellites={visibleSatellites} paused={paused} />
        </CitySkinProvider>
      </Canvas>
    </div>
  )
})

const FloatingCityGroup = memo(function FloatingCityGroup({
  capital,
  satellites,
  paused,
  selectedIslandId,
  selectedBuildingId,
  hoveredBuildingId = null,
  droneCount,
  queenActive = true,
  departmentHouses,
  selectedDepartment,
  onSelectDepartment,
  onSelectIsland,
  onSelectBuilding,
  onHoverBuilding,
  onBackgroundClick,
  timeScale = 1,
  simClockRef
}: {
  readonly capital: CityIslandView
  readonly satellites: readonly CityIslandView[]
  readonly paused: boolean
  readonly selectedIslandId: string | null
  readonly selectedBuildingId: string | null
  readonly hoveredBuildingId: string | null
  readonly droneCount: number
  readonly queenActive?: boolean
  readonly departmentHouses: readonly HiveCityHouseLot[]
  readonly selectedDepartment: string | null
  readonly onSelectDepartment: ((departmentKey: string) => void) | undefined
  readonly onSelectIsland: (islandId: string) => void
  readonly onSelectBuilding: (islandId: string, buildingId: string) => void
  readonly onHoverBuilding?: ((buildingId: string | null) => void) | undefined
  readonly onBackgroundClick: () => void
  readonly timeScale?: number | undefined
  readonly simClockRef?: MutableRefObject<number> | undefined
}): React.JSX.Element {
  const hasSouthBridge = satellites.some(
    (sat) => sat.z > capital.radius * 0.5 && Math.abs(sat.x) < 2.2
  )
  return (
    <group>
      <IslandGroup3D
        view={capital}
        paused={paused}
        selectedIsland={selectedIslandId === capital.island.id}
        selectedBuildingId={selectedBuildingId}
        hoveredBuildingId={hoveredBuildingId}
        droneCount={droneCount}
        queenActive={queenActive}
        departmentHouses={departmentHouses}
        selectedDepartment={selectedDepartment}
        onSelectDepartment={onSelectDepartment}
        hasSouthBridge={hasSouthBridge}
        onSelectIsland={onSelectIsland}
        onSelectBuilding={onSelectBuilding}
        onHoverBuilding={onHoverBuilding}
        onBackgroundClick={onBackgroundClick}
        timeScale={timeScale}
        simClockRef={simClockRef}
      />
      {satellites.map((view) => (
        <IslandGroup3D
          key={view.island.id}
          view={view}
          paused={paused}
          selectedIsland={selectedIslandId === view.island.id}
          selectedBuildingId={selectedBuildingId}
          hoveredBuildingId={hoveredBuildingId}
          selectedDepartment={selectedDepartment}
          onSelectIsland={onSelectIsland}
          onSelectBuilding={onSelectBuilding}
          onHoverBuilding={onHoverBuilding}
          onBackgroundClick={onBackgroundClick}
          timeScale={timeScale}
          simClockRef={simClockRef}
        />
      ))}
    </group>
  )
})
