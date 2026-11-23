import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode
} from 'react'
import { Canvas, useFrame, useThree, type ThreeEvent } from '@react-three/fiber'
import * as THREE from 'three'

import {
  CAPITAL_TOWER_INNER_RADIUS,
  CITY_MAP_MAX_SATELLITES,
  capitalHouseAngle,
  capitalHouseRingRadius,
  displayIslandHue,
  fnv1a32,
  type CityBuildingKind,
  type CityBuildingView,
  type CityEdgeKind,
  type CityIslandView
} from '@shared/hive-city-map'
import type { HiveCityHouseLot } from './HiveCityScene'

/**
 * WebGL-архипелаг на `three` + `@react-three/fiber` (план 199.3).
 *
 * Камера фиксированная изометрическая (угол не вращается): фокус — это
 * плавный перенос цели взгляда и зум ортокамеры. Все раскладки приходят из
 * общего детерминированного билдера, как и в SVG-режиме. Геометрии,
 * материалы и CanvasTexture явно освобождаются (`dispose`) при размонтировании.
 */

/** Fixed front-isometric camera offset aligned with +Z so top/bottom hex edges stay horizontal. */
const CAMERA_OFFSET = new THREE.Vector3(0, 34, 42)
/** Top-down camera offset (small Z offset keeps X/Z axes strictly aligned). */
const TOP_CAMERA_OFFSET = new THREE.Vector3(0, 56, 0.35)

const OVERVIEW_ZOOM_SOLO = 22
const OVERVIEW_ZOOM_ARCHIPELAGO = 14
const FOCUS_ZOOM = 28
const MIN_ZOOM = 8
const MAX_ZOOM = 70

/** Reusable per-frame scratch objects: no 60 Hz allocations inside useFrame. */
const FRAME_DUMMY = new THREE.Object3D()
const FRAME_VEC_A = new THREE.Vector3()
const FRAME_VEC_B = new THREE.Vector3()

/**
 * Visual height of a tower: low-weight buildings stay readable instead of
 * collapsing into flat plates (display-only; the data keeps `height_weight_bp`).
 */
function towerDisplayHeight(heightWeight: number): number {
  return Math.max(1.1, heightWeight * 4.0)
}

/** Цвет категории модуля для цоколя и световых поясов здания. */
function buildingKindColor(kind: CityBuildingKind): string {
  switch (kind) {
    case 'citadel':
      return '#f0c45a'
    case 'gateway':
      return '#f0b849'
    case 'storage':
      return '#4ec9a4'
    case 'ui':
      return '#a78bfa'
    case 'worker':
      return '#38bdf8'
    case 'service':
      return '#5ea8ff'
    case 'library':
      return '#818cf8'
    case 'module':
    default:
      return '#6d92e8'
  }
}

/** Читаемая подпись типа модуля для 3D-маркера. */
function buildingKindLabel(kind: CityBuildingKind): string {
  switch (kind) {
    case 'citadel':
      return 'Цитадель'
    case 'gateway':
      return 'Шлюз'
    case 'storage':
      return 'Хранилище'
    case 'ui':
      return 'Интерфейс'
    case 'worker':
      return 'Воркер'
    case 'service':
      return 'Сервис'
    case 'library':
      return 'Библиотека'
    case 'module':
    default:
      return 'Модуль'
  }
}

/** Цвет дороги по типу связи между модулями. */
function edgeKindColor(kind: CityEdgeKind): string {
  switch (kind) {
    case 'ipc':
      return '#f0c45a'
    case 'runtime':
      return '#a78bfa'
    case 'dependency':
    default:
      return '#5ea8ff'
  }
}

export interface HiveArchipelago3DProps {
  readonly capital: CityIslandView
  readonly satellites: readonly CityIslandView[]
  readonly paused: boolean
  readonly selectedIslandId: string | null
  readonly selectedBuildingId: string | null
  readonly hoveredBuildingId: string | null
  readonly droneCount: number
  /** Слоты-дома работников департаментов на кольце вокруг башни Королевы. */
  readonly departmentHouses?: readonly HiveCityHouseLot[]
  /** Ключ выбранного департамента (подсвечивает его дома). */
  readonly selectedDepartment?: string | null
  /** Клик по дому работника (выбор департамента), опционально. */
  readonly onSelectDepartment?: ((departmentKey: string) => void) | undefined
  readonly onSelectIsland: (islandId: string) => void
  readonly onSelectBuilding: (islandId: string, buildingId: string) => void
  readonly onHoverBuilding: (buildingId: string | null) => void
  readonly onBackgroundClick: () => void
  readonly onContextLost: () => void
}

const FACADE_VARIANTS = 4

interface FacadeSet {
  readonly material: THREE.MeshStandardMaterial
  readonly albedo: THREE.CanvasTexture
  readonly emissive: THREE.CanvasTexture
}

function createFacadeSet(variant: number): FacadeSet {
  const width = 128
  const height = 256
  const columns = 6
  const rows = 14
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  const context = canvas.getContext('2d')
  const emissiveCanvas = document.createElement('canvas')
  emissiveCanvas.width = width
  emissiveCanvas.height = height
  const emissiveContext = emissiveCanvas.getContext('2d')
  if (context !== null && emissiveContext !== null) {
    const bases = ['#2d385e', '#283254', '#333f68', '#2a3558']
    context.fillStyle = bases[variant % bases.length] ?? '#2d385e'
    context.fillRect(0, 0, width, height)
    emissiveContext.fillStyle = '#000000'
    emissiveContext.fillRect(0, 0, width, height)
    const litColors = ['#ffd37a', '#ffb861', '#ffe8b8', '#8cc4ff']
    const cellWidth = width / columns
    const cellHeight = height / rows
    for (let row = 0; row < rows; row += 1) {
      for (let column = 0; column < columns; column += 1) {
        const seed = fnv1a32(`facade-${variant}-${row}-${column}`)
        if (seed % 100 < 48) {
          const color = litColors[(seed >>> 7) % litColors.length] ?? '#ffd37a'
          context.fillStyle = color
          emissiveContext.fillStyle = color
        } else {
          context.fillStyle = '#141b33'
          emissiveContext.fillStyle = '#000000'
        }
        context.fillRect(
          column * cellWidth + cellWidth * 0.24,
          row * cellHeight + cellHeight * 0.26,
          cellWidth * 0.52,
          cellHeight * 0.46
        )
        emissiveContext.fillRect(
          column * cellWidth + cellWidth * 0.24,
          row * cellHeight + cellHeight * 0.26,
          cellWidth * 0.52,
          cellHeight * 0.46
        )
      }
    }
  }
  const albedo = new THREE.CanvasTexture(canvas)
  albedo.colorSpace = THREE.SRGBColorSpace
  const emissive = new THREE.CanvasTexture(emissiveCanvas)
  emissive.colorSpace = THREE.SRGBColorSpace
  const material = new THREE.MeshStandardMaterial({
    map: albedo,
    emissive: new THREE.Color('#ffffff'),
    emissiveMap: emissive,
    emissiveIntensity: 1.15,
    roughness: 0.62,
    metalness: 0.24
  })
  return { material, albedo, emissive }
}

/** Shared per-canvas materials of the whole archipelago. */
interface CitySkin {
  readonly facades: readonly FacadeSet[]
  readonly roof: THREE.MeshStandardMaterial
  readonly plinth: THREE.MeshStandardMaterial
  readonly gold: THREE.MeshStandardMaterial
  readonly roadBed: THREE.MeshBasicMaterial
}

function createCitySkin(): CitySkin {
  return {
    facades: Array.from({ length: FACADE_VARIANTS }, (_, index) => createFacadeSet(index)),
    roof: new THREE.MeshStandardMaterial({ color: '#1b2340', roughness: 0.75, metalness: 0.18 }),
    plinth: new THREE.MeshStandardMaterial({ color: '#202948', roughness: 0.82, metalness: 0.14 }),
    gold: new THREE.MeshStandardMaterial({
      color: new THREE.Color('#f0cf7b'),
      emissive: new THREE.Color('#e8b34a'),
      emissiveIntensity: 0.55,
      roughness: 0.28,
      metalness: 0.65
    }),
    roadBed: new THREE.MeshBasicMaterial({
      color: '#1b2747',
      transparent: true,
      opacity: 0.88
    })
  }
}

const CitySkinContext = createContext<CitySkin | null>(null)

function CitySkinProvider({ children }: { children: ReactNode }): React.JSX.Element {
  const skin = useMemo(() => createCitySkin(), [])
  useEffect(() => () => {
    for (const facade of skin.facades) {
      facade.albedo.dispose()
      facade.emissive.dispose()
      facade.material.dispose()
    }
    skin.roof.dispose()
    skin.plinth.dispose()
    skin.gold.dispose()
    skin.roadBed.dispose()
  }, [skin])
  return <CitySkinContext.Provider value={skin}>{children}</CitySkinContext.Provider>
}

function useCitySkin(): CitySkin {
  const contextSkin = useContext(CitySkinContext)
  return useMemo(() => contextSkin ?? createCitySkin(), [contextSkin])
}

/** Акцентный свет материалов острова: тонкие световые пояса зданий и кромка плиты. */
function useIslandAccent(hue: number): THREE.MeshStandardMaterial {
  const material = useMemo(
    () =>
      new THREE.MeshStandardMaterial({
        color: new THREE.Color().setHSL(hue / 360, 0.62, 0.54),
        emissive: new THREE.Color().setHSL(hue / 360, 0.78, 0.52),
        emissiveIntensity: 0.8,
        roughness: 0.35,
        metalness: 0.3
      }),
    [hue]
  )
  useEffect(() => () => material.dispose(), [material])
  return material
}

/** Верхняя плита острова: благородный ночной сланец с чёткой дорожной разметкой. */
function createStreetsTexture(view: CityIslandView): THREE.CanvasTexture {
  const size = 512
  const canvas = document.createElement('canvas')
  canvas.width = size
  canvas.height = size
  const context = canvas.getContext('2d')
  const hue = displayIslandHue(view.island.id, view.island.role)

  if (context !== null) {
    const centre = size / 2
    const gradient = context.createRadialGradient(centre, centre, 20, centre, centre, size / 2)
    gradient.addColorStop(0, '#222f58')
    gradient.addColorStop(0.5, '#17203e')
    gradient.addColorStop(1, '#10162b')
    context.fillStyle = gradient
    context.fillRect(0, 0, size, size)

    // Тонкая градостроительная сетка кварталов.
    context.strokeStyle = 'rgba(130, 165, 255, 0.07)'
    context.lineWidth = 1
    const step = size / 16
    for (let pos = step; pos < size; pos += step) {
      context.beginPath()
      context.moveTo(pos, 0)
      context.lineTo(pos, size)
      context.stroke()
      context.beginPath()
      context.moveTo(0, pos)
      context.lineTo(size, pos)
      context.stroke()
    }

    if (view.island.role === 'capital') {
      const ringRadius = (CAPITAL_TOWER_INNER_RADIUS / view.radius) * centre
      // Тёплая центральная площадь под Цитаделью Королевы.
      const plazaGradient = context.createRadialGradient(centre, centre, 8, centre, centre, ringRadius * 0.58)
      plazaGradient.addColorStop(0, 'rgba(240, 200, 108, 0.24)')
      plazaGradient.addColorStop(1, 'rgba(240, 200, 108, 0)')
      context.fillStyle = plazaGradient
      context.beginPath()
      context.arc(centre, centre, ringRadius * 0.58, 0, Math.PI * 2)
      context.fill()

      // Внешнее кольцо кварталов.
      context.strokeStyle = 'rgba(120, 168, 255, 0.2)'
      context.lineWidth = 3
      context.beginPath()
      context.arc(centre, centre, ringRadius * 1.18, 0, Math.PI * 2)
      context.stroke()

      // Кольцо двора агентов.
      context.strokeStyle = 'rgba(240, 204, 114, 0.28)'
      context.lineWidth = 4
      context.beginPath()
      context.arc(centre, centre, ringRadius * 0.76, 0, Math.PI * 2)
      context.stroke()
    } else {
      // Центральный маяк и радиальные лучи на спутнике.
      const beaconGrad = context.createRadialGradient(centre, centre, 4, centre, centre, size * 0.22)
      beaconGrad.addColorStop(0, `hsla(${hue}, 80%, 66%, 0.26)`)
      beaconGrad.addColorStop(1, `hsla(${hue}, 80%, 66%, 0)`)
      context.fillStyle = beaconGrad
      context.beginPath()
      context.arc(centre, centre, size * 0.22, 0, Math.PI * 2)
      context.fill()

      context.strokeStyle = `hsla(${hue}, 75%, 65%, 0.18)`
      context.lineWidth = 2.5
      context.beginPath()
      context.arc(centre, centre, size * 0.28, 0, Math.PI * 2)
      context.stroke()
    }
  }
  const texture = new THREE.CanvasTexture(canvas)
  texture.colorSpace = THREE.SRGBColorSpace
  return texture
}

function IslandGround({
  view,
  selected,
  onSelect
}: {
  view: CityIslandView
  selected: boolean
  onSelect: () => void
}) {
  const ref = useRef<THREE.Group>(null)
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
  const side = useMemo(
    () =>
      new THREE.MeshStandardMaterial({
        color: new THREE.Color().setHSL(accentHue / 360, 0.36, 0.18),
        roughness: 0.85,
        metalness: 0.15
      }),
    [accentHue]
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
  const rockMaterial = useMemo(
    () => new THREE.MeshStandardMaterial({ color: '#171d32', roughness: 0.92, metalness: 0.08 }),
    []
  )
  useEffect(() => () => {
    top.dispose()
    side.dispose()
    rim.dispose()
    rockMaterial.dispose()
  }, [top, side, rim, rockMaterial])

  // 6 рёбер контурного обода ровно по вершинам повёрнутого на 30° CylinderGeometry
  // (верхняя и нижняя грани соты горизонтальны по оси X).
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

  // Скальное основание под плитой.
  const rocks = useMemo(() => {
    const seed = fnv1a32(`${view.island.id}/rocks`)
    return Array.from({ length: 5 }, (_, index) => {
      const angle = (((seed >>> (index * 5)) % 360) / 180) * Math.PI
      const distance = 0.52 + (((seed >>> (index * 3 + 2)) % 100) / 100) * 0.24
      return {
        x: Math.cos(angle) * view.radius * distance,
        y: -1.6 - (((seed >>> (index * 4 + 1)) % 100) / 100) * view.radius * 0.22,
        z: Math.sin(angle) * view.radius * distance,
        size: 0.45 + (((seed >>> (index * 6)) % 100) / 100) * 0.55,
        rotation: (index * Math.PI) / 5
      }
    })
  }, [view.island.id, view.radius])
  const rockHeight = view.radius * 0.48

  return (
    <group ref={ref}>
      {/* Плита города: шестиугольная (flat-top: верхняя и нижняя грани горизонтальны). */}
      <mesh
        position={[0, -0.375, 0]}
        rotation={[0, Math.PI / 6, 0]}
        material={[side, top, side]}
        onPointerDown={(event: ThreeEvent<PointerEvent>) => {
          event.stopPropagation()
          onSelect()
        }}
      >
        <cylinderGeometry args={[view.radius, view.radius * 0.93, 0.75, 6]} />
      </mesh>
      {/* Светящийся контурный обод только по краям шестиугольника. */}
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
      {/* Скальное основание: перевёрнутая усечённая пирамида строго под плитой (y <= -0.75). */}
      <mesh position={[0, -0.75 - rockHeight / 2, 0]} rotation={[Math.PI, Math.PI / 6, 0]} material={rockMaterial}>
        <cylinderGeometry args={[view.radius * 0.24, view.radius * 0.88, rockHeight, 6]} />
      </mesh>
      <group>
        {rocks.map((rock, index) => (
          <mesh
            key={index}
            position={[rock.x, rock.y, rock.z]}
            rotation={[rock.rotation, rock.rotation * 0.7, 0]}
            material={rockMaterial}
          >
            <dodecahedronGeometry args={[rock.size, 0]} />
          </mesh>
        ))}
      </group>
    </group>
  )
}

/**
 * Компактная 3D-табличка в пространстве сцены (CanvasTexture sprite).
 * Используется для названий островов, секторов департаментов и инспектируемых зданий.
 */
function LabelSprite3D({
  text,
  subtitle,
  accentColor,
  position,
  scale,
  fontSize = 24
}: {
  text: string
  subtitle?: string | undefined
  accentColor: string
  position: readonly [number, number, number]
  scale: readonly [number, number, number]
  fontSize?: number
}) {
  const texture = useMemo(() => {
    const canvas = document.createElement('canvas')
    canvas.width = 320
    canvas.height = subtitle ? 84 : 64
    const context = canvas.getContext('2d')
    if (context) {
      const radius = 14
      context.fillStyle = 'rgba(11, 15, 30, 0.88)'
      if (typeof context.roundRect === 'function') {
        context.beginPath()
        context.roundRect(3, 3, canvas.width - 6, canvas.height - 6, radius)
        context.fill()
        context.strokeStyle = accentColor
        context.lineWidth = 2.5
        context.stroke()
      } else {
        context.fillRect(2, 2, canvas.width - 4, canvas.height - 4)
        context.strokeStyle = accentColor
        context.lineWidth = 2.5
        context.strokeRect(3, 3, canvas.width - 6, canvas.height - 6)
      }

      context.fillStyle = '#eef3ff'
      context.textAlign = 'center'
      context.textBaseline = 'middle'
      let px = fontSize
      context.font = `600 ${px}px system-ui, -apple-system, sans-serif`
      const maxWidth = canvas.width - 36
      while (px > 13 && context.measureText(text).width > maxWidth) {
        px -= 2
        context.font = `600 ${px}px system-ui, -apple-system, sans-serif`
      }
      const titleY = subtitle ? canvas.height * 0.36 : canvas.height * 0.52
      context.fillText(text, canvas.width / 2, titleY, maxWidth)

      if (subtitle) {
        context.fillStyle = '#9bb0e0'
        context.font = '500 16px system-ui, -apple-system, sans-serif'
        context.fillText(subtitle, canvas.width / 2, canvas.height * 0.72, maxWidth)
      }
    }
    const created = new THREE.CanvasTexture(canvas)
    created.colorSpace = THREE.SRGBColorSpace
    return created
  }, [text, subtitle, accentColor, fontSize])

  useEffect(() => () => texture.dispose(), [texture])

  return (
    <sprite position={[position[0], position[1], position[2]]} scale={[scale[0], scale[1], scale[2]]}>
      <spriteMaterial map={texture} transparent depthWrite={false} />
    </sprite>
  )
}

/**
 * Световые мосты между Столицей и Спутниками с двусторонними вытянутыми
 * импульсами информации, ориентированными по касательной к дуге.
 */
function ConnectingBridges({
  satellites,
  paused
}: {
  satellites: readonly CityIslandView[]
  paused: boolean
}) {
  const pulsesRef = useRef<THREE.InstancedMesh>(null)
  const pulseMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: new THREE.Color('#ffffff'),
        transparent: true,
        opacity: 0.94
      }),
    []
  )
  useEffect(() => () => pulseMaterial.dispose(), [pulseMaterial])

  const bridgeData = useMemo(
    () =>
      satellites.map((view) => {
        const from = new THREE.Vector3(0, 0.7, 0)
        const to = new THREE.Vector3(view.x, 0.7, view.z)
        const mid = from.clone().add(to).multiplyScalar(0.5)
        mid.y += 4.5
        const curve = new THREE.QuadraticBezierCurve3(from, mid, to)
        const hue = displayIslandHue(view.island.id, view.island.role)
        return {
          islandId: view.island.id,
          curve,
          color: `hsl(${hue}, 82%, 68%)`
        }
      }),
    [satellites]
  )

  // По 4 пакета на каждый мост: 2 туда (золотые от Столицы), 2 обратно (цвет спутника).
  const packetsPerBridge = 4
  const pulseCount = Math.max(0, bridgeData.length * packetsPerBridge)
  const instanceColor = useMemo(() => new THREE.Color(), [])

  useLayoutEffect(() => {
    const pulses = pulsesRef.current
    if (!pulses || pulseCount === 0) return
    bridgeData.forEach((bridge, bIdx) => {
      for (let p = 0; p < packetsPerBridge; p += 1) {
        const idx = bIdx * packetsPerBridge + p
        const isOutbound = p < 2
        instanceColor.set(isOutbound ? '#ffe082' : bridge.color)
        pulses.setColorAt(idx, instanceColor)
      }
    })
    if (pulses.instanceColor !== null) pulses.instanceColor.needsUpdate = true
  }, [bridgeData, pulseCount, instanceColor])

  useFrame((state) => {
    const pulses = pulsesRef.current
    if (!pulses || pulseCount === 0) return
    const dummy = FRAME_DUMMY
    const time = paused ? 0 : state.clock.elapsedTime
    bridgeData.forEach((bridge, bIdx) => {
      for (let p = 0; p < packetsPerBridge; p += 1) {
        const idx = bIdx * packetsPerBridge + p
        if (idx >= pulses.count) continue
        const isOutbound = p < 2
        const phase = (p % 2) * 0.5 + bIdx * 0.17
        const rawT = paused ? 0.3 + p * 0.15 : (time * 0.24 + phase) % 1
        const t = isOutbound ? rawT : 1 - rawT
        bridge.curve.getPoint(t, FRAME_VEC_A)
        bridge.curve.getTangent(t, FRAME_VEC_B)
        if (!isOutbound) FRAME_VEC_B.negate()
        dummy.position.copy(FRAME_VEC_A)
        dummy.lookAt(
          FRAME_VEC_A.x + FRAME_VEC_B.x,
          FRAME_VEC_A.y + FRAME_VEC_B.y,
          FRAME_VEC_A.z + FRAME_VEC_B.z
        )
        dummy.scale.set(0.16, 0.16, 0.65)
        dummy.updateMatrix()
        pulses.setMatrixAt(idx, dummy.matrix)
      }
    })
    pulses.instanceMatrix.needsUpdate = true
  })

  if (bridgeData.length === 0) return <></>

  return (
    <group>
      {bridgeData.map((bridge) => (
        <group key={`bridge-group-${bridge.islandId}`}>
          <mesh>
            <tubeGeometry args={[bridge.curve, 28, 0.11, 6, false]} />
            <meshBasicMaterial color="#2a4475" transparent opacity={0.45} />
          </mesh>
          <mesh>
            <tubeGeometry args={[bridge.curve, 28, 0.04, 6, false]} />
            <meshBasicMaterial color={bridge.color} transparent opacity={0.78} />
          </mesh>
        </group>
      ))}
      <instancedMesh
        ref={pulsesRef}
        args={[undefined, pulseMaterial, Math.max(1, pulseCount)]}
        visible={pulseCount > 0}
        frustumCulled={false}
      >
        <boxGeometry args={[1, 1, 1]} />
      </instancedMesh>
    </group>
  )
}

/**
 * Артикулированная башня одного модуля: стилобат с цветом категории,
 * разнообразные архитектурные силуэты и 3D-табличка при наведении/выборе.
 */
function BuildingComplex3D({
  view,
  item,
  skin,
  accent,
  selected,
  hovered,
  onSelectBuilding,
  onHoverBuilding
}: {
  view: CityIslandView
  item: CityBuildingView
  skin: CitySkin
  accent: THREE.MeshStandardMaterial
  selected: boolean
  hovered: boolean
  onSelectBuilding: (islandId: string, buildingId: string) => void
  onHoverBuilding: (buildingId: string | null) => void
}) {
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

  // Изометрический амфитеатр высот: ближние к камере здания ниже, дальние — выше,
  // чтобы при фиксированном ракурсе без вращения здания не перекрывали друг друга.
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

  // Одна аккуратная пристройка внутри подиума квартала (воздушная застройка без наползания на улицы).
  const annexes = useMemo(() => {
    const seed = fnv1a32(`${view.island.id}/${item.building.id}/annex`)
    const side1 = seed % 2 === 0 ? 1 : -1
    const side2 = (seed >>> 3) % 2 === 0 ? 1 : -1
    return [
      {
        dx: side1 * 0.44,
        dz: -side2 * 0.42,
        w: 0.44 + ((seed >>> 5) % 14) / 100,
        d: 0.42 + ((seed >>> 8) % 14) / 100,
        h: Math.max(0.5, height * (0.34 + ((seed >>> 11) % 16) / 100))
      }
    ]
  }, [view.island.id, item.building.id, height])

  const layers = useMemo<Array<{ w: number; h: number; accent: boolean; rot: number; y: number; dx: number }>>(() => {
    const baseH = 0.12
    const band = 0.06
    const bodyH = Math.max(0.8, height - baseH)
    switch (variant) {
      case 0: {
        const main = bodyH - band * 2
        return [
          { w: width * 1.1, h: main * 0.55, accent: false, rot: 0, y: baseH + main * 0.275, dx: 0 },
          { w: width * 0.9, h: band, accent: true, rot: 0, y: baseH + main * 0.55 + band / 2, dx: 0 },
          { w: width * 0.8, h: main * 0.3, accent: false, rot: 0, y: baseH + main * 0.55 + band + main * 0.15, dx: 0 },
          { w: width * 0.62, h: band, accent: true, rot: 0, y: baseH + main * 0.85 + band / 2, dx: 0 },
          { w: width * 0.52, h: main * 0.15, accent: false, rot: 0, y: baseH + main * 0.85 + band + main * 0.075, dx: 0 }
        ]
      }
      case 1: {
        const podium = bodyH * 0.24
        const tower = bodyH - podium
        return [
          { w: width * 1.24, h: podium, accent: false, rot: 0, y: baseH + podium / 2, dx: 0 },
          { w: width * 0.54, h: tower, accent: false, rot: 0, y: baseH + podium + tower / 2, dx: -width * 0.32 },
          { w: width * 0.5, h: tower * 0.72, accent: false, rot: 0, y: baseH + podium + (tower * 0.72) / 2, dx: width * 0.32 }
        ]
      }
      case 2: {
        const main = bodyH * 0.78
        const spire = bodyH * 0.22
        return [
          { w: width * 1.12, h: main, accent: false, rot: 0, y: baseH + main / 2, dx: 0 },
          { w: width * 0.72, h: main * 0.12, accent: true, rot: 0, y: baseH + main + main * 0.06, dx: 0 },
          { w: width * 0.18, h: spire, accent: false, rot: 0, y: baseH + main + main * 0.12 + spire / 2, dx: 0 }
        ]
      }
      default: {
        const step = bodyH / 3
        return [
          { w: width * 1.22, h: step, accent: false, rot: Math.PI / 4, y: baseH + step / 2, dx: 0 },
          { w: width * 0.92, h: step, accent: false, rot: Math.PI / 4, y: baseH + step * 1.5, dx: 0 },
          { w: width * 0.62, h: step, accent: true, rot: Math.PI / 4, y: baseH + step * 2.5, dx: 0 }
        ]
      }
    }
  }, [height, variant])

  const handlePointerDown = (event: ThreeEvent<PointerEvent>): void => {
    event.stopPropagation()
    onSelectBuilding(view.island.id, item.building.id)
  }
  const lit = selected || hovered
  const facade = skin.facades[variant % skin.facades.length]?.material ?? skin.roof
  const wallWithRoof = useMemo(() => [facade, facade, skin.roof, skin.roof, facade, facade], [facade, skin.roof])

  return (
    <group
      position={[x, 0, z]}
      userData={{ testid: `hive-archipelago-tower-${item.building.id}`, buildingId: item.building.id }}
      onPointerDown={handlePointerDown}
      onPointerMove={(event: ThreeEvent<PointerEvent>) => {
        event.stopPropagation()
        onHoverBuilding(item.building.id)
      }}
      onPointerOut={() => onHoverBuilding(null)}
    >
      {/* Квартальный тротуар-блок с тонкой акцентной окантовкой категории модуля. */}
      <mesh position={[0, 0.05, 0]} material={skin.plinth}>
        <boxGeometry args={[1.56, 0.1, 1.56]} />
      </mesh>
      <mesh position={[0, 0.105, 0]}>
        <boxGeometry args={[1.5, 0.02, 1.5]} />
        <meshBasicMaterial color={kindColor} transparent opacity={lit ? 0.9 : 0.42} />
      </mesh>

      {/* Соседние малоэтажные здания квартала. */}
      {annexes.map((annex, idx) => (
        <mesh
          key={`annex-${idx}`}
          position={[annex.dx, 0.11 + annex.h / 2, annex.dz]}
          material={wallWithRoof}
          scale={[annex.w, annex.h, annex.d]}
        >
          <boxGeometry args={[1, 1, 1]} />
        </mesh>
      ))}

      {layers.map((layer, index) => (
        <mesh
          key={index}
          position={[layer.dx, layer.y, 0]}
          material={layer.accent ? accent : wallWithRoof}
          rotation={[0, layer.rot, 0]}
          scale={[layer.w, layer.h, layer.w]}
        >
          <boxGeometry args={[1, 1, 1]} />
        </mesh>
      ))}

      {/* Крышная тех-надстройка и антенна-маячок. */}
      <mesh position={[0.16, height + 0.08, -0.16]} material={skin.roof}>
        <boxGeometry args={[0.26, 0.16, 0.26]} />
      </mesh>
      <mesh position={[-0.18, height + 0.22, 0.18]}>
        <cylinderGeometry args={[0.02, 0.03, 0.44, 6]} />
        <meshBasicMaterial color={kindColor} />
      </mesh>

      {lit ? (
        <>
          <mesh position={[0, height / 2 + 0.1, 0]}>
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
}

/** Башни одного острова: артикулированные комплексы по детерминированным силуэтам. */
function IslandTowers({
  view,
  skin,
  accent,
  selectedBuildingId,
  hoveredBuildingId,
  onSelectBuilding,
  onHoverBuilding
}: {
  view: CityIslandView
  skin: CitySkin
  accent: THREE.MeshStandardMaterial
  selectedBuildingId: string | null
  hoveredBuildingId: string | null
  onSelectBuilding: (islandId: string, buildingId: string) => void
  onHoverBuilding: (buildingId: string | null) => void
}) {
  const activeId = hoveredBuildingId ?? selectedBuildingId
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
          hovered={activeId === item.building.id}
          onSelectBuilding={onSelectBuilding}
          onHoverBuilding={onHoverBuilding}
        />
      ))}
    </group>
  )
}

/** Табличка названия острова над передним краем плиты. */
function IslandNameplate({ view }: { view: CityIslandView }) {
  const hue = displayIslandHue(view.island.id, view.island.role)
  const accent = `hsl(${hue}, 78%, 64%)`
  const subtitle =
    view.island.status === 'unavailable'
      ? 'Недоступен'
      : `${view.buildings.length} мод. · ${view.island.total_files} ф.`
  return (
    <LabelSprite3D
      text={view.island.name}
      subtitle={subtitle}
      accentColor={accent}
      position={[view.x, 2.8, view.z + view.radius + 2.8]}
      scale={[6.2, 1.62, 1]}
      fontSize={26}
    />
  )
}

/**
 * Двухслойный отрезок дороги на плите острова: тёмное дорожное полотно +
 * светящаяся осевая линия, чтобы дороги чётко читались в изометрии.
 */
function RoadSegment3D({
  from,
  to,
  bedMaterial,
  lineMaterial,
  width = 0.34,
  y = 0.025
}: {
  from: readonly [number, number]
  to: readonly [number, number]
  bedMaterial: THREE.Material
  lineMaterial: THREE.Material
  width?: number
  y?: number
}) {
  const dx = to[0] - from[0]
  const dz = to[1] - from[1]
  const length = Math.hypot(dx, dz)
  if (length < 0.08) return <></>
  const angle = Math.atan2(dz, dx)
  const cx = (from[0] + to[0]) / 2
  const cz = (from[1] + to[1]) / 2
  return (
    <group position={[cx, y, cz]} rotation={[0, -angle, 0]}>
      <mesh material={bedMaterial}>
        <boxGeometry args={[length, 0.02, width]} />
      </mesh>
      <mesh position={[0, 0.015, 0]} material={lineMaterial}>
        <boxGeometry args={[length, 0.015, Math.max(0.05, width * 0.24)]} />
      </mesh>
    </group>
  )
}

/**
 * Городская уличная сеть острова: кольцевые проспекты, радиальные авеню,
 * уличные фонари с тёплыми огнями и зелёные скверы/деревья на бульварах.
 */
function CityStreets3D({ view, skin }: { view: CityIslandView; skin: CitySkin }) {
  const isCapital = view.island.role === 'capital'
  const laneMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#4e6fb8',
        transparent: true,
        opacity: 0.36,
        depthWrite: false
      }),
    []
  )
  const lampPoleMaterial = useMemo(
    () => new THREE.MeshStandardMaterial({ color: '#1b2238', roughness: 0.7, metalness: 0.4 }),
    []
  )
  const lampBulbMaterial = useMemo(
    () => new THREE.MeshBasicMaterial({ color: '#ffdf8e' }),
    []
  )
  const treeTrunkMaterial = useMemo(
    () => new THREE.MeshStandardMaterial({ color: '#3b2e2a', roughness: 0.9 }),
    []
  )
  const treeCrownMaterial = useMemo(
    () =>
      new THREE.MeshStandardMaterial({
        color: '#2a8f7a',
        emissive: '#144d42',
        emissiveIntensity: 0.3,
        roughness: 0.65
      }),
    []
  )
  useEffect(() => () => {
    laneMaterial.dispose()
    lampPoleMaterial.dispose()
    lampBulbMaterial.dispose()
    treeTrunkMaterial.dispose()
    treeCrownMaterial.dispose()
  }, [laneMaterial, lampPoleMaterial, lampBulbMaterial, treeTrunkMaterial, treeCrownMaterial])

  const ringRadii = useMemo(() => {
    const outer = view.radius * 0.78
    if (isCapital) {
      // Кольцевые проспекты проходят строго между кольцами зданий (дома на 4.8, модули на 7.6, 10.8, 14.0).
      return [6.0, 9.2, 12.4, outer].filter((r) => r <= view.radius * 0.82)
    }
    return [view.radius * 0.42, outer].filter((r) => r >= 2.2)
  }, [isCapital, view.radius])

  // Позиции уличных фонарей вдоль кольцевых проспектов и набережной.
  const lampPositions = useMemo(() => {
    const list: Array<readonly [number, number]> = []
    ringRadii.forEach((r, rIdx) => {
      const count = Math.max(8, Math.floor((2 * Math.PI * r) / 4.2))
      for (let i = 0; i < count; i += 1) {
        const angle = (i / count) * Math.PI * 2 + rIdx * 0.25
        const lx = Math.cos(angle) * (r + 0.34)
        const lz = Math.sin(angle) * (r + 0.34)
        const tooClose = view.buildings.some((b) => Math.hypot(b.x - lx, b.z - lz) < 1.25)
        if (!tooClose) list.push([view.x + lx, view.z + lz])
      }
    })
    return list
  }, [ringRadii, view.buildings, view.x, view.z])

  // Позиции деревьев в скверах (на безопасном расстоянии от зданий и дорог).
  const treePositions = useMemo(() => {
    const list: Array<readonly [number, number, number]> = []
    const outerR = view.radius * 0.74
    const candidateCount = isCapital ? 24 : 10
    for (let i = 0; i < candidateCount; i += 1) {
      const seed = fnv1a32(`${view.island.id}/tree-${i}`)
      const angle = ((i / candidateCount) * Math.PI * 2) + ((seed % 100) / 100) * 0.18
      const r = (isCapital ? 6.6 : 1.8) + (((seed >>> 6) % 100) / 100) * Math.max(1.2, outerR - (isCapital ? 6.8 : 2.0))
      const tx = Math.cos(angle) * r
      const tz = Math.sin(angle) * r
      const nearBuilding = view.buildings.some((b) => Math.hypot(b.x - tx, b.z - tz) < 1.85)
      const nearRoad = ringRadii.some((ringR) => Math.abs(r - ringR) < 0.65)
      if (!nearBuilding && !nearRoad) {
        const scale = 0.8 + ((seed >>> 12) % 45) / 100
        list.push([view.x + tx, view.z + tz, scale])
      }
    }
    return list
  }, [isCapital, ringRadii, view])

  const poleRef = useRef<THREE.InstancedMesh>(null)
  const bulbRef = useRef<THREE.InstancedMesh>(null)
  const trunkRef = useRef<THREE.InstancedMesh>(null)
  const crownRef = useRef<THREE.InstancedMesh>(null)

  useLayoutEffect(() => {
    const dummy = FRAME_DUMMY
    if (poleRef.current && bulbRef.current) {
      lampPositions.forEach(([lx, lz], i) => {
        dummy.position.set(lx, 0.24, lz)
        dummy.rotation.set(0, 0, 0)
        dummy.scale.set(1, 1, 1)
        dummy.updateMatrix()
        poleRef.current?.setMatrixAt(i, dummy.matrix)

        dummy.position.set(lx, 0.5, lz)
        dummy.updateMatrix()
        bulbRef.current?.setMatrixAt(i, dummy.matrix)
      })
      poleRef.current.instanceMatrix.needsUpdate = true
      bulbRef.current.instanceMatrix.needsUpdate = true
    }
    if (trunkRef.current && crownRef.current) {
      treePositions.forEach(([tx, tz, sc], i) => {
        dummy.position.set(tx, 0.14 * sc, tz)
        dummy.rotation.set(0, 0, 0)
        dummy.scale.set(sc, sc, sc)
        dummy.updateMatrix()
        trunkRef.current?.setMatrixAt(i, dummy.matrix)

        dummy.position.set(tx, 0.42 * sc, tz)
        dummy.updateMatrix()
        crownRef.current?.setMatrixAt(i, dummy.matrix)
      })
      trunkRef.current.instanceMatrix.needsUpdate = true
      crownRef.current.instanceMatrix.needsUpdate = true
    }
  }, [lampPositions, treePositions])

  return (
    <group>
      {/* Кольцевые проспекты с двумя полосами движения. */}
      {ringRadii.map((r, idx) => (
        <group key={`avenue-ring-${idx}`}>
          <mesh position={[view.x, 0.015, view.z]} rotation={[-Math.PI / 2, 0, 0]} material={skin.roadBed}>
            <ringGeometry args={[r - 0.3, r + 0.3, 64]} />
          </mesh>
          <mesh position={[view.x, 0.024, view.z]} rotation={[Math.PI / 2, 0, 0]} material={laneMaterial}>
            <torusGeometry args={[r, 0.025, 6, 64]} />
          </mesh>
        </group>
      ))}
      {/* Уличные фонари. */}
      {lampPositions.length > 0 ? (
        <>
          <instancedMesh ref={poleRef} args={[undefined, lampPoleMaterial, lampPositions.length]}>
            <cylinderGeometry args={[0.025, 0.035, 0.48, 5]} />
          </instancedMesh>
          <instancedMesh ref={bulbRef} args={[undefined, lampBulbMaterial, lampPositions.length]}>
            <sphereGeometry args={[0.06, 6, 6]} />
          </instancedMesh>
        </>
      ) : null}
      {/* Деревья и скверы. */}
      {treePositions.length > 0 ? (
        <>
          <instancedMesh ref={trunkRef} args={[undefined, treeTrunkMaterial, treePositions.length]}>
            <cylinderGeometry args={[0.04, 0.06, 0.28, 5]} />
          </instancedMesh>
          <instancedMesh ref={crownRef} args={[undefined, treeCrownMaterial, treePositions.length]}>
            <coneGeometry args={[0.24, 0.48, 6]} />
          </instancedMesh>
        </>
      ) : null}
    </group>
  )
}

interface QueenPacket {
  readonly color: string
  readonly speed: number
  readonly phase: number
  readonly lot: number
  readonly kind: 'to-queen' | 'from-queen' | 'ring'
  readonly active: boolean
}

/**
 * Дороги столицы и движение информации от агентов:
 * - радиальные проспекты от двора Королевы к каждому дому агента;
 * - кольцевой проспект, соединяющий дома агентов;
 * - вытянутые светящиеся пакеты (направленные по движению!), бегущие между
 *   агентами и Королевой и по кольцу между департаментами.
 */
function QueenRoads3D({
  view,
  lots,
  skin,
  paused
}: {
  view: CityIslandView
  lots: readonly HiveCityHouseLot[]
  skin: CitySkin
  paused: boolean
}) {
  const ringRadius = Math.min(
    capitalHouseRingRadius(lots[0]?.total ?? 0),
    CAPITAL_TOWER_INNER_RADIUS - 1.2
  )
  const spokeLineMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#f0cf78',
        transparent: true,
        opacity: 0.55,
        depthWrite: false
      }),
    []
  )
  const ringLineMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#88b2ff',
        transparent: true,
        opacity: 0.45,
        depthWrite: false
      }),
    []
  )
  const ringBedMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#1b2747',
        transparent: true,
        opacity: 0.85,
        depthWrite: false
      }),
    []
  )
  const packetMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#ffffff',
        transparent: true,
        opacity: 0.95
      }),
    []
  )
  useEffect(() => () => {
    spokeLineMaterial.dispose()
    ringLineMaterial.dispose()
    ringBedMaterial.dispose()
    packetMaterial.dispose()
  }, [spokeLineMaterial, ringLineMaterial, ringBedMaterial, packetMaterial])

  const packets = useMemo<readonly QueenPacket[]>(() => {
    const list: QueenPacket[] = []
    lots.forEach((lot, lotIndex) => {
      const seed = fnv1a32(`${view.island.id}/road-${lot.departmentKey}-${lot.slot}`)
      const flowSpeed = lot.active ? 0.26 : 0.14
      const toQueenCount = lot.active ? 2 : 1
      for (let i = 0; i < toQueenCount; i += 1) {
        list.push({
          color: lot.accent,
          speed: flowSpeed,
          phase: ((seed % 997) / 997 + i / toQueenCount) % 1,
          lot: lotIndex,
          kind: 'to-queen',
          active: lot.active
        })
      }
      const fromQueenCount = lot.active ? 2 : 1
      for (let i = 0; i < fromQueenCount; i += 1) {
        list.push({
          color: '#ffe494',
          speed: flowSpeed,
          phase: (((seed >>> 3) % 997) / 997 + i / fromQueenCount) % 1,
          lot: lotIndex,
          kind: 'from-queen',
          active: lot.active
        })
      }
      list.push({
        color: lot.accent,
        speed: 0.18,
        phase: lot.slot / Math.max(1, lots.length),
        lot: lotIndex,
        kind: 'ring',
        active: lot.active
      })
    })
    return list
  }, [lots, view.island.id])

  const houseAngles = useMemo(
    () => lots.map((lot) => capitalHouseAngle(lot.slot, lot.total)),
    [lots]
  )

  const meshRef = useRef<THREE.InstancedMesh>(null)
  const instanceColor = useMemo(() => new THREE.Color(), [])
  useLayoutEffect(() => {
    const mesh = meshRef.current
    if (!mesh) return
    packets.forEach((packet, index) => {
      instanceColor.set(packet.color)
      mesh.setColorAt(index, instanceColor)
    })
    if (mesh.instanceColor !== null) mesh.instanceColor.needsUpdate = true
  }, [packets, instanceColor])

  useFrame((state) => {
    const mesh = meshRef.current
    if (!mesh) return
    const time = paused ? 0 : state.clock.elapsedTime
    const dummy = FRAME_DUMMY
    const laneOffset = 0.11
    packets.forEach((packet, index) => {
      if (index >= mesh.count) return
      const angle = houseAngles[packet.lot]
      if (angle === undefined) return
      const progress = paused ? packet.phase : (packet.phase + time * packet.speed) % 1
      let x = view.x
      let z = view.z
      let dirX = 0
      let dirZ = 1

      if (packet.kind === 'ring') {
        const nextAngle =
          (packet.lot + 1 < lots.length ? houseAngles[packet.lot + 1] : (houseAngles[0] ?? angle) + Math.PI * 2) ?? angle
        const target = nextAngle > angle ? nextAngle : nextAngle + Math.PI * 2
        const current = angle + (target - angle) * progress
        const laneR = ringRadius + 0.12
        x = view.x + Math.cos(current) * laneR
        z = view.z + Math.sin(current) * laneR
        dirX = -Math.sin(current)
        dirZ = Math.cos(current)
        dummy.scale.set(0.12, 0.1, packet.active ? 0.32 : 0.22)
      } else {
        const roadStart = 2.4
        const roadEnd = Math.max(roadStart + 0.4, ringRadius - 0.55)
        const outward = packet.kind === 'from-queen' ? progress : 1 - progress
        const radius = roadStart + outward * (roadEnd - roadStart)
        const sign = packet.kind === 'from-queen' ? 1 : -1
        dirX = Math.cos(angle) * sign
        dirZ = Math.sin(angle) * sign
        // Правостороннее движение: вправо от вектора направления (dirZ, -dirX) в плоскости XZ.
        const rightX = -dirZ
        const rightZ = dirX
        x = view.x + Math.cos(angle) * radius + rightX * laneOffset
        z = view.z + Math.sin(angle) * radius + rightZ * laneOffset
        dummy.scale.set(packet.active ? 0.14 : 0.11, 0.1, packet.active ? 0.34 : 0.24)
      }

      dummy.position.set(x, 0.12, z)
      dummy.lookAt(x + dirX, 0.12, z + dirZ)
      dummy.updateMatrix()
      mesh.setMatrixAt(index, dummy.matrix)
    })
    mesh.instanceMatrix.needsUpdate = true
  })

  if (lots.length === 0) return <></>

  return (
    <group>
      {/* Двухполосные радиальные проспекты от двора Королевы до края двориков домов. */}
      {lots.map((lot, index) => {
        const angle = houseAngles[index]
        if (angle === undefined) return null
        const roadEnd = Math.max(2.8, ringRadius - 0.55)
        return (
          <RoadSegment3D
            key={`road-${lot.departmentKey}-${lot.slot}`}
            from={[view.x + Math.cos(angle) * 2.4, view.z + Math.sin(angle) * 2.4]}
            to={[view.x + Math.cos(angle) * roadEnd, view.z + Math.sin(angle) * roadEnd]}
            bedMaterial={skin.roadBed}
            lineMaterial={spokeLineMaterial}
            width={0.46}
            y={0.025}
          />
        )
      })}
      {/* Двухполосный кольцевой бульвар между домами агентов. */}
      <mesh position={[view.x, 0.022, view.z]} rotation={[-Math.PI / 2, 0, 0]} material={ringBedMaterial}>
        <ringGeometry args={[ringRadius - 0.24, ringRadius + 0.24, 64]} />
      </mesh>
      <mesh position={[view.x, 0.036, view.z]} rotation={[Math.PI / 2, 0, 0]} material={ringLineMaterial}>
        <torusGeometry args={[ringRadius, 0.028, 6, 64]} />
      </mesh>
      {/* Вытянутые пакеты информации вдоль лучей и кольца. */}
      <instancedMesh
        ref={meshRef}
        args={[undefined, packetMaterial, Math.max(1, packets.length)]}
        visible={packets.length > 0}
        frustumCulled={false}
      >
        <boxGeometry args={[1, 1, 1]} />
      </instancedMesh>
    </group>
  )
}

/**
 * Дороги от домов активных агентов (или кольца агентов при droneCount > 0)
 * к кварталам модулей города: курьерские пакеты доставляют задачи и забирают
 * результаты прямо по дорогам, а не летают бесцельно в космосе.
 */
function AgentModuleCouriers3D({
  view,
  lots,
  droneCount,
  skin,
  paused
}: {
  view: CityIslandView
  lots: readonly HiveCityHouseLot[]
  droneCount: number
  skin: CitySkin
  paused: boolean
}) {
  const courierLineMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#6ce8c2',
        transparent: true,
        opacity: 0.42,
        depthWrite: false
      }),
    []
  )
  const packetMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#ffffff',
        transparent: true,
        opacity: 0.95
      }),
    []
  )
  useEffect(() => () => {
    courierLineMaterial.dispose()
    packetMaterial.dispose()
  }, [courierLineMaterial, packetMaterial])

  const MAIN_AVENUES_RADIUS = 6.0

  const routes = useMemo(() => {
    const modules = view.buildings.filter((item) => item.building.kind !== 'citadel')
    if (modules.length === 0 || lots.length === 0) return []
    const ringRadius = Math.min(
      capitalHouseRingRadius(lots[0]?.total ?? 0),
      CAPITAL_TOWER_INNER_RADIUS - 1.2
    )
    const activeLots = lots.filter((lot) => lot.active)
    const sourceLots =
      activeLots.length > 0
        ? activeLots
        : lots.slice(0, Math.min(lots.length, Math.max(2, droneCount)))

    const list: Array<{
      readonly houseAngle: number
      readonly houseExitR: number
      readonly targetAngle: number
      readonly targetEntryR: number
      readonly color: string
      readonly phase: number
      readonly active: boolean
    }> = []

    sourceLots.forEach((lot, idx) => {
      const houseAngle = capitalHouseAngle(lot.slot, lot.total)
      const hx = Math.cos(houseAngle) * ringRadius
      const hz = Math.sin(houseAngle) * ringRadius
      const sorted = [...modules].sort((a, b) => {
        const da = Math.hypot(a.x - hx, a.z - hz)
        const db = Math.hypot(b.x - hx, b.z - hz)
        return da - db
      })
      const target = sorted[0]
      if (!target) return
      const targetAngle = Math.atan2(target.z, target.x)
      const targetR = Math.hypot(target.x, target.z)
      list.push({
        houseAngle,
        houseExitR: ringRadius + 0.54,
        targetAngle,
        targetEntryR: Math.max(MAIN_AVENUES_RADIUS + 0.25, targetR - 0.85),
        color: lot.active ? lot.accent : '#7ef2c4',
        phase: idx / Math.max(1, sourceLots.length),
        active: lot.active
      })
    })
    return list.slice(0, 18)
  }, [view, lots, droneCount])

  const meshRef = useRef<THREE.InstancedMesh>(null)
  const instanceColor = useMemo(() => new THREE.Color(), [])
  useLayoutEffect(() => {
    const mesh = meshRef.current
    if (!mesh) return
    routes.forEach((route, index) => {
      // Две машины на маршрут: туда (к модулю) и обратно (к агенту).
      instanceColor.set(route.color)
      mesh.setColorAt(index * 2, instanceColor)
      instanceColor.set('#ffe494')
      mesh.setColorAt(index * 2 + 1, instanceColor)
    })
    if (mesh.instanceColor !== null) mesh.instanceColor.needsUpdate = true
  }, [routes, instanceColor])

  useFrame((state) => {
    const mesh = meshRef.current
    if (!mesh || routes.length === 0) return
    const time = paused ? 0 : state.clock.elapsedTime
    const dummy = FRAME_DUMMY
    const flowSpeed = 0.16
    const laneOffset = 0.11

    routes.forEach((route, index) => {
      // Кратчайший угол по кольцу от дома к модулю.
      let deltaAngle = route.targetAngle - route.houseAngle
      while (deltaAngle > Math.PI) deltaAngle -= Math.PI * 2
      while (deltaAngle < -Math.PI) deltaAngle += Math.PI * 2

      for (let dir = 0; dir < 2; dir += 1) {
        const instIdx = index * 2 + dir
        if (instIdx >= mesh.count) continue
        const p = paused ? (route.phase + dir * 0.5) % 1 : (route.phase + dir * 0.5 + time * flowSpeed) % 1
        const t = dir === 0 ? p : 1 - p

        let x = view.x
        let z = view.z
        let dirX = 1
        let dirZ = 0

        if (t < 0.2) {
          // Участок 1: выезд от дома агента до Главного кольца.
          const u = t / 0.2
          const r = route.houseExitR + u * (MAIN_AVENUES_RADIUS - route.houseExitR)
          const sign = dir === 0 ? 1 : -1
          dirX = Math.cos(route.houseAngle) * sign
          dirZ = Math.sin(route.houseAngle) * sign
          x = view.x + Math.cos(route.houseAngle) * r - dirZ * laneOffset
          z = view.z + Math.sin(route.houseAngle) * r + dirX * laneOffset
        } else if (t <= 0.8) {
          // Участок 2: движение по дуге Главного кольца (внешняя/внутренняя полоса).
          const u = (t - 0.2) / 0.6
          const curAngle = route.houseAngle + deltaAngle * u
          const ringSign = (deltaAngle >= 0 ? 1 : -1) * (dir === 0 ? 1 : -1)
          const laneR = MAIN_AVENUES_RADIUS + (dir === 0 ? 0.13 : -0.13)
          x = view.x + Math.cos(curAngle) * laneR
          z = view.z + Math.sin(curAngle) * laneR
          dirX = -Math.sin(curAngle) * ringSign
          dirZ = Math.cos(curAngle) * ringSign
        } else {
          // Участок 3: заезд от Главного кольца к подиуму модуля.
          const u = (t - 0.8) / 0.2
          const r = MAIN_AVENUES_RADIUS + u * (route.targetEntryR - MAIN_AVENUES_RADIUS)
          const sign = dir === 0 ? 1 : -1
          dirX = Math.cos(route.targetAngle) * sign
          dirZ = Math.sin(route.targetAngle) * sign
          x = view.x + Math.cos(route.targetAngle) * r - dirZ * laneOffset
          z = view.z + Math.sin(route.targetAngle) * r + dirX * laneOffset
        }

        dummy.position.set(x, 0.12, z)
        dummy.lookAt(x + dirX, 0.12, z + dirZ)
        dummy.scale.set(route.active ? 0.14 : 0.11, 0.1, route.active ? 0.34 : 0.24)
        dummy.updateMatrix()
        mesh.setMatrixAt(instIdx, dummy.matrix)
      }
    })
    mesh.instanceMatrix.needsUpdate = true
  })

  if (routes.length === 0) return <></>

  return (
    <group>
      {/* Двухполосные подъездные дороги от домов и от Главного кольца к модулям (не режут здания). */}
      {routes.map((route, index) => (
        <group key={`agent-module-road-${index}`}>
          <RoadSegment3D
            from={[
              view.x + Math.cos(route.houseAngle) * route.houseExitR,
              view.z + Math.sin(route.houseAngle) * route.houseExitR
            ]}
            to={[
              view.x + Math.cos(route.houseAngle) * MAIN_AVENUES_RADIUS,
              view.z + Math.sin(route.houseAngle) * MAIN_AVENUES_RADIUS
            ]}
            bedMaterial={skin.roadBed}
            lineMaterial={courierLineMaterial}
            width={0.42}
            y={0.02}
          />
          <RoadSegment3D
            from={[
              view.x + Math.cos(route.targetAngle) * MAIN_AVENUES_RADIUS,
              view.z + Math.sin(route.targetAngle) * MAIN_AVENUES_RADIUS
            ]}
            to={[
              view.x + Math.cos(route.targetAngle) * route.targetEntryR,
              view.z + Math.sin(route.targetAngle) * route.targetEntryR
            ]}
            bedMaterial={skin.roadBed}
            lineMaterial={courierLineMaterial}
            width={0.42}
            y={0.02}
          />
        </group>
      ))}
      <instancedMesh
        ref={meshRef}
        args={[undefined, packetMaterial, Math.max(1, routes.length * 2)]}
        visible={routes.length > 0}
        frustumCulled={false}
      >
        <boxGeometry args={[1, 1, 1]} />
      </instancedMesh>
    </group>
  )
}

/** Магистрали между зданиями модулей: цветные трассы зависимостей/IPC с направленными пакетами. */
function ModuleRoads3D({
  view,
  skin,
  highlightedBuildingId,
  paused
}: {
  view: CityIslandView
  skin: CitySkin
  highlightedBuildingId: string | null
  paused: boolean
}) {
  const roads = useMemo(() => {
    const byId = new Map(view.buildings.map((item) => [item.building.id, item]))
    const edges = [...view.island.edges]
      .sort((left, right) => right.weight - left.weight)
      .slice(0, 24)
    const isCapital = view.island.role === 'capital'
    const segments: Array<{
      readonly fromId: string
      readonly toId: string
      readonly fromAngle: number
      readonly fromEntryR: number
      readonly toAngle: number
      readonly toEntryR: number
      readonly ringR: number
      readonly deltaAngle: number
      readonly color: string
    }> = []
    for (const edge of edges) {
      const from = byId.get(edge.from_id)
      const to = byId.get(edge.to_id)
      if (from === undefined || to === undefined) continue
      if (from.building.kind === 'citadel' || to.building.kind === 'citadel') continue
      const rFrom = Math.hypot(from.x, from.z)
      const rTo = Math.hypot(to.x, to.z)
      if (rFrom < 0.5 || rTo < 0.5) continue
      const avgR = (rFrom + rTo) / 2
      const ringR = isCapital
        ? avgR < 7.6
          ? 6.0
          : avgR < 10.8
            ? 9.2
            : 12.4
        : Math.max(2.2, view.radius * 0.42)
      const fromAngle = Math.atan2(from.z, from.x)
      const toAngle = Math.atan2(to.z, to.x)
      let deltaAngle = toAngle - fromAngle
      while (deltaAngle > Math.PI) deltaAngle -= Math.PI * 2
      while (deltaAngle < -Math.PI) deltaAngle += Math.PI * 2
      const fromEntryR = rFrom > ringR ? Math.max(ringR + 0.25, rFrom - 0.85) : Math.min(ringR - 0.25, rFrom + 0.85)
      const toEntryR = rTo > ringR ? Math.max(ringR + 0.25, rTo - 0.85) : Math.min(ringR - 0.25, rTo + 0.85)
      segments.push({
        fromId: edge.from_id,
        toId: edge.to_id,
        fromAngle,
        fromEntryR,
        toAngle,
        toEntryR,
        ringR,
        deltaAngle,
        color: edgeKindColor(edge.kind)
      })
    }
    return segments
  }, [view])

  const lineMaterials = useMemo(
    () => ({
      dependency: new THREE.MeshBasicMaterial({ color: '#5ea8ff', transparent: true, opacity: 0.45, depthWrite: false }),
      ipc: new THREE.MeshBasicMaterial({ color: '#f0c45a', transparent: true, opacity: 0.55, depthWrite: false }),
      runtime: new THREE.MeshBasicMaterial({ color: '#a78bfa', transparent: true, opacity: 0.5, depthWrite: false }),
      highlighted: new THREE.MeshBasicMaterial({ color: '#ffe494', transparent: true, opacity: 0.9, depthWrite: false })
    }),
    []
  )
  const pulseMaterial = useMemo(
    () => new THREE.MeshBasicMaterial({ color: '#ffffff', transparent: true, opacity: 0.92 }),
    []
  )
  useEffect(() => () => {
    lineMaterials.dependency.dispose()
    lineMaterials.ipc.dispose()
    lineMaterials.runtime.dispose()
    lineMaterials.highlighted.dispose()
    pulseMaterial.dispose()
  }, [lineMaterials, pulseMaterial])

  const meshRef = useRef<THREE.InstancedMesh>(null)
  const instanceColor = useMemo(() => new THREE.Color(), [])

  useLayoutEffect(() => {
    const mesh = meshRef.current
    if (!mesh) return
    roads.forEach((road, index) => {
      const isLit =
        highlightedBuildingId !== null &&
        (road.fromId === highlightedBuildingId || road.toId === highlightedBuildingId)
      instanceColor.set(isLit ? '#ffe9b0' : road.color)
      mesh.setColorAt(index, instanceColor)
    })
    if (mesh.instanceColor !== null) mesh.instanceColor.needsUpdate = true
  }, [roads, highlightedBuildingId, instanceColor])

  useFrame((state) => {
    const mesh = meshRef.current
    if (!mesh) return
    const time = paused ? 0 : state.clock.elapsedTime
    const dummy = FRAME_DUMMY
    const laneOffset = 0.1
    roads.forEach((road, index) => {
      if (index >= mesh.count) return
      const isLit =
        highlightedBuildingId !== null &&
        (road.fromId === highlightedBuildingId || road.toId === highlightedBuildingId)
      const speed = isLit ? 0.28 : 0.18
      const basePhase = index / Math.max(1, roads.length)
      const t = paused ? basePhase : (basePhase + time * speed) % 1

      let x = view.x
      let z = view.z
      let dirX = 1
      let dirZ = 0

      if (t < 0.2) {
        const u = t / 0.2
        const r = road.fromEntryR + u * (road.ringR - road.fromEntryR)
        const sign = road.ringR >= road.fromEntryR ? 1 : -1
        dirX = Math.cos(road.fromAngle) * sign
        dirZ = Math.sin(road.fromAngle) * sign
        x = view.x + Math.cos(road.fromAngle) * r - dirZ * laneOffset
        z = view.z + Math.sin(road.fromAngle) * r + dirX * laneOffset
      } else if (t <= 0.8) {
        const u = (t - 0.2) / 0.6
        const curAngle = road.fromAngle + road.deltaAngle * u
        const ringSign = road.deltaAngle >= 0 ? 1 : -1
        const laneR = road.ringR + (ringSign > 0 ? 0.13 : -0.13)
        x = view.x + Math.cos(curAngle) * laneR
        z = view.z + Math.sin(curAngle) * laneR
        dirX = -Math.sin(curAngle) * ringSign
        dirZ = Math.cos(curAngle) * ringSign
      } else {
        const u = (t - 0.8) / 0.2
        const r = road.ringR + u * (road.toEntryR - road.ringR)
        const sign = road.toEntryR >= road.ringR ? 1 : -1
        dirX = Math.cos(road.toAngle) * sign
        dirZ = Math.sin(road.toAngle) * sign
        x = view.x + Math.cos(road.toAngle) * r - dirZ * laneOffset
        z = view.z + Math.sin(road.toAngle) * r + dirX * laneOffset
      }

      dummy.position.set(x, 0.12, z)
      dummy.lookAt(x + dirX, 0.12, z + dirZ)
      dummy.scale.set(isLit ? 0.14 : 0.1, 0.09, isLit ? 0.34 : 0.24)
      dummy.updateMatrix()
      mesh.setMatrixAt(index, dummy.matrix)
    })
    mesh.instanceMatrix.needsUpdate = true
  })

  if (roads.length === 0) return <></>

  return (
    <group>
      {roads.map((road, index) => {
        const isLit =
          highlightedBuildingId !== null &&
          (road.fromId === highlightedBuildingId || road.toId === highlightedBuildingId)
        const mat = isLit
          ? lineMaterials.highlighted
          : road.color === '#f0c45a'
            ? lineMaterials.ipc
            : road.color === '#a78bfa'
              ? lineMaterials.runtime
              : lineMaterials.dependency
        return (
          <group key={`module-road-${index}`}>
            <RoadSegment3D
              from={[view.x + Math.cos(road.fromAngle) * road.fromEntryR, view.z + Math.sin(road.fromAngle) * road.fromEntryR]}
              to={[view.x + Math.cos(road.fromAngle) * road.ringR, view.z + Math.sin(road.fromAngle) * road.ringR]}
              bedMaterial={skin.roadBed}
              lineMaterial={mat}
              width={0.36}
              y={0.018}
            />
            <RoadSegment3D
              from={[view.x + Math.cos(road.toAngle) * road.ringR, view.z + Math.sin(road.toAngle) * road.ringR]}
              to={[view.x + Math.cos(road.toAngle) * road.toEntryR, view.z + Math.sin(road.toAngle) * road.toEntryR]}
              bedMaterial={skin.roadBed}
              lineMaterial={mat}
              width={0.36}
              y={0.018}
            />
          </group>
        )
      })}
      <instancedMesh
        ref={meshRef}
        args={[undefined, pulseMaterial, Math.max(1, roads.length)]}
        visible={roads.length > 0}
        frustumCulled={false}
      >
        <boxGeometry args={[1, 1, 1]} />
      </instancedMesh>
    </group>
  )
}

/** Один дом работника департамента на кольце вокруг Цитадели Королевы. */
function DepartmentHouse3D({
  view,
  lot,
  houseMaterials,
  highlighted,
  paused,
  onSelectDepartment
}: {
  view: CityIslandView
  lot: HiveCityHouseLot
  houseMaterials: {
    readonly activeBody: THREE.MeshStandardMaterial
    readonly activeRoof: THREE.MeshStandardMaterial
    readonly idleBody: THREE.MeshStandardMaterial
    readonly idleRoof: THREE.MeshStandardMaterial
  }
  highlighted: boolean
  paused: boolean
  onSelectDepartment: ((departmentKey: string) => void) | undefined
}) {
  const angle = capitalHouseAngle(lot.slot, lot.total)
  const ringRadius = Math.min(capitalHouseRingRadius(lot.total), CAPITAL_TOWER_INNER_RADIUS - 1.2)
  const position = useMemo<[number, number, number]>(() => {
    const x = view.x + Math.cos(angle) * ringRadius
    const z = view.z + Math.sin(angle) * ringRadius
    return [x, 0, z]
  }, [angle, ringRadius, view.x, view.z])

  const body = lot.active ? houseMaterials.activeBody : houseMaterials.idleBody
  const roof = lot.active ? houseMaterials.activeRoof : houseMaterials.idleRoof
  const beaconRef = useRef<THREE.Mesh>(null)

  useFrame((state) => {
    if (!lot.active || !beaconRef.current) return
    const t = paused ? 0 : state.clock.elapsedTime
    beaconRef.current.position.y = 1.65 + Math.sin(t * 2.4 + lot.slot) * 0.08
    beaconRef.current.rotation.y = t * 1.4
  })

  return (
    <group
      position={position}
      userData={{
        testid: `hive-archipelago-house-${lot.departmentKey}-${lot.slot}`,
        dept: lot.departmentKey,
        active: lot.active
      }}
      onPointerDown={(event: ThreeEvent<PointerEvent>) => {
        if (onSelectDepartment === undefined) return
        event.stopPropagation()
        onSelectDepartment(lot.departmentKey)
      }}
    >
      {/* Аккуратный тёмный дворик с тонкой цветной каймой департамента. */}
      <mesh position={[0, 0.03, 0]}>
        <boxGeometry args={[1.02, 0.06, 1.02]} />
        <meshBasicMaterial color="#182038" />
      </mesh>
      <mesh position={[0, 0.065, 0]}>
        <boxGeometry args={[0.98, 0.015, 0.98]} />
        <meshBasicMaterial color={lot.accent} transparent opacity={lot.active ? 0.72 : 0.3} />
      </mesh>
      {/* Стены дома ровно на фундаменте. */}
      <mesh position={[0, 0.36, 0]} material={body}>
        <boxGeometry args={[0.78, 0.58, 0.78]} />
      </mesh>
      {/* Акцентный пояс департамента под крышей. */}
      <mesh position={[0, 0.67, 0]}>
        <boxGeometry args={[0.84, 0.06, 0.84]} />
        <meshBasicMaterial color={lot.accent} />
      </mesh>
      {/* Двускатная/пирамидальная крыша. */}
      <mesh material={roof} position={[0, 0.95, 0]} rotation={[0, Math.PI / 4, 0]}>
        <coneGeometry args={[0.68, 0.48, 4]} />
      </mesh>
      {/* Труба. */}
      <mesh position={[0.2, 0.96, -0.2]}>
        <boxGeometry args={[0.12, 0.28, 0.12]} />
        <meshStandardMaterial color="#181c2c" roughness={0.9} />
      </mesh>
      {/* Окна на обеих видимых в изометрии гранях (+Z и +X). */}
      <mesh position={[0, 0.36, 0.4]}>
        <boxGeometry args={[0.32, 0.2, 0.02]} />
        <meshBasicMaterial color={lot.active ? '#ffe18c' : '#3a4668'} />
      </mesh>
      <mesh position={[0.4, 0.36, 0]}>
        <boxGeometry args={[0.02, 0.2, 0.32]} />
        <meshBasicMaterial color={lot.active ? '#ffe18c' : '#3a4668'} />
      </mesh>
      {/* Парящий индикатор активности над домом работающего агента. */}
      {lot.active ? (
        <mesh ref={beaconRef} position={[0, 1.38, 0]}>
          <octahedronGeometry args={[0.13, 0]} />
          <meshBasicMaterial color={lot.accent} />
        </mesh>
      ) : null}
      {highlighted ? (
        <mesh position={[0, 0.55, 0]}>
          <boxGeometry args={[1.1, 1.05, 1.1]} />
          <meshBasicMaterial color="#ffe9b0" wireframe transparent opacity={0.75} />
        </mesh>
      ) : null}
    </group>
  )
}

/** Маяк спутника: мягкое дышащее гало над центром острова без цитадели. */
function BeaconGlow3D({ view, paused }: { view: CityIslandView; paused: boolean }) {
  const coreRef = useRef<THREE.Mesh>(null)
  const haloRef = useRef<THREE.Mesh>(null)
  const hue = displayIslandHue(view.island.id, view.island.role)
  const color = `hsl(${hue}, 82%, 66%)`

  useFrame((state) => {
    const t = paused ? 0 : state.clock.elapsedTime
    if (coreRef.current) coreRef.current.scale.setScalar(0.38 + 0.06 * Math.sin(t * 1.9))
    if (haloRef.current) haloRef.current.scale.setScalar(1 + 0.08 * Math.sin(t * 1.2))
  })
  return (
    <group position={[view.x, 1.8, view.z]}>
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
}

/** Starfield: static deterministic points on a far shell; no motion cost. */
function Starfield(): React.JSX.Element | null {
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
    <points geometry={geometry}>
      <pointsMaterial color="#9fb4ff" size={1.4} sizeAttenuation={false} transparent opacity={0.6} />
    </points>
  )
}

/** Дома работников департаментов и подписи их секторов на кольце столицы. */
function DepartmentHouses3D({
  view,
  lots,
  selectedDepartment,
  paused,
  onSelectDepartment
}: {
  view: CityIslandView
  lots: readonly HiveCityHouseLot[]
  selectedDepartment: string | null
  paused: boolean
  onSelectDepartment: ((departmentKey: string) => void) | undefined
}) {
  const houseMaterials = useMemo(
    () => ({
      activeBody: new THREE.MeshStandardMaterial({
        color: new THREE.Color('#445075'),
        roughness: 0.55,
        metalness: 0.14
      }),
      activeRoof: new THREE.MeshStandardMaterial({
        color: new THREE.Color('#ebc052'),
        emissive: new THREE.Color('#d89b2a'),
        emissiveIntensity: 0.65,
        roughness: 0.35
      }),
      idleBody: new THREE.MeshStandardMaterial({
        color: new THREE.Color('#2a324d'),
        roughness: 0.7
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
      houseMaterials.activeBody.dispose()
      houseMaterials.activeRoof.dispose()
      houseMaterials.idleBody.dispose()
      houseMaterials.idleRoof.dispose()
    },
    [houseMaterials]
  )

  // Одна аккуратная подпись на каждый департамент (над центральным домом его сектора).
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
    const ringRadius = Math.min(
      capitalHouseRingRadius(lots[0]?.total ?? 0),
      CAPITAL_TOWER_INNER_RADIUS - 1.2
    )
    const total = Math.max(1, lots[0]?.total ?? lots.length)
    return Array.from(groups.entries()).map(([key, info], index) => {
      const midSlot = info.slots[Math.floor(info.slots.length / 2)] ?? 0
      const angle = capitalHouseAngle(midSlot, total)
      return {
        key,
        label: info.label,
        accent: info.accent,
        x: view.x + Math.cos(angle) * (ringRadius + 0.65),
        y: index % 2 === 0 ? 1.85 : 2.45,
        z: view.z + Math.sin(angle) * (ringRadius + 0.65)
      }
    })
  }, [lots, view.x, view.z])

  if (lots.length === 0) return <></>

  return (
    <group>
      {lots.map((lot) => (
        <DepartmentHouse3D
          key={`${lot.departmentKey}-${lot.slot}`}
          view={view}
          lot={lot}
          houseMaterials={houseMaterials}
          highlighted={selectedDepartment === lot.departmentKey}
          paused={paused}
          onSelectDepartment={onSelectDepartment}
        />
      ))}
      {sectorLabels.map((sector) => (
        <LabelSprite3D
          key={`dept-label-${sector.key}`}
          text={sector.label}
          accentColor={sector.accent}
          position={[sector.x, sector.y, sector.z]}
          scale={[2.25, 0.54, 1]}
          fontSize={22}
        />
      ))}
    </group>
  )
}

/**
 * Шпиль Королевы: восьмигранные ярусы со светящимися фасадами, золотые пояса,
 * конус-корона, кристалл-маяк с пульсом и вертикальный луч света.
 */
function QueenTower3D({ view, skin, paused }: { view: CityIslandView; skin: CitySkin; paused: boolean }) {
  const beaconRef = useRef<THREE.Mesh>(null)
  const haloRef = useRef<THREE.Mesh>(null)
  const beamRef = useRef<THREE.Mesh>(null)
  const facade = skin.facades[0]?.material ?? skin.roof
  const tierMaterials = useMemo<[THREE.Material, THREE.Material, THREE.Material]>(
    () => [facade, skin.roof, skin.roof],
    [facade, skin.roof]
  )

  const plan = useMemo(() => {
    const citadel = view.buildings.find((item) => item.building.kind === 'citadel')
    return { x: view.x + (citadel?.x ?? 0), z: view.z + (citadel?.z ?? 0) }
  }, [view])

  useFrame((state) => {
    const t = paused ? 0 : state.clock.elapsedTime
    if (beaconRef.current) {
      beaconRef.current.scale.setScalar(1 + 0.14 * Math.sin(t * 2.1))
    }
    if (haloRef.current) {
      haloRef.current.scale.setScalar(1 + 0.08 * Math.sin(t * 1.3))
    }
    if (beamRef.current) {
      const material = beamRef.current.material as THREE.MeshBasicMaterial
      material.opacity = 0.12 + 0.05 * Math.sin(t * 1.7)
    }
  })

  return (
    <group position={[plan.x, 0, plan.z]} scale={[1, 0.78, 1]}>
      <mesh position={[0, 0.25, 0]} material={skin.plinth}>
        <cylinderGeometry args={[2.3, 2.45, 0.5, 6]} />
      </mesh>
      <mesh position={[0, 0.53, 0]} rotation={[Math.PI / 2, 0, 0]} material={skin.gold}>
        <torusGeometry args={[1.64, 0.055, 8, 32]} />
      </mesh>
      <mesh position={[0, 1.6, 0]} rotation={[0, Math.PI / 8, 0]} material={tierMaterials}>
        <cylinderGeometry args={[1.35, 1.62, 2.2, 8]} />
      </mesh>
      <mesh position={[0, 2.72, 0]} rotation={[Math.PI / 2, 0, 0]} material={skin.gold}>
        <torusGeometry args={[1.28, 0.045, 8, 32]} />
      </mesh>
      <mesh position={[0, 3.62, 0]} rotation={[0, Math.PI / 8, 0]} material={tierMaterials}>
        <cylinderGeometry args={[1.05, 1.22, 1.8, 8]} />
      </mesh>
      <mesh position={[0, 4.54, 0]} rotation={[Math.PI / 2, 0, 0]} material={skin.gold}>
        <torusGeometry args={[1.0, 0.04, 8, 32]} />
      </mesh>
      <mesh position={[0, 5.28, 0]} rotation={[0, Math.PI / 8, 0]} material={tierMaterials}>
        <cylinderGeometry args={[0.8, 0.98, 1.5, 8]} />
      </mesh>
      <mesh position={[0, 6.08, 0]} material={skin.gold}>
        <coneGeometry args={[0.72, 1.1, 8]} />
      </mesh>
      <mesh ref={beaconRef} position={[0, 6.95, 0]}>
        <icosahedronGeometry args={[0.32, 0]} />
        <meshBasicMaterial color="#ffe9b0" />
      </mesh>
      <mesh ref={haloRef} position={[0, 6.95, 0]}>
        <sphereGeometry args={[0.74, 14, 14]} />
        <meshBasicMaterial color="#ffe9b0" transparent opacity={0.18} depthWrite={false} />
      </mesh>
      <mesh ref={beamRef} position={[0, 9.4, 0]}>
        <cylinderGeometry args={[0.09, 0.14, 4.6, 6, 1, true]} />
        <meshBasicMaterial color="#ffd98a" transparent opacity={0.14} blending={THREE.AdditiveBlending} depthWrite={false} />
      </mesh>
    </group>
  )
}

/** Fixed-angle rig: pans the look target, lerps ortho zoom and switches iso/top-down. */
function CameraRig({
  focus,
  overviewZoom,
  topView,
  paused
}: {
  focus: { readonly x: number; readonly z: number } | null
  overviewZoom: number
  topView: boolean
  paused: boolean
}) {
  const { camera, gl } = useThree()
  const look = useRef(new THREE.Vector3(0, 0, 0))
  const desired = useRef(new THREE.Vector3(0, 0, 0))
  const currentOffset = useRef(CAMERA_OFFSET.clone())
  const desiredZoom = useRef(overviewZoom)

  useEffect(() => {
    const dom = gl.domElement
    const onWheel = (event: WheelEvent): void => {
      event.preventDefault()
      const scale = event.deltaY < 0 ? 1.12 : 1 / 1.12
      desiredZoom.current = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, desiredZoom.current * scale))
    }
    dom.addEventListener('wheel', onWheel, { passive: false })
    return () => dom.removeEventListener('wheel', onWheel)
  }, [gl])

  useEffect(() => {
    if (focus) {
      desired.current.set(focus.x, 0, focus.z)
      desiredZoom.current = FOCUS_ZOOM
    } else {
      desired.current.set(0, 0, 0)
      desiredZoom.current = overviewZoom
    }
  }, [focus, overviewZoom])

  useFrame((_, delta) => {
    const factor = paused ? 1 : 1 - Math.exp(-7 * Math.min(delta, 0.1))
    look.current.lerp(desired.current, factor)
    currentOffset.current.lerp(topView ? TOP_CAMERA_OFFSET : CAMERA_OFFSET, factor)
    if (topView && Math.abs(currentOffset.current.x) < 0.25) {
      currentOffset.current.x = 0
    }
    camera.position.copy(look.current).add(currentOffset.current)
    camera.lookAt(look.current)
    const ortho = camera as THREE.OrthographicCamera
    if (typeof ortho.zoom === 'number') {
      ortho.zoom = ortho.zoom + (desiredZoom.current - ortho.zoom) * factor
      ortho.updateProjectionMatrix()
    }
  })

  return null
}

function scaleForPhase(phase: string): number {
  if (phase === 'exiting') return 0.02
  if (phase === 'entering') return 0.04
  return 1
}

function IslandGroup3D({
  view,
  paused,
  selectedIsland,
  selectedBuildingId,
  hoveredBuildingId,
  droneCount,
  departmentHouses,
  selectedDepartment,
  onSelectDepartment,
  onSelectIsland,
  onSelectBuilding,
  onHoverBuilding
}: {
  view: CityIslandView
  paused: boolean
  selectedIsland: boolean
  selectedBuildingId: string | null
  hoveredBuildingId: string | null
  droneCount?: number
  departmentHouses?: readonly HiveCityHouseLot[]
  selectedDepartment?: string | null
  onSelectDepartment?: ((departmentKey: string) => void) | undefined
  onSelectIsland: (islandId: string) => void
  onSelectBuilding: (islandId: string, buildingId: string) => void
  onHoverBuilding: (buildingId: string | null) => void
}) {
  const groupRef = useRef<THREE.Group>(null)
  const skin = useCitySkin()
  const accent = useIslandAccent(displayIslandHue(view.island.id, view.island.role))

  const targetScale = scaleForPhase(view.phase)
  useFrame((_, delta) => {
    const group = groupRef.current
    if (!group) return
    const factor = paused ? 1 : 1 - Math.exp(-6 * Math.min(delta, 0.1))
    const current = group.scale.x
    const next = current + (targetScale - current) * factor
    group.scale.setScalar(Math.max(0.02, next))
    group.visible = next > 0.03
  })

  useLayoutEffect(() => {
    const group = groupRef.current
    if (!group) return
    group.scale.setScalar(Math.max(0.02, Math.min(group.scale.x, 1)))
    group.visible = true
  }, [view.island.id])

  const highlightedBuilding =
    view.island.status === 'ready' ? (hoveredBuildingId ?? (selectedIsland ? selectedBuildingId : null)) : null

  return (
    <group ref={groupRef}>
      <IslandGround view={view} selected={selectedIsland} onSelect={() => onSelectIsland(view.island.id)} />
      {/* Уличная сеть города: проспекты, фонари, скверы. */}
      <CityStreets3D view={view} skin={skin} />
      {/* Дороги под застройкой: связи модулей, двор Королевы и курьеры от агентов к модулям. */}
      <ModuleRoads3D
        view={view}
        skin={skin}
        highlightedBuildingId={highlightedBuilding}
        paused={paused}
      />
      {departmentHouses !== undefined && departmentHouses.length > 0 ? (
        <>
          <QueenRoads3D view={view} lots={departmentHouses} skin={skin} paused={paused} />
          <AgentModuleCouriers3D
            view={view}
            lots={departmentHouses}
            droneCount={droneCount ?? 0}
            skin={skin}
            paused={paused}
          />
        </>
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
        <QueenTower3D view={view} skin={skin} paused={paused} />
      ) : (
        <BeaconGlow3D view={view} paused={paused} />
      )}
      {departmentHouses !== undefined && departmentHouses.length > 0 ? (
        <DepartmentHouses3D
          view={view}
          lots={departmentHouses}
          selectedDepartment={selectedDepartment ?? null}
          paused={paused}
          onSelectDepartment={onSelectDepartment}
        />
      ) : null}
      <IslandNameplate view={view} />
    </group>
  )
}

/**
 * Полный 3D-режим. Родитель оборачивает его в ErrorBoundary: при любой
 * ошибке контекста рендер переключается на SVG-режим без перезагрузки окна.
 */
export function HiveArchipelago3D({
  capital,
  satellites,
  paused,
  selectedIslandId,
  selectedBuildingId,
  hoveredBuildingId,
  droneCount,
  departmentHouses,
  selectedDepartment,
  onSelectDepartment,
  onSelectIsland,
  onSelectBuilding,
  onHoverBuilding,
  onBackgroundClick,
  onContextLost
}: HiveArchipelago3DProps): React.JSX.Element {
  const [topView, setTopView] = useState(false)
  const visibleSatellites = satellites.slice(0, CITY_MAP_MAX_SATELLITES)
  const focus = useMemo(() => {
    if (selectedIslandId === capital.island.id) return { x: 0, z: 0 }
    const selected = visibleSatellites.find((view) => view.island.id === selectedIslandId)
    return selected ? { x: selected.x, z: selected.z } : null
  }, [capital.island.id, selectedIslandId, visibleSatellites])

  const overviewZoom = visibleSatellites.length > 0 ? OVERVIEW_ZOOM_ARCHIPELAGO : OVERVIEW_ZOOM_SOLO

  const handleContextLost = useMemo(() => {
    return (event: Event): void => {
      event.preventDefault()
      onContextLost()
    }
  }, [onContextLost])

  return (
    <div
      className="hive-archipelago hive-archipelago--3d"
      data-testid="hive-archipelago-3d"
    >
      <Canvas
        orthographic
        camera={{
          position: CAMERA_OFFSET.toArray(),
          zoom: overviewZoom,
          near: 0.1,
          far: 600
        }}
        dpr={[1, 2]}
        gl={{ antialias: true, powerPreference: 'low-power' }}
        style={{ width: '100%', height: '100%' }}
        onCreated={({ gl }) => {
          gl.domElement.addEventListener('webglcontextlost', handleContextLost, false)
        }}
        onPointerMissed={(event: unknown) => {
          void event
          onBackgroundClick()
        }}
      >
        <color attach="background" args={['#080b16']} />
        <ambientLight intensity={0.58} />
        <hemisphereLight args={['#7c96e8', '#14192e', 0.85]} />
        <directionalLight position={[26, 40, 18]} intensity={1.25} color="#eef3ff" />
        <directionalLight position={[-22, 18, -20]} intensity={0.45} color="#78a2ff" />
        <pointLight position={[0, 12, 0]} intensity={55} distance={60} decay={1.5} color="#ffd98a" />
        <CameraRig focus={focus} overviewZoom={overviewZoom} topView={topView} paused={paused} />
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
            departmentHouses={departmentHouses ?? []}
            selectedDepartment={selectedDepartment ?? null}
            onSelectDepartment={onSelectDepartment}
            onSelectIsland={onSelectIsland}
            onSelectBuilding={onSelectBuilding}
            onHoverBuilding={onHoverBuilding}
          />
          <ConnectingBridges satellites={visibleSatellites} paused={paused} />
        </CitySkinProvider>
      </Canvas>
      <button
        type="button"
        data-testid="hive-archipelago-view-toggle"
        aria-pressed={topView}
        onClick={() => setTopView((value) => !value)}
        style={{
          position: 'absolute',
          right: '16px',
          bottom: '16px',
          zIndex: 6,
          cursor: 'pointer',
          padding: '6px 12px',
          borderRadius: '999px',
          border: '1px solid rgba(141, 130, 232, 0.42)',
          background: topView ? 'rgba(141, 130, 232, 0.28)' : 'rgba(10, 10, 20, 0.72)',
          color: '#e6e4f2',
          fontSize: '12px',
          backdropFilter: 'blur(8px)'
        }}
      >
        {topView ? 'Изометрия' : 'Вид сверху'}
      </button>
    </div>
  )
}

function FloatingCityGroup({
  capital,
  satellites,
  paused,
  selectedIslandId,
  selectedBuildingId,
  hoveredBuildingId,
  droneCount,
  departmentHouses,
  selectedDepartment,
  onSelectDepartment,
  onSelectIsland,
  onSelectBuilding,
  onHoverBuilding
}: {
  readonly capital: CityIslandView
  readonly satellites: readonly CityIslandView[]
  readonly paused: boolean
  readonly selectedIslandId: string | null
  readonly selectedBuildingId: string | null
  readonly hoveredBuildingId: string | null
  readonly droneCount: number
  readonly departmentHouses: readonly HiveCityHouseLot[]
  readonly selectedDepartment: string | null
  readonly onSelectDepartment: ((departmentKey: string) => void) | undefined
  readonly onSelectIsland: (islandId: string) => void
  readonly onSelectBuilding: (islandId: string, buildingId: string) => void
  readonly onHoverBuilding: (buildingId: string | null) => void
}): React.JSX.Element {
  return (
    <group>
      <IslandGroup3D
        view={capital}
        paused={paused}
        selectedIsland={selectedIslandId === capital.island.id}
        selectedBuildingId={selectedBuildingId}
        hoveredBuildingId={hoveredBuildingId}
        droneCount={droneCount}
        departmentHouses={departmentHouses}
        selectedDepartment={selectedDepartment}
        onSelectDepartment={onSelectDepartment}
        onSelectIsland={onSelectIsland}
        onSelectBuilding={onSelectBuilding}
        onHoverBuilding={onHoverBuilding}
      />
      {satellites.map((view) => (
        <IslandGroup3D
          key={view.island.id}
          view={view}
          paused={paused}
          selectedIsland={selectedIslandId === view.island.id}
          selectedBuildingId={selectedBuildingId}
          hoveredBuildingId={hoveredBuildingId}
          onSelectIsland={onSelectIsland}
          onSelectBuilding={onSelectBuilding}
          onHoverBuilding={onHoverBuilding}
        />
      ))}
    </group>
  )
}
