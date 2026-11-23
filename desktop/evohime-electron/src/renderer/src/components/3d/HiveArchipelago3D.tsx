import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
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
  type CityIslandView
} from '@shared/hive-city-map'
import type { HiveCityHouseLot } from './HiveCityScene'

/**
 * WebGL-архипелаг на `three` + `@react-three/fiber` (план 199.3).
 *
 * Камера фиксированная — строго вид сверху (ортокамера): фокус — это
 * плавный перенос цели взгляда и зум ортокамеры. Все раскладки приходят из
 * общего детерминированного билдера, как и в SVG-режиме. Геометрии,
 * материалы и CanvasTexture явно освобождаются (`dispose`) при размонтировании.
 */

/** Top-down camera offset (tiny Z offset keeps X/Z axes strictly aligned so top/bottom hex edges stay horizontal). */
const TOP_CAMERA_OFFSET = new THREE.Vector3(0, 56, 0.01)

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

export interface HiveArchipelago3DProps {
  readonly capital: CityIslandView
  readonly satellites: readonly CityIslandView[]
  readonly paused: boolean
  readonly selectedIslandId: string | null
  readonly selectedBuildingId: string | null
  readonly hoveredBuildingId: string | null
  readonly droneCount: number
  /** Активна ли Королева (при false в городе нет трафика и активных агентов). */
  readonly queenActive?: boolean | undefined
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

const TRAFFIC_CYCLE_SEC = 6.0

function isRadialTrafficGreen(time: number): boolean {
  return (time % TRAFFIC_CYCLE_SEC) < TRAFFIC_CYCLE_SEC * 0.5
}

/** Shared per-canvas materials of the whole archipelago. */
interface CitySkin {
  readonly facades: readonly FacadeSet[]
  readonly roof: THREE.MeshStandardMaterial
  readonly plinth: THREE.MeshStandardMaterial
  readonly gold: THREE.MeshStandardMaterial
  readonly roadBed: THREE.MeshBasicMaterial
  readonly roadCurb: THREE.MeshBasicMaterial
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
      color: '#0b101e',
      transparent: true,
      opacity: 0.94
    }),
    roadCurb: new THREE.MeshBasicMaterial({
      color: '#6b91dc',
      transparent: true,
      opacity: 0.75,
      depthWrite: false
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
    skin.roadCurb.dispose()
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
 * Плоская надпись на поверхности земли/сектора (лежит в плоскости XZ,
 * ориентирована вдоль сектора и никогда не перекрывает дома и дороги сверху).
 */
function GroundLabel3D({
  text,
  subtitle,
  accentColor,
  position,
  size,
  rotationZ = 0,
  fontSize = 26
}: {
  text: string
  subtitle?: string | undefined
  accentColor: string
  position: readonly [number, number, number]
  size: readonly [number, number]
  rotationZ?: number
  fontSize?: number
}) {
  const texture = useMemo(() => {
    const canvas = document.createElement('canvas')
    canvas.width = 512
    canvas.height = subtitle ? 128 : 96
    const context = canvas.getContext('2d')
    if (context) {
      const radius = 18
      context.fillStyle = 'rgba(10, 14, 28, 0.78)'
      if (typeof context.roundRect === 'function') {
        context.beginPath()
        context.roundRect(4, 4, canvas.width - 8, canvas.height - 8, radius)
        context.fill()
        context.strokeStyle = accentColor
        context.lineWidth = 3
        context.stroke()
      } else {
        context.fillRect(4, 4, canvas.width - 8, canvas.height - 8)
      }

      context.fillStyle = '#f2f6ff'
      context.textAlign = 'center'
      context.textBaseline = 'middle'
      let px = fontSize
      context.font = `700 ${px}px system-ui, -apple-system, sans-serif`
      const maxWidth = canvas.width - 40
      while (px > 16 && context.measureText(text).width > maxWidth) {
        px -= 2
        context.font = `700 ${px}px system-ui, -apple-system, sans-serif`
      }
      const titleY = subtitle ? canvas.height * 0.36 : canvas.height * 0.52
      context.fillText(text, canvas.width / 2, titleY, maxWidth)

      if (subtitle) {
        context.fillStyle = '#a7bce8'
        context.font = '500 22px system-ui, -apple-system, sans-serif'
        context.fillText(subtitle, canvas.width / 2, canvas.height * 0.73, maxWidth)
      }
    }
    const created = new THREE.CanvasTexture(canvas)
    created.colorSpace = THREE.SRGBColorSpace
    return created
  }, [text, subtitle, accentColor, fontSize])

  useEffect(() => () => texture.dispose(), [texture])

  return (
    <mesh position={[position[0], position[1], position[2]]} rotation={[-Math.PI / 2, 0, rotationZ]}>
      <planeGeometry args={[size[0], size[1]]} />
      <meshBasicMaterial map={texture} transparent depthWrite={false} />
    </mesh>
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
          { w: width * 1.16, h: step, accent: false, rot: 0, y: baseH + step / 2, dx: 0 },
          { w: width * 0.88, h: step, accent: false, rot: 0, y: baseH + step * 1.5, dx: 0 },
          { w: width * 0.58, h: step, accent: true, rot: 0, y: baseH + step * 2.5, dx: 0 }
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

  const radialRotY =
    view.island.role === 'capital' && (item.x !== 0 || item.z !== 0)
      ? -Math.atan2(item.z, item.x)
      : 0

  return (
    <group
      position={[x, 0, z]}
      rotation={[0, radialRotY, 0]}
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

/** Плоская табличка названия острова у передней грани плиты. */
function IslandNameplate({ view }: { view: CityIslandView }) {
  const hue = displayIslandHue(view.island.id, view.island.role)
  const accent = `hsl(${hue}, 78%, 64%)`
  const subtitle =
    view.island.status === 'unavailable'
      ? 'Недоступен'
      : `${view.buildings.length} мод. · ${view.island.total_files} ф.`
  return (
    <GroundLabel3D
      text={view.island.name}
      subtitle={subtitle}
      accentColor={accent}
      position={[view.x, 0.025, view.z + view.radius + 0.95]}
      size={[4.8, 1.2]}
      fontSize={34}
    />
  )
}

/**
 * Двухполосный отрезок дороги: контрастный тёмный асфальт, светлые бордюры по
 * обоим краям и яркая осевая разделительная линия посередине.
 */
function RoadSegment3D({
  from,
  to,
  bedMaterial,
  curbMaterial,
  lineMaterial,
  width = 0.52,
  y = 0.025
}: {
  from: readonly [number, number]
  to: readonly [number, number]
  bedMaterial: THREE.Material
  curbMaterial?: THREE.Material
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
  const curbMat = curbMaterial ?? lineMaterial
  return (
    <group position={[cx, y, cz]} rotation={[0, -angle, 0]}>
      <mesh material={bedMaterial}>
        <boxGeometry args={[length, 0.02, width]} />
      </mesh>
      {/* Левый и правый бордюры для чёткой читаемости двух полос. */}
      <mesh position={[0, 0.014, -width * 0.46]} material={curbMat}>
        <boxGeometry args={[length, 0.014, 0.032]} />
      </mesh>
      <mesh position={[0, 0.014, width * 0.46]} material={curbMat}>
        <boxGeometry args={[length, 0.014, 0.032]} />
      </mesh>
      {/* Осевая разделительная полоса. */}
      <mesh position={[0, 0.016, 0]} material={lineMaterial}>
        <boxGeometry args={[length, 0.015, 0.045]} />
      </mesh>
    </group>
  )
}

/**
 * Городская уличная сеть острова: кольцевые проспекты, радиальные авеню,
 * уличные фонари с тёплыми огнями и зелёные скверы/деревья на бульварах.
 */
function CityStreets3D({ view, skin, paused }: { view: CityIslandView; skin: CitySkin; paused: boolean }) {
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
  const greenLightMaterial = useMemo(() => new THREE.MeshBasicMaterial({ color: '#4ade80' }), [])
  const redLightMaterial = useMemo(() => new THREE.MeshBasicMaterial({ color: '#f87171' }), [])
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
    greenLightMaterial.dispose()
    redLightMaterial.dispose()
    lampPoleMaterial.dispose()
    lampBulbMaterial.dispose()
    treeTrunkMaterial.dispose()
    treeCrownMaterial.dispose()
  }, [laneMaterial, greenLightMaterial, redLightMaterial, lampPoleMaterial, lampBulbMaterial, treeTrunkMaterial, treeCrownMaterial])

  const ringRadii = useMemo(() => {
    const outer = view.radius * 0.72
    if (isCapital) {
      // Динамические кольцевые проспекты под любой масштаб города (шаг 3.2 между кольцами зданий).
      const rings: number[] = []
      for (let r = 6.0; r <= view.radius * 0.7; r += 3.2) {
        rings.push(Number(r.toFixed(2)))
      }
      if (rings.length === 0 || outer - (rings[rings.length - 1] ?? 0) > 1.4) {
        rings.push(Number(outer.toFixed(2)))
      }
      return rings
    }
    return [view.radius * 0.42, outer].filter((r) => r >= 2.2)
  }, [isCapital, view.radius])

  // Гарантированный съезд с края ближайшего свободного кольцевого проспекта к КАЖДОМУ зданию-модулю (не заезжает внутрь кольца и дома).
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
      // Сортируем кольца по близости к зданию и берём ближайшее, путь от которого не пересекает другие здания.
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
      const edgeR = r >= chosenRing ? chosenRing + 0.33 : chosenRing - 0.33
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

  // Межкольцевые соединители в свободных от зданий секторах (стыкуются по краям колец).
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
      let placed = 0
      for (let slot = 0; slot < 16 && placed < 4; slot += 1) {
        const angle = (slot / 16) * Math.PI * 2 + rIdx * 0.2
        const midR = (r0 + r1) / 2
        const mx = Math.cos(angle) * midR
        const mz = Math.sin(angle) * midR
        const blocked = view.buildings.some((b) => Math.hypot(b.x - mx, b.z - mz) < 1.85)
        if (!blocked) {
          list.push({
            from: [view.x + Math.cos(angle) * (r0 + 0.33), view.z + Math.sin(angle) * (r0 + 0.33)],
            to: [view.x + Math.cos(angle) * (r1 - 0.33), view.z + Math.sin(angle) * (r1 - 0.33)],
            angle,
            r0: r0 + 0.33,
            r1: r1 - 0.33
          })
          placed += 1
        }
      }
    }
    return list
  }, [ringRadii, view.buildings, view.x, view.z])

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

  const radialSignalRef = useRef<THREE.InstancedMesh>(null)
  const ringSignalRef = useRef<THREE.InstancedMesh>(null)
  const signalPositions = useMemo(() => {
    if (!isCapital) return []
    const r = 6.0
    return Array.from({ length: 4 }, (_, i) => {
      const a = (i / 4) * Math.PI * 2 + Math.PI / 4
      return [view.x + Math.cos(a) * r, view.z + Math.sin(a) * r] as const
    })
  }, [isCapital, view.x, view.z])

  useFrame((state) => {
    const t = paused ? 0 : state.clock.elapsedTime
    const radialGreen = isRadialTrafficGreen(t)
    if (radialSignalRef.current) {
      radialSignalRef.current.material = radialGreen ? greenLightMaterial : redLightMaterial
    }
    if (ringSignalRef.current) {
      ringSignalRef.current.material = radialGreen ? redLightMaterial : greenLightMaterial
    }
  })

  useLayoutEffect(() => {
    const dummy = FRAME_DUMMY
    if (radialSignalRef.current && ringSignalRef.current && signalPositions.length > 0) {
      signalPositions.forEach(([sx, sz], i) => {
        dummy.position.set(sx - 0.38, 0.56, sz)
        dummy.rotation.set(0, 0, 0)
        dummy.scale.set(1, 1, 1)
        dummy.updateMatrix()
        radialSignalRef.current?.setMatrixAt(i, dummy.matrix)

        dummy.position.set(sx + 0.38, 0.56, sz)
        dummy.updateMatrix()
        ringSignalRef.current?.setMatrixAt(i, dummy.matrix)
      })
      radialSignalRef.current.instanceMatrix.needsUpdate = true
      ringSignalRef.current.instanceMatrix.needsUpdate = true
    }
  }, [signalPositions])

  return (
    <group>
      {/* Двухполосные кольцевые проспекты с бордюрами и осевой разметкой. */}
      {ringRadii.map((r, idx) => (
        <group key={`avenue-ring-${idx}`}>
          <mesh position={[view.x, 0.015, view.z]} rotation={[-Math.PI / 2, 0, 0]} material={skin.roadBed}>
            <ringGeometry args={[r - 0.34, r + 0.34, 72]} />
          </mesh>
          <mesh position={[view.x, 0.024, view.z]} rotation={[Math.PI / 2, 0, 0]} material={skin.roadCurb}>
            <torusGeometry args={[r - 0.33, 0.02, 6, 72]} />
          </mesh>
          <mesh position={[view.x, 0.024, view.z]} rotation={[Math.PI / 2, 0, 0]} material={skin.roadCurb}>
            <torusGeometry args={[r + 0.33, 0.02, 6, 72]} />
          </mesh>
          <mesh position={[view.x, 0.026, view.z]} rotation={[Math.PI / 2, 0, 0]} material={laneMaterial}>
            <torusGeometry args={[r, 0.03, 6, 72]} />
          </mesh>
          {r === 6.0 && isCapital ? (
            <mesh
              position={[
                view.x + Math.cos(QUEEN_U_TURN_ANGLE) * r,
                0.027,
                view.z + Math.sin(QUEEN_U_TURN_ANGLE) * r
              ]}
              rotation={[-Math.PI / 2, 0, -QUEEN_U_TURN_ANGLE + Math.PI / 2]}
              material={skin.roadBed}
            >
              <planeGeometry args={[0.52, 0.14]} />
            </mesh>
          ) : null}
        </group>
      ))}
      {/* Межкольцевые проспекты в свободных секторах. */}
      {ringConnectors.map((conn, idx) => (
        <group key={`ring-conn-${idx}`}>
          <RoadSegment3D
            from={conn.from}
            to={conn.to}
            bedMaterial={skin.roadBed}
            curbMaterial={skin.roadCurb}
            lineMaterial={laneMaterial}
            width={0.54}
            y={0.016}
          />
          <mesh
            position={[view.x + Math.cos(conn.angle) * conn.r0, 0.026, view.z + Math.sin(conn.angle) * conn.r0]}
            rotation={[-Math.PI / 2, 0, -conn.angle + Math.PI / 2]}
            material={skin.roadBed}
          >
            <planeGeometry args={[0.46, 0.08]} />
          </mesh>
          <mesh
            position={[view.x + Math.cos(conn.angle) * conn.r1, 0.026, view.z + Math.sin(conn.angle) * conn.r1]}
            rotation={[-Math.PI / 2, 0, -conn.angle + Math.PI / 2]}
            material={skin.roadBed}
          >
            <planeGeometry args={[0.46, 0.08]} />
          </mesh>
        </group>
      ))}
      {/* Съезды с кольцевых дорог к каждому зданию (до границы подиума). */}
      {moduleDriveways.map((drive, idx) => (
        <group key={`module-drive-${idx}`}>
          <RoadSegment3D
            from={drive.from}
            to={drive.to}
            bedMaterial={skin.roadBed}
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
      {/* Светофоры регулировки трафика на главных перекрёстках. */}
      {signalPositions.length > 0 ? (
        <>
          <instancedMesh ref={radialSignalRef} args={[undefined, greenLightMaterial, signalPositions.length]}>
            <sphereGeometry args={[0.1, 8, 8]} />
          </instancedMesh>
          <instancedMesh ref={ringSignalRef} args={[undefined, redLightMaterial, signalPositions.length]}>
            <sphereGeometry args={[0.1, 8, 8]} />
          </instancedMesh>
        </>
      ) : null}
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

export const QUEEN_GATE_ANGLE = Math.PI * 0.38
const QUEEN_U_TURN_ANGLE = QUEEN_GATE_ANGLE - Math.PI

export function computeQueenGateStopAngle(innerBoulevardR: number): number {
  // Дорога ворот имеет ширину 0.64 (полуширина 0.32), длина машины — 0.36 (полудлина 0.18).
  // Стоп-линия вынесена до бордюра дороги ворот, чтобы передний бампер не торчал на перекрёстке.
  const gateClearanceArc = (0.32 + 0.18 + 0.06) / Math.max(1.5, innerBoulevardR)
  return QUEEN_GATE_ANGLE - gateClearanceArc
}

export function computeRingEntryStopProgress(innerBoulevardR: number, homeEdgeR: number): number {
  const span = homeEdgeR - innerBoulevardR
  if (span <= 0.02) return 0.2
  // Внешний бордюр кольца проходит на innerBoulevardR + 0.42; стоп-линия — строго до него.
  const stopR = Math.min(homeEdgeR - 0.02, innerBoulevardR + 0.42 + 0.18 + 0.04)
  return Math.max(0.05, Math.min(0.88, (homeEdgeR - stopR) / span))
}

function angularDistance(a: number, b: number): number {
  let d = a - b
  while (d > Math.PI) d -= Math.PI * 2
  while (d < -Math.PI) d += Math.PI * 2
  return Math.abs(d)
}

/**
 * Квадратичная кривая Безье и единичная касательная для плавного поворота
 * машины на перекрёстке без мгновенного скачка позиции или угла.
 */
function evalTurnBezier(
  ax: number,
  az: number,
  cx: number,
  cz: number,
  bx: number,
  bz: number,
  u: number
): { x: number; z: number; dx: number; dz: number } {
  const s = Math.min(1, Math.max(0, u))
  const inv = 1 - s
  const x = inv * inv * ax + 2 * inv * s * cx + s * s * bx
  const z = inv * inv * az + 2 * inv * s * cz + s * s * bz
  let dx = 2 * inv * (cx - ax) + 2 * s * (bx - cx)
  let dz = 2 * inv * (cz - az) + 2 * s * (bz - cz)
  const len = Math.hypot(dx, dz)
  if (len > 1e-5) {
    dx /= len
    dz /= len
  } else {
    const fallbackLen = Math.hypot(bx - ax, bz - az) || 1
    dx = (bx - ax) / fallbackLen
    dz = (bz - az) / fallbackLen
  }
  return { x, z, dx, dz }
}

function posArc(delta: number): number {
  let d = delta
  while (d < 0) d += Math.PI * 2
  while (d >= Math.PI * 2) d -= Math.PI * 2
  return d
}

/** Находит свободный угол ровно между двумя домами агентов, ближайший к `targetAngle`. */
function nearestInterHouseAngle(total: number, targetAngle: number): number {
  const count = Math.max(2, total)
  let bestAngle = capitalHouseAngle(0.5, count)
  let bestDist = Infinity
  for (let i = 0; i < count; i += 1) {
    const a = capitalHouseAngle(i + 0.5, count)
    const d = angularDistance(a, targetAngle)
    if (d < bestDist) {
      bestDist = d
      bestAngle = a
    }
  }
  return bestAngle
}

/**
 * Траектория курьера на Главном кольце по ПДД РФ (правостороннее движение):
 * все выезды и заезды — только направо, пересечение сплошной осевой кольца
 * происходит исключительно в официальном разрыве сплошной `uTurnAngle`.
 */
function evalMainRingPddPose(
  viewX: number,
  viewZ: number,
  startAngle: number,
  startR: number,
  endAngle: number,
  endR: number,
  ringR: number,
  uTurnAngle: number,
  startIsInner: boolean,
  t: number,
  laneOffset: number
): { x: number; z: number; dx: number; dz: number } {
  const ring1Sign = startIsInner ? 1 : -1
  const ring2Sign = -ring1Sign
  const ring1LaneR = ringR - ring1Sign * 0.13
  const ring2LaneR = ringR - ring2Sign * 0.13
  const arc1 = startIsInner ? posArc(uTurnAngle - startAngle) : posArc(startAngle - uTurnAngle)
  const arc2 = startIsInner ? posArc(uTurnAngle - endAngle) : posArc(endAngle - uTurnAngle)

  const radial1Sign = startIsInner ? 1 : -1
  const r1dx = Math.cos(startAngle) * radial1Sign
  const r1dz = Math.sin(startAngle) * radial1Sign
  const radial2Sign = startIsInner ? 1 : -1
  const r2dx = Math.cos(endAngle) * radial2Sign
  const r2dz = Math.sin(endAngle) * radial2Sign

  const border1R = startIsInner ? ringR - 0.34 : ringR + 0.34
  const border2R = startIsInner ? ringR + 0.34 : ringR - 0.34
  const lead1 = Math.min(0.08, arc1 * 0.35)
  const lead2 = Math.min(0.08, arc2 * 0.35)

  if (t < 0.1) {
    const u = t / 0.1
    const r = startR + u * (border1R - startR)
    return {
      x: viewX + Math.cos(startAngle) * r - r1dz * laneOffset,
      z: viewZ + Math.sin(startAngle) * r + r1dx * laneOffset,
      dx: r1dx,
      dz: r1dz
    }
  }
  if (t <= 0.2) {
    const ax = viewX + Math.cos(startAngle) * border1R - r1dz * laneOffset
    const az = viewZ + Math.sin(startAngle) * border1R + r1dx * laneOffset
    const cx = viewX + Math.cos(startAngle) * ring1LaneR - r1dz * laneOffset
    const cz = viewZ + Math.sin(startAngle) * ring1LaneR + r1dx * laneOffset
    const outA = startAngle + ring1Sign * lead1
    const bx = viewX + Math.cos(outA) * ring1LaneR
    const bz = viewZ + Math.sin(outA) * ring1LaneR
    return evalTurnBezier(ax, az, cx, cz, bx, bz, (t - 0.1) / 0.1)
  }
  if (t < 0.45) {
    const u = (t - 0.2) / 0.25
    const curA = startAngle + ring1Sign * (lead1 + u * Math.max(0, arc1 - lead1 * 2))
    return {
      x: viewX + Math.cos(curA) * ring1LaneR,
      z: viewZ + Math.sin(curA) * ring1LaneR,
      dx: -Math.sin(curA) * ring1Sign,
      dz: Math.cos(curA) * ring1Sign
    }
  }
  if (t <= 0.55) {
    const inA = uTurnAngle - ring1Sign * Math.min(0.1, arc1 * 0.4)
    const outA = uTurnAngle + ring2Sign * Math.min(0.1, arc2 * 0.4)
    const ax = viewX + Math.cos(inA) * ring1LaneR
    const az = viewZ + Math.sin(inA) * ring1LaneR
    const cx = viewX + Math.cos(uTurnAngle) * ringR
    const cz = viewZ + Math.sin(uTurnAngle) * ringR
    const bx = viewX + Math.cos(outA) * ring2LaneR
    const bz = viewZ + Math.sin(outA) * ring2LaneR
    return evalTurnBezier(ax, az, cx, cz, bx, bz, (t - 0.45) / 0.1)
  }
  if (t < 0.8) {
    const u = (t - 0.55) / 0.25
    const start2A = uTurnAngle + ring2Sign * Math.min(0.1, arc2 * 0.4)
    const curA = start2A + ring2Sign * u * Math.max(0, arc2 - lead2 - Math.min(0.1, arc2 * 0.4))
    return {
      x: viewX + Math.cos(curA) * ring2LaneR,
      z: viewZ + Math.sin(curA) * ring2LaneR,
      dx: -Math.sin(curA) * ring2Sign,
      dz: Math.cos(curA) * ring2Sign
    }
  }
  if (t <= 0.9) {
    const inA = endAngle - ring2Sign * lead2
    const ax = viewX + Math.cos(inA) * ring2LaneR
    const az = viewZ + Math.sin(inA) * ring2LaneR
    const cx = viewX + Math.cos(endAngle) * ring2LaneR - r2dz * laneOffset
    const cz = viewZ + Math.sin(endAngle) * ring2LaneR + r2dx * laneOffset
    const bx = viewX + Math.cos(endAngle) * border2R - r2dz * laneOffset
    const bz = viewZ + Math.sin(endAngle) * border2R + r2dx * laneOffset
    return evalTurnBezier(ax, az, cx, cz, bx, bz, (t - 0.8) / 0.1)
  }
  const u = (t - 0.9) / 0.1
  const r = border2R + u * (endR - border2R)
  return {
    x: viewX + Math.cos(endAngle) * r - r2dz * laneOffset,
    z: viewZ + Math.sin(endAngle) * r + r2dx * laneOffset,
    dx: r2dx,
    dz: r2dz
  }
}
const QUEEN_PROCESS_SEC = 1.15

interface AgentCarSim {
  phase: 'delivering' | 'to_ring' | 'outer_to_uturn' | 'along_ring' | 'queued' | 'processing' | 'returning_ring' | 'cooldown'
  progress: number
  curAngle: number
  cooldown: number
  curLaneOffset: number
  turnStartAngle?: number
}

/**
 * Дороги у двора Королевы:
 * - ЕДИНСТВЕННЫЙ въезд в Цитадель Королевы (спереди, по +Z);
 * - кольцевой бульвар (3.55), к которому подключены съезды от каждого дома;
 * - живая пробка и очередь к воротам Королевы: Королева обрабатывает строго
 *   одну машину-запрос за раз, остальные стоят в очереди и ждут.
 */
function QueenRoads3D({
  view,
  lots,
  skin,
  deliveredMapRef,
  active,
  paused
}: {
  view: CityIslandView
  lots: readonly HiveCityHouseLot[]
  skin: CitySkin
  deliveredMapRef: React.MutableRefObject<Map<number, boolean>>
  active: boolean
  paused: boolean
}) {
  const ringRadius = capitalHouseRingRadius(lots[0]?.total ?? 0)
  const innerBoulevardR = Math.max(2.9, ringRadius - 1.25)

  const spokeLineMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#ffd980',
        transparent: true,
        opacity: 0.78,
        depthWrite: false
      }),
    []
  )
  const ringLineMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#ffd980',
        transparent: true,
        opacity: 0.72,
        depthWrite: false
      }),
    []
  )
  const ringBedMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#0b101e',
        transparent: true,
        opacity: 0.94,
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
  const gateSignalGreen = useMemo(() => new THREE.MeshBasicMaterial({ color: '#4ade80' }), [])
  const gateSignalAmber = useMemo(() => new THREE.MeshBasicMaterial({ color: '#f59e0b' }), [])

  useEffect(() => () => {
    spokeLineMaterial.dispose()
    ringLineMaterial.dispose()
    ringBedMaterial.dispose()
    packetMaterial.dispose()
    gateSignalGreen.dispose()
    gateSignalAmber.dispose()
  }, [spokeLineMaterial, ringLineMaterial, ringBedMaterial, packetMaterial, gateSignalGreen, gateSignalAmber])

  const houseAngles = useMemo(
    () => lots.map((lot) => capitalHouseAngle(lot.slot, lot.total)),
    [lots]
  )

  const meshRef = useRef<THREE.InstancedMesh>(null)
  const gateLampRef = useRef<THREE.Mesh>(null)
  const instanceColor = useMemo(() => new THREE.Color(), [])

  const carsRef = useRef<AgentCarSim[]>([])
  const queueRef = useRef<number[]>([])
  const processingLotRef = useRef<{ lotIndex: number; remaining: number } | null>(null)

  useLayoutEffect(() => {
    carsRef.current = lots.map((lot, idx) => ({
      phase: lot.active ? 'delivering' : 'cooldown',
      progress: 0,
      curAngle: QUEEN_GATE_ANGLE,
      cooldown: (idx % 6) * 0.28,
      curLaneOffset: 0.11
    }))
    queueRef.current = []
    processingLotRef.current = null

    const mesh = meshRef.current
    if (!mesh) return
    lots.forEach((lot, index) => {
      instanceColor.set(lot.active ? lot.accent : '#5ea8ff')
      mesh.setColorAt(index, instanceColor)
    })
    if (mesh.instanceColor !== null) mesh.instanceColor.needsUpdate = true
  }, [lots, instanceColor])

  useFrame((_, delta) => {
    const mesh = meshRef.current
    if (!mesh) return
    const dt = paused ? 0 : Math.min(delta, 0.05)
    const dummy = FRAME_DUMMY
    const laneOffset = 0.13
    const gateEntryR = 2.45
    const homeEdgeR = Math.max(innerBoulevardR + 0.35, ringRadius - 0.55)
    const cars = carsRef.current
    const queue = queueRef.current

    if (!active) {
      deliveredMapRef.current.clear()
      queue.length = 0
      processingLotRef.current = null
      if (gateLampRef.current) {
        gateLampRef.current.material = gateSignalAmber
      }
      for (let i = 0; i < lots.length && i < mesh.count; i += 1) {
        dummy.scale.setScalar(0)
        dummy.updateMatrix()
        mesh.setMatrixAt(i, dummy.matrix)
      }
      mesh.instanceMatrix.needsUpdate = true
      return
    }

    if (paused) {
      lots.forEach((lot) => {
        if (lot.active) deliveredMapRef.current.set(lot.slot, true)
      })
    }

    // Шаг 1: обновляем обработку текущей машины у Королевы (строго 1 машина за раз!).
    if (!paused && processingLotRef.current !== null) {
      processingLotRef.current.remaining -= dt
      if (processingLotRef.current.remaining <= 0) {
        const finishedIdx = processingLotRef.current.lotIndex
        const finishedCar = cars[finishedIdx]
        if (finishedCar) {
          finishedCar.phase = 'returning_ring'
          finishedCar.progress = 0
          finishedCar.curAngle = QUEEN_GATE_ANGLE
        }
        processingLotRef.current = null
      }
    }

    const MIN_CAR_ARC_GAP = Math.max(0.15, (0.36 + 0.14) / innerBoulevardR)
    const GATE_STOP_ANGLE = computeQueenGateStopAngle(innerBoulevardR)
    const RING_ENTRY_STOP_PROGRESS = computeRingEntryStopProgress(innerBoulevardR, homeEdgeR)

    // Если ворота Королевы свободны — запускаем ТОЛЬКО самую ближнюю к воротам машину у стоп-линии!
    if (!paused && processingLotRef.current === null) {
      let bestIdx = -1
      let bestAngle = -Infinity
      for (let i = 0; i < cars.length; i += 1) {
        const candidate = cars[i]
        if (!candidate || (candidate.phase !== 'queued' && candidate.phase !== 'along_ring')) continue
        if (candidate.curAngle > bestAngle) {
          bestAngle = candidate.curAngle
          bestIdx = i
        }
      }
      const bestCar = bestIdx >= 0 ? cars[bestIdx] : undefined
      if (bestCar && bestAngle >= GATE_STOP_ANGLE - 0.06) {
        const qPos = queue.indexOf(bestIdx)
        if (qPos >= 0) queue.splice(qPos, 1)
        bestCar.turnStartAngle = bestCar.curAngle
        bestCar.phase = 'processing'
        bestCar.progress = 0
        processingLotRef.current = { lotIndex: bestIdx, remaining: QUEEN_PROCESS_SEC }
      }
    }

    if (gateLampRef.current) {
      gateLampRef.current.material = processingLotRef.current !== null ? gateSignalAmber : gateSignalGreen
    }
    const evalQueenOutPose = (t: number, homeAngle: number) => {
      const gdx = Math.cos(QUEEN_GATE_ANGLE)
      const gdz = Math.sin(QUEEN_GATE_ANGLE)
      const hdx = Math.cos(homeAngle)
      const hdz = Math.sin(homeAngle)
      const innerLaneR = innerBoulevardR - 0.13
      let arcFromUTurn = QUEEN_U_TURN_ANGLE - homeAngle
      while (arcFromUTurn < 0) arcFromUTurn += Math.PI * 2
      while (arcFromUTurn >= Math.PI * 2) arcFromUTurn -= Math.PI * 2
      const outerRForU = (u: number): number => {
        if (arcFromUTurn <= Math.PI * 0.35) return innerBoulevardR + 0.3
        const s = Math.min(1, Math.max(0, (u - 0.45) / 0.32))
        const smooth = s * s * (3 - 2 * s)
        return innerBoulevardR + 0.13 + smooth * 0.17
      }
      const outerRDerivForU = (u: number): number => {
        if (arcFromUTurn <= Math.PI * 0.35 || u <= 0.45 || u >= 0.77) return 0
        const s = (u - 0.45) / 0.32
        return ((6 * s * (1 - s)) / 0.32) * 0.17
      }
      const gateTurnR = Math.max(gateEntryR + 0.25, innerBoulevardR - 0.56)

      if (t < 0.12) {
        const u = t / 0.12
        const r = gateEntryR + u * (gateTurnR - gateEntryR)
        return {
          x: view.x + gdx * r - gdz * laneOffset,
          z: view.z + gdz * r + gdx * laneOffset,
          dx: gdx,
          dz: gdz
        }
      }
      if (t <= 0.22) {
        // Выезд из ворот Королевы: поворот ТОЛЬКО НАПРАВО (+angle) на внутреннее полукольцо.
        const ax = view.x + gdx * gateTurnR - gdz * laneOffset
        const az = view.z + gdz * gateTurnR + gdx * laneOffset
        const cx = view.x + gdx * innerLaneR - gdz * laneOffset
        const cz = view.z + gdz * innerLaneR + gdx * laneOffset
        const outA = QUEEN_GATE_ANGLE + 0.12
        const bx = view.x + Math.cos(outA) * innerLaneR
        const bz = view.z + Math.sin(outA) * innerLaneR
        return evalTurnBezier(ax, az, cx, cz, bx, bz, (t - 0.12) / 0.1)
      }
      if (t < 0.44) {
        const u = (t - 0.22) / 0.22
        const curA = QUEEN_GATE_ANGLE + 0.12 + u * (Math.PI - 0.24)
        return {
          x: view.x + Math.cos(curA) * innerLaneR,
          z: view.z + Math.sin(curA) * innerLaneR,
          dx: -Math.sin(curA),
          dz: Math.cos(curA)
        }
      }
      if (t <= 0.54) {
        // Разворот в официальном разрыве сплошной линии (QUEEN_U_TURN_ANGLE) на внешнее полукольцо (-angle).
        const inA = QUEEN_GATE_ANGLE + Math.PI - 0.12
        const outA = QUEEN_U_TURN_ANGLE - 0.08
        const ax = view.x + Math.cos(inA) * innerLaneR
        const az = view.z + Math.sin(inA) * innerLaneR
        const cx = view.x + Math.cos(QUEEN_U_TURN_ANGLE) * innerBoulevardR
        const cz = view.z + Math.sin(QUEEN_U_TURN_ANGLE) * innerBoulevardR
        const outLaneR = outerRForU(0.05)
        const bx = view.x + Math.cos(outA) * outLaneR
        const bz = view.z + Math.sin(outA) * outLaneR
        return evalTurnBezier(ax, az, cx, cz, bx, bz, (t - 0.44) / 0.1)
      }
      if (t < 0.8) {
        const u = (t - 0.54) / 0.26
        const curA = QUEEN_U_TURN_ANGLE - 0.08 - Math.max(0, arcFromUTurn - 0.16) * u
        const laneR = outerRForU(u)
        const dRdu = outerRDerivForU(u)
        const dAdu = -Math.max(0.05, arcFromUTurn - 0.16)
        const rawDx = -Math.sin(curA) * laneR * dAdu + Math.cos(curA) * dRdu
        const rawDz = Math.cos(curA) * laneR * dAdu + Math.sin(curA) * dRdu
        const len = Math.hypot(rawDx, rawDz) || 1
        return {
          x: view.x + Math.cos(curA) * laneR,
          z: view.z + Math.sin(curA) * laneR,
          dx: rawDx / len,
          dz: rawDz / len
        }
      }
      if (t <= 0.9) {
        // Поворот НАПРАВО с внешнего полукольца на съезд к дому.
        const inA = homeAngle + 0.08
        const inLaneR = outerRForU(1)
        const ax = view.x + Math.cos(inA) * inLaneR
        const az = view.z + Math.sin(inA) * inLaneR
        const cx = view.x + hdx * inLaneR - hdz * laneOffset
        const cz = view.z + hdz * inLaneR + hdx * laneOffset
        const rOut = Math.min(homeEdgeR, innerBoulevardR + 0.56)
        const bx = view.x + hdx * rOut - hdz * laneOffset
        const bz = view.z + hdz * rOut + hdx * laneOffset
        return evalTurnBezier(ax, az, cx, cz, bx, bz, (t - 0.8) / 0.1)
      }
      const u = (t - 0.9) / 0.1
      const rStart = Math.min(homeEdgeR, innerBoulevardR + 0.56)
      const r = rStart + u * (homeEdgeR - rStart)
      return {
        x: view.x + hdx * r - hdz * laneOffset,
        z: view.z + hdz * r + hdx * laneOffset,
        dx: hdx,
        dz: hdz
      }
    }

    // Шаг 2: двигаем каждую машину и обновляем её матрицу (с запретом проезда сквозь впереди идущую машину).
    lots.forEach((lot, index) => {
      if (index >= mesh.count) return
      const car = cars[index]
      const homeAngle = houseAngles[index]
      if (!car || homeAngle === undefined) return

      if (!lot.active) {
        deliveredMapRef.current.delete(lot.slot)
        dummy.scale.setScalar(0)
        dummy.updateMatrix()
        mesh.setMatrixAt(index, dummy.matrix)
        return
      }

      if (paused) {
        const x = view.x + Math.cos(homeAngle) * (innerBoulevardR + 0.13)
        const z = view.z + Math.sin(homeAngle) * (innerBoulevardR + 0.13)
        dummy.position.set(x, 0.12, z)
        dummy.lookAt(x - Math.sin(homeAngle), 0.12, z + Math.cos(homeAngle))
        dummy.scale.set(0.15, 0.11, 0.36)
        dummy.updateMatrix()
        mesh.setMatrixAt(index, dummy.matrix)
        return
      }

      let x = view.x
      let z = view.z
      let dirX = 0
      let dirZ = 1
      let visible = true

      // Обратный путь по внутренней полосе (-angle) от ворот к дому.
      let backArc = QUEEN_GATE_ANGLE - homeAngle
      while (backArc < 0) backArc += Math.PI * 2
      while (backArc >= Math.PI * 2) backArc -= Math.PI * 2

      if (car.phase === 'delivering') {
        if (car.cooldown > 0) {
          car.cooldown -= dt
          visible = false
        } else {
          car.progress += dt * 0.55
          if (car.progress >= 1) {
            deliveredMapRef.current.set(lot.slot, true)
            car.phase = 'to_ring'
            car.progress = 0
            car.cooldown = 0.25
          }
        }
        const t = Math.min(1, Math.max(0, car.progress))
        const pose = evalQueenOutPose(t, homeAngle)
        x = pose.x
        z = pose.z
        dirX = pose.dx
        dirZ = pose.dz
      } else if (car.phase === 'to_ring') {
        const hdx = -Math.cos(homeAngle)
        const hdz = -Math.sin(homeAngle)
        if (car.cooldown > 0) {
          car.cooldown -= dt
          dirX = hdx
          dirZ = hdz
          x = view.x + Math.cos(homeAngle) * homeEdgeR - dirZ * laneOffset
          z = view.z + Math.sin(homeAngle) * homeEdgeR + dirX * laneOffset
        } else {
          // Выезд от дома агента ТОЛЬКО НАПРАВО (-angle) на внешнее полукольцо (без пересечения сплошной!).
          const ringBlocked = cars.some((other, oIdx) => {
            if (oIdx === index) return false
            if (other.phase === 'outer_to_uturn') {
              return angularDistance(other.curAngle, homeAngle) < MIN_CAR_ARC_GAP * 1.25
            }
            if ((other.phase === 'delivering' || other.phase === 'returning_ring') && other.progress >= 0.52 && other.progress < 0.85) {
              const otherHome = houseAngles[oIdx]
              if (otherHome === undefined) return false
              let otherArc = QUEEN_U_TURN_ANGLE - otherHome
              while (otherArc < 0) otherArc += Math.PI * 2
              while (otherArc >= Math.PI * 2) otherArc -= Math.PI * 2
              const u = (other.progress - 0.54) / 0.26
              const otherA = QUEEN_U_TURN_ANGLE - 0.08 - Math.max(0, otherArc - 0.16) * u
              return angularDistance(otherA, homeAngle) < MIN_CAR_ARC_GAP * 1.25
            }
            return false
          })
          const nextProgress = car.progress + dt * 1.4
          if (car.progress > RING_ENTRY_STOP_PROGRESS) {
            car.progress = Math.min(1, nextProgress)
          } else if (car.progress < RING_ENTRY_STOP_PROGRESS) {
            car.progress = ringBlocked ? Math.min(RING_ENTRY_STOP_PROGRESS, nextProgress) : Math.min(1, nextProgress)
          } else if (!ringBlocked) {
            car.progress = Math.min(1, nextProgress)
          }
          const targetEntryAngle = homeAngle - 0.06
          const targetLaneOffset = 0.3
          const targetLaneR = innerBoulevardR + targetLaneOffset
          if (car.progress >= 1) {
            car.phase = 'outer_to_uturn'
            car.progress = 0
            car.curAngle = targetEntryAngle
            car.curLaneOffset = targetLaneOffset
          }
          if (car.progress <= RING_ENTRY_STOP_PROGRESS) {
            const r = homeEdgeR - Math.min(1, car.progress) * (homeEdgeR - innerBoulevardR)
            dirX = hdx
            dirZ = hdz
            x = view.x + Math.cos(homeAngle) * r - dirZ * laneOffset
            z = view.z + Math.sin(homeAngle) * r + dirX * laneOffset
          } else {
            const turnU = (car.progress - RING_ENTRY_STOP_PROGRESS) / Math.max(0.05, 1 - RING_ENTRY_STOP_PROGRESS)
            const stopR = homeEdgeR - RING_ENTRY_STOP_PROGRESS * (homeEdgeR - innerBoulevardR)
            const ax = view.x + Math.cos(homeAngle) * stopR - hdz * laneOffset
            const az = view.z + Math.sin(homeAngle) * stopR + hdx * laneOffset
            const cx = view.x + Math.cos(homeAngle) * targetLaneR - hdz * laneOffset
            const cz = view.z + Math.sin(homeAngle) * targetLaneR + hdx * laneOffset
            const bx = view.x + Math.cos(targetEntryAngle) * targetLaneR
            const bz = view.z + Math.sin(targetEntryAngle) * targetLaneR
            const pose = evalTurnBezier(ax, az, cx, cz, bx, bz, turnU)
            x = pose.x
            z = pose.z
            dirX = pose.dx
            dirZ = pose.dz
          }
        }
      } else if (car.phase === 'outer_to_uturn') {
        // Движение по внешнему полукольцу (-angle) до разрыва сплошной (QUEEN_U_TURN_ANGLE) и разворот на внутреннее (+angle).
        let arcToUTurn = car.curAngle - QUEEN_U_TURN_ANGLE
        while (arcToUTurn < 0) arcToUTurn += Math.PI * 2
        while (arcToUTurn >= Math.PI * 2) arcToUTurn -= Math.PI * 2
        let maxTravel = arcToUTurn
        for (let oIdx = 0; oIdx < cars.length; oIdx += 1) {
          if (oIdx === index) continue
          const other = cars[oIdx]
          if (!other || other.phase !== 'outer_to_uturn') continue
          let otherArc = other.curAngle - QUEEN_U_TURN_ANGLE
          while (otherArc < 0) otherArc += Math.PI * 2
          while (otherArc >= Math.PI * 2) otherArc -= Math.PI * 2
          if (otherArc < arcToUTurn) {
            maxTravel = Math.min(maxTravel, arcToUTurn - otherArc - MIN_CAR_ARC_GAP)
          }
        }
        // Не выезжаем в точку разворота, если начало внутреннего кольца у разворота занято.
        const uTurnBlocked =
          arcToUTurn <= 0.16 &&
          cars.some(
            (other, oIdx) =>
              oIdx !== index &&
              (other.phase === 'along_ring' || other.phase === 'queued') &&
              Math.abs(other.curAngle - QUEEN_U_TURN_ANGLE) < MIN_CAR_ARC_GAP * 1.25
          )
        const allowedStep = uTurnBlocked ? 0 : Math.min(Math.max(0, maxTravel), dt * 1.15)
        car.curAngle -= allowedStep
        const newArcToUTurn = Math.max(0, arcToUTurn - allowedStep)
        if (newArcToUTurn <= 0.02 && !uTurnBlocked) {
          car.phase = 'along_ring'
          car.progress = 0
          car.curAngle = QUEEN_U_TURN_ANGLE
          car.curLaneOffset = 0.11
        }
        // На развороте (последние 0.18 рад) плавно пересекаем разрыв сплошной с внешнего на внутреннее полукольцо.
        const desiredOuterOffset = newArcToUTurn > 0.35 ? 0.13 : 0.13
        const uTurnZone = newArcToUTurn < 0.18 ? 1 - newArcToUTurn / 0.18 : 0
        if (uTurnZone > 0) {
          const ax = view.x + Math.cos(QUEEN_U_TURN_ANGLE + 0.18) * (innerBoulevardR + 0.13)
          const az = view.z + Math.sin(QUEEN_U_TURN_ANGLE + 0.18) * (innerBoulevardR + 0.13)
          const cx = view.x + Math.cos(QUEEN_U_TURN_ANGLE) * innerBoulevardR
          const cz = view.z + Math.sin(QUEEN_U_TURN_ANGLE) * innerBoulevardR
          const bx = view.x + Math.cos(QUEEN_U_TURN_ANGLE) * (innerBoulevardR - 0.11)
          const bz = view.z + Math.sin(QUEEN_U_TURN_ANGLE) * (innerBoulevardR - 0.11)
          const pose = evalTurnBezier(ax, az, cx, cz, bx, bz, uTurnZone)
          x = pose.x
          z = pose.z
          dirX = pose.dx
          dirZ = pose.dz
        } else {
          const laneDiff = desiredOuterOffset - car.curLaneOffset
          const arcMove = allowedStep * (innerBoulevardR + car.curLaneOffset)
          const maxStep = arcMove * 0.36
          const laneStep = Math.abs(laneDiff) <= maxStep ? laneDiff : Math.sign(laneDiff) * maxStep
          car.curLaneOffset += laneStep
          const laneR = innerBoulevardR + car.curLaneOffset
          x = view.x + Math.cos(car.curAngle) * laneR
          z = view.z + Math.sin(car.curAngle) * laneR
          const tx = Math.sin(car.curAngle)
          const tz = -Math.cos(car.curAngle)
          if (arcMove > 1e-5 && Math.abs(laneStep) > 1e-6) {
            const moveX = tx * arcMove + Math.cos(car.curAngle) * laneStep
            const moveZ = tz * arcMove + Math.sin(car.curAngle) * laneStep
            const dLen = Math.hypot(moveX, moveZ) || 1
            dirX = moveX / dLen
            dirZ = moveZ / dLen
          } else {
            dirX = tx
            dirZ = tz
          }
        }
      } else if (car.phase === 'along_ring' || car.phase === 'queued') {
        // 4-полосное движение: транзитные/дальние машины едут по скоростной (-0.11) мимо пробки,
        // а на подъезде к воротам плавно перестраиваются в полосу очереди (-0.30) с соблюдением дистанции.
        const distToGate = GATE_STOP_ANGLE - car.curAngle
        const isInQueueZone = car.phase === 'queued' || distToGate <= 0.65
        let maxAllowedAngle = GATE_STOP_ANGLE
        let queuedAhead = false
        for (let oIdx = 0; oIdx < cars.length; oIdx += 1) {
          if (oIdx === index) continue
          const other = cars[oIdx]
          if (!other || (other.phase !== 'along_ring' && other.phase !== 'queued')) continue
          if (other.curAngle <= car.curAngle) continue
          const otherInQueueZone = other.phase === 'queued' || GATE_STOP_ANGLE - other.curAngle <= 0.65
          if (isInQueueZone || otherInQueueZone || other.curAngle - car.curAngle < MIN_CAR_ARC_GAP * 2) {
            maxAllowedAngle = Math.min(maxAllowedAngle, other.curAngle - MIN_CAR_ARC_GAP)
            if (other.phase === 'queued') queuedAhead = true
          }
        }
        const nextAngle = Math.min(maxAllowedAngle, car.curAngle + dt * (isInQueueZone ? 0.85 : 1.15))
        const angleStep = Math.max(0, nextAngle - car.curAngle)
        car.curAngle = Math.max(car.curAngle, nextAngle)
        if (
          car.phase === 'along_ring' &&
          (car.curAngle >= GATE_STOP_ANGLE - 0.02 ||
            ((isInQueueZone || queuedAhead) && maxAllowedAngle - car.curAngle < 0.02))
        ) {
          car.phase = 'queued'
          if (!queue.includes(index)) queue.push(index)
        }
        // Плавное перестроение в полосу очереди (0.11 -> 0.30) только при движении вперёд (машины не ездят боком!).
        const desiredLaneOffset = car.phase === 'queued' || GATE_STOP_ANGLE - car.curAngle <= 0.72 ? 0.3 : 0.11
        const laneDiff = desiredLaneOffset - car.curLaneOffset
        const arcMove = angleStep * (innerBoulevardR - car.curLaneOffset)
        const maxLaneStep = arcMove * 0.36
        const laneStep = Math.abs(laneDiff) <= maxLaneStep ? laneDiff : Math.sign(laneDiff) * maxLaneStep
        car.curLaneOffset += laneStep
        const laneR = innerBoulevardR - car.curLaneOffset
        x = view.x + Math.cos(car.curAngle) * laneR
        z = view.z + Math.sin(car.curAngle) * laneR
        const tx = -Math.sin(car.curAngle)
        const tz = Math.cos(car.curAngle)
        if (arcMove > 1e-5 && Math.abs(laneStep) > 1e-6) {
          const moveX = tx * arcMove - Math.cos(car.curAngle) * laneStep
          const moveZ = tz * arcMove - Math.sin(car.curAngle) * laneStep
          const dLen = Math.hypot(moveX, moveZ) || 1
          dirX = moveX / dLen
          dirZ = moveZ / dLen
        } else {
          dirX = tx
          dirZ = tz
        }
      } else if (car.phase === 'processing') {
        car.progress = Math.min(1, car.progress + dt * 2.0)
        const gdx = -Math.cos(QUEEN_GATE_ANGLE)
        const gdz = -Math.sin(QUEEN_GATE_ANGLE)
        if (car.progress <= 0.42) {
          const u = car.progress / 0.42
          const startA = car.turnStartAngle ?? GATE_STOP_ANGLE
          const startLaneR = innerBoulevardR - car.curLaneOffset
          const ax = view.x + Math.cos(startA) * startLaneR
          const az = view.z + Math.sin(startA) * startLaneR
          const cx = view.x + Math.cos(QUEEN_GATE_ANGLE) * startLaneR - gdz * laneOffset
          const cz = view.z + Math.sin(QUEEN_GATE_ANGLE) * startLaneR + gdx * laneOffset
          const enterR = innerBoulevardR - 0.52
          const bx = view.x + Math.cos(QUEEN_GATE_ANGLE) * enterR - gdz * laneOffset
          const bz = view.z + Math.sin(QUEEN_GATE_ANGLE) * enterR + gdx * laneOffset
          const pose = evalTurnBezier(ax, az, cx, cz, bx, bz, u)
          x = pose.x
          z = pose.z
          dirX = pose.dx
          dirZ = pose.dz
        } else {
          const u = (car.progress - 0.42) / 0.58
          const enterR = innerBoulevardR - 0.52
          const r = enterR - u * (enterR - gateEntryR)
          dirX = gdx
          dirZ = gdz
          x = view.x + Math.cos(QUEEN_GATE_ANGLE) * r - dirZ * laneOffset
          z = view.z + Math.sin(QUEEN_GATE_ANGLE) * r + dirX * laneOffset
        }
      } else if (car.phase === 'returning_ring') {
        // Возврат по выездной полосе ворот и внешней полосе кольца (-angle) со скруглениями Безье.
        car.progress += dt * 0.55
        if (car.progress >= 1) {
          car.phase = 'cooldown'
          car.progress = 0
          car.cooldown = 0.25 + (index % 5) * 0.15
        }
        const t = Math.min(1, car.progress)
        const pose = evalQueenOutPose(t, homeAngle)
        x = pose.x
        z = pose.z
        dirX = pose.dx
        dirZ = pose.dz
      } else {
        // Ожидание у подъезда дома без пропадания/мигания.
        dirX = -Math.cos(homeAngle)
        dirZ = -Math.sin(homeAngle)
        x = view.x + Math.cos(homeAngle) * homeEdgeR - dirZ * laneOffset
        z = view.z + Math.sin(homeAngle) * homeEdgeR + dirX * laneOffset
        car.cooldown -= dt
        if (car.cooldown <= 0) {
          car.phase = 'to_ring'
          car.progress = 0
        }
      }

      if (!visible) {
        dummy.scale.setScalar(0)
        dummy.updateMatrix()
        mesh.setMatrixAt(index, dummy.matrix)
        return
      }

      const carY = 0.13 + (index % 7) * 0.0008
      dummy.position.set(x, carY, z)
      dummy.lookAt(x + dirX, carY, z + dirZ)
      dummy.scale.set(0.16, 0.11, 0.36)
      dummy.updateMatrix()
      mesh.setMatrixAt(index, dummy.matrix)
    })
    mesh.instanceMatrix.needsUpdate = true
  })

  if (lots.length === 0) return <></>
  const homeEdgeR = Math.max(innerBoulevardR + 0.35, ringRadius - 0.55)
  const gateX = view.x + Math.cos(QUEEN_GATE_ANGLE) * 2.52
  const gateZ = view.z + Math.sin(QUEEN_GATE_ANGLE) * 2.52

  return (
    <group>
      {/* ЕДИНСТВЕННЫЙ парадный въезд в Цитадель Королевы с золотой аркой ворот (стыкуется ровно по внутреннему краю кольца). */}
      <RoadSegment3D
        from={[view.x + Math.cos(QUEEN_GATE_ANGLE) * 2.4, view.z + Math.sin(QUEEN_GATE_ANGLE) * 2.4]}
        to={[view.x + Math.cos(QUEEN_GATE_ANGLE) * (innerBoulevardR - 0.41), view.z + Math.sin(QUEEN_GATE_ANGLE) * (innerBoulevardR - 0.41)]}
        bedMaterial={skin.roadBed}
        curbMaterial={skin.roadCurb}
        lineMaterial={spokeLineMaterial}
        width={0.64}
        y={0.025}
      />
      {/* Золотая арка единственного въезда и индикатор занятости Королевы. */}
      <group position={[gateX, 0, gateZ]} rotation={[0, -QUEEN_GATE_ANGLE + Math.PI / 2, 0]}>
        <mesh position={[-0.36, 0.32, 0]} material={skin.gold}>
          <boxGeometry args={[0.1, 0.64, 0.1]} />
        </mesh>
        <mesh position={[0.36, 0.32, 0]} material={skin.gold}>
          <boxGeometry args={[0.1, 0.64, 0.1]} />
        </mesh>
        <mesh position={[0, 0.66, 0]} material={skin.gold}>
          <boxGeometry args={[0.84, 0.1, 0.12]} />
        </mesh>
        <mesh ref={gateLampRef} position={[0, 0.82, 0]} material={gateSignalGreen}>
          <sphereGeometry args={[0.1, 8, 8]} />
        </mesh>
      </group>

      {/* ЕДИНСТВЕННЫЙ подъезд к КАЖДОМУ дому агента (1 въезд и выезд с внутреннего кольца). */}
      {lots.map((lot, index) => {
        const angle = houseAngles[index]
        if (angle === undefined) return null
        const frontStartR = innerBoulevardR + 0.41
        return (
          <RoadSegment3D
            key={`road-${lot.departmentKey}-${lot.slot}`}
            from={[view.x + Math.cos(angle) * frontStartR, view.z + Math.sin(angle) * frontStartR]}
            to={[view.x + Math.cos(angle) * homeEdgeR, view.z + Math.sin(angle) * homeEdgeR]}
            bedMaterial={skin.roadBed}
            curbMaterial={skin.roadCurb}
            lineMaterial={spokeLineMaterial}
            width={0.46}
            y={0.024}
          />
        )
      })}
      {/* Межкольцевой проспект в свободном зазоре между домами (связывает внутреннее кольцо и Главный проспект 6.0). */}
      {(() => {
        const bridgeAngle = nearestInterHouseAngle(lots[0]?.total ?? lots.length, QUEEN_U_TURN_ANGLE)
        const startR = innerBoulevardR + 0.41
        const endR = Math.max(startR + 0.3, 6.0 - 0.33)
        return (
          <group>
            <RoadSegment3D
              from={[view.x + Math.cos(bridgeAngle) * startR, view.z + Math.sin(bridgeAngle) * startR]}
              to={[view.x + Math.cos(bridgeAngle) * endR, view.z + Math.sin(bridgeAngle) * endR]}
              bedMaterial={skin.roadBed}
              curbMaterial={skin.roadCurb}
              lineMaterial={spokeLineMaterial}
              width={0.46}
              y={0.023}
            />
            <mesh
              position={[view.x + Math.cos(bridgeAngle) * startR, 0.037, view.z + Math.sin(bridgeAngle) * startR]}
              rotation={[-Math.PI / 2, 0, -bridgeAngle + Math.PI / 2]}
              material={skin.roadBed}
            >
              <planeGeometry args={[0.38, 0.08]} />
            </mesh>
            <mesh
              position={[view.x + Math.cos(bridgeAngle) * endR, 0.037, view.z + Math.sin(bridgeAngle) * endR]}
              rotation={[-Math.PI / 2, 0, -bridgeAngle + Math.PI / 2]}
              material={skin.roadBed}
            >
              <planeGeometry args={[0.38, 0.08]} />
            </mesh>
          </group>
        )
      })()}

      {/* 4-полосный кольцевой бульвар перед домами агентов (2 полосы к воротам + 2 скоростных/обратных). */}
      <mesh position={[view.x, 0.022, view.z]} rotation={[-Math.PI / 2, 0, 0]} material={ringBedMaterial}>
        <ringGeometry args={[innerBoulevardR - 0.42, innerBoulevardR + 0.42, 72]} />
      </mesh>
      <mesh position={[view.x, 0.032, view.z]} rotation={[Math.PI / 2, 0, 0]} material={skin.roadCurb}>
        <torusGeometry args={[innerBoulevardR - 0.41, 0.018, 6, 72]} />
      </mesh>
      <mesh position={[view.x, 0.032, view.z]} rotation={[Math.PI / 2, 0, 0]} material={skin.roadCurb}>
        <torusGeometry args={[innerBoulevardR + 0.41, 0.018, 6, 72]} />
      </mesh>
      {/* Межполосная разметка внутри каждого из 2 направлений (делит дорогу на 4 полосы). */}
      <mesh position={[view.x, 0.034, view.z]} rotation={[Math.PI / 2, 0, 0]} material={skin.roadCurb}>
        <torusGeometry args={[innerBoulevardR - 0.205, 0.012, 6, 72]} />
      </mesh>
      <mesh position={[view.x, 0.034, view.z]} rotation={[Math.PI / 2, 0, 0]} material={skin.roadCurb}>
        <torusGeometry args={[innerBoulevardR + 0.205, 0.012, 6, 72]} />
      </mesh>
      {/* Главная осевая разделительная линия. */}
      <mesh position={[view.x, 0.036, view.z]} rotation={[Math.PI / 2, 0, 0]} material={ringLineMaterial}>
        <torusGeometry args={[innerBoulevardR, 0.03, 6, 72]} />
      </mesh>
      {/* Официальный разрыв сплошной линии на противоположной стороне кольца (место разворота). */}
      <mesh
        position={[
          view.x + Math.cos(QUEEN_U_TURN_ANGLE) * innerBoulevardR,
          0.037,
          view.z + Math.sin(QUEEN_U_TURN_ANGLE) * innerBoulevardR
        ]}
        rotation={[-Math.PI / 2, 0, -QUEEN_U_TURN_ANGLE + Math.PI / 2]}
        material={skin.roadBed}
      >
        <planeGeometry args={[0.56, 0.14]} />
      </mesh>
      {/* Чистый перекрёсток на въезде в Цитадель Королевы (убирает только внутренний бордюр в проёме въезда). */}
      <mesh
        position={[
          view.x + Math.cos(QUEEN_GATE_ANGLE) * (innerBoulevardR - 0.41),
          0.037,
          view.z + Math.sin(QUEEN_GATE_ANGLE) * (innerBoulevardR - 0.41)
        ]}
        rotation={[-Math.PI / 2, 0, -QUEEN_GATE_ANGLE + Math.PI / 2]}
        material={skin.roadBed}
      >
        <planeGeometry args={[0.56, 0.08]} />
      </mesh>
      {/* Чистые Т-образные стыки единственных съездов домов с внутренним кольцом. */}
      {houseAngles.map((angle, idx) => (
        <mesh
          key={`junction-patch-${idx}`}
          position={[
            view.x + Math.cos(angle) * (innerBoulevardR + 0.41),
            0.037,
            view.z + Math.sin(angle) * (innerBoulevardR + 0.41)
          ]}
          rotation={[-Math.PI / 2, 0, -angle + Math.PI / 2]}
          material={skin.roadBed}
        >
          <planeGeometry args={[0.38, 0.08]} />
        </mesh>
      ))}

      {/* Машины в очереди и на выезде. */}
      <instancedMesh
        ref={meshRef}
        args={[undefined, packetMaterial, Math.max(1, lots.length)]}
        visible={lots.length > 0}
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
  deliveredMapRef,
  active,
  paused
}: {
  view: CityIslandView
  lots: readonly HiveCityHouseLot[]
  deliveredMapRef: React.MutableRefObject<Map<number, boolean>>
  active: boolean
  paused: boolean
}) {
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
    packetMaterial.dispose()
  }, [packetMaterial])

  const MAIN_AVENUES_RADIUS = 6.0

  const routes = useMemo(() => {
    const modules = view.buildings.filter((item) => item.building.kind !== 'citadel')
    if (modules.length === 0 || lots.length === 0) return []
    const ringRadius = capitalHouseRingRadius(lots[0]?.total ?? 0)
    const innerBoulevardR = Math.max(2.9, ringRadius - 1.25)
    const homeEdgeR = Math.max(innerBoulevardR + 0.35, ringRadius - 0.55)
    const bridgeAngle = nearestInterHouseAngle(lots[0]?.total ?? lots.length, QUEEN_U_TURN_ANGLE)
    const sourceLots = lots.filter((lot) => lot.active)

    const list: Array<{
      readonly lotSlot: number
      readonly houseAngle: number
      readonly houseExitR: number
      readonly bridgeAngle: number
      readonly innerRingR: number
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
        lotSlot: lot.slot,
        houseAngle,
        houseExitR: homeEdgeR,
        bridgeAngle,
        innerRingR: innerBoulevardR,
        targetAngle,
        targetEntryR: Math.max(MAIN_AVENUES_RADIUS + 0.34, Math.min(MAIN_AVENUES_RADIUS + 1.4, targetR - 0.85)),
        color: lot.active ? lot.accent : '#7ef2c4',
        phase: idx / Math.max(1, sourceLots.length),
        active: lot.active
      })
    })
    return list.slice(0, 18)
  }, [view, lots])

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
    if (!mesh || !active || routes.length === 0) return
    const time = paused ? 0 : state.clock.elapsedTime
    const dummy = FRAME_DUMMY
    const flowSpeed = 0.16
    const laneOffset = 0.11

    routes.forEach((route, index) => {
      for (let dir = 0; dir < 2; dir += 1) {
        const instIdx = index * 2 + dir
        if (instIdx >= mesh.count) continue
        // Пока машина от Королевы не доехала до этого дома-агента, он не отправляет курьеров к модулям.
        if (route.active && !paused && deliveredMapRef.current.get(route.lotSlot) !== true) {
          dummy.scale.setScalar(0)
          dummy.updateMatrix()
          mesh.setMatrixAt(instIdx, dummy.matrix)
          continue
        }
        const p = paused ? (route.phase + dir * 0.5) % 1 : (route.phase + dir * 0.5 + time * flowSpeed) % 1
        // Курьеры выезжают и возвращаются через ЕДИНСТВЕННЫЙ подъезд дома (на внутреннее кольцо),
        // затем через межкольцевой проспект bridgeAngle выходят на Главный проспект 6.0.
        let pose: { x: number; z: number; dx: number; dz: number }
        if (dir === 0) {
          if (p < 0.32) {
            const subT = p / 0.32
            const outerLaneR = route.innerRingR + 0.28
            const innerBorderR = route.innerRingR + 0.41
            const hdx = -Math.cos(route.houseAngle)
            const hdz = -Math.sin(route.houseAngle)
            const bdx = Math.cos(route.bridgeAngle)
            const bdz = Math.sin(route.bridgeAngle)
            const arc = posArc(route.houseAngle - route.bridgeAngle)
            if (subT < 0.25) {
              const u = subT / 0.25
              const r = route.houseExitR - u * (route.houseExitR - innerBorderR)
              pose = {
                x: view.x + Math.cos(route.houseAngle) * r - hdz * laneOffset,
                z: view.z + Math.sin(route.houseAngle) * r + hdx * laneOffset,
                dx: hdx,
                dz: hdz
              }
            } else if (subT < 0.75) {
              const u = (subT - 0.25) / 0.5
              const curA = route.houseAngle - arc * u
              pose = {
                x: view.x + Math.cos(curA) * outerLaneR,
                z: view.z + Math.sin(curA) * outerLaneR,
                dx: Math.sin(curA),
                dz: -Math.cos(curA)
              }
            } else {
              const u = (subT - 0.75) / 0.25
              const r = innerBorderR + u * (MAIN_AVENUES_RADIUS - 0.34 - innerBorderR)
              pose = {
                x: view.x + Math.cos(route.bridgeAngle) * r - bdz * laneOffset,
                z: view.z + Math.sin(route.bridgeAngle) * r + bdx * laneOffset,
                dx: bdx,
                dz: bdz
              }
            }
          } else {
            pose = evalMainRingPddPose(
              view.x,
              view.z,
              route.bridgeAngle,
              MAIN_AVENUES_RADIUS - 0.34,
              route.targetAngle,
              route.targetEntryR,
              MAIN_AVENUES_RADIUS,
              QUEEN_U_TURN_ANGLE,
              true,
              (p - 0.32) / 0.68,
              laneOffset
            )
          }
        } else {
          if (p < 0.68) {
            pose = evalMainRingPddPose(
              view.x,
              view.z,
              route.targetAngle,
              route.targetEntryR,
              route.bridgeAngle,
              MAIN_AVENUES_RADIUS - 0.34,
              MAIN_AVENUES_RADIUS,
              QUEEN_U_TURN_ANGLE,
              false,
              p / 0.68,
              laneOffset
            )
          } else {
            const subT = (p - 0.68) / 0.32
            const outerLaneR = route.innerRingR + 0.28
            const innerBorderR = route.innerRingR + 0.41
            const bdx = -Math.cos(route.bridgeAngle)
            const bdz = -Math.sin(route.bridgeAngle)
            const hdx = Math.cos(route.houseAngle)
            const hdz = Math.sin(route.houseAngle)
            const arc = posArc(route.bridgeAngle - route.houseAngle)
            if (subT < 0.25) {
              const u = subT / 0.25
              const r = MAIN_AVENUES_RADIUS - 0.34 - u * (MAIN_AVENUES_RADIUS - 0.34 - innerBorderR)
              pose = {
                x: view.x + Math.cos(route.bridgeAngle) * r - bdz * laneOffset,
                z: view.z + Math.sin(route.bridgeAngle) * r + bdx * laneOffset,
                dx: bdx,
                dz: bdz
              }
            } else if (subT < 0.75) {
              const u = (subT - 0.25) / 0.5
              const curA = route.bridgeAngle - arc * u
              pose = {
                x: view.x + Math.cos(curA) * outerLaneR,
                z: view.z + Math.sin(curA) * outerLaneR,
                dx: Math.sin(curA),
                dz: -Math.cos(curA)
              }
            } else {
              const u = (subT - 0.75) / 0.25
              const r = innerBorderR + u * (route.houseExitR - innerBorderR)
              pose = {
                x: view.x + Math.cos(route.houseAngle) * r - hdz * laneOffset,
                z: view.z + Math.sin(route.houseAngle) * r + hdx * laneOffset,
                dx: hdx,
                dz: hdz
              }
            }
          }
        }

        const courierY = 0.12 + (instIdx % 7) * 0.0008
        dummy.position.set(pose.x, courierY, pose.z)
        dummy.lookAt(pose.x + pose.dx, courierY, pose.z + pose.dz)
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
      {/* Подъезды к домам и модулям уже построены без пересечений в QueenRoads3D и CityStreets3D;
          здесь рисуются только курьерские машины-потоки. */}
      <instancedMesh
        ref={meshRef}
        args={[undefined, packetMaterial, Math.max(1, routes.length * 2)]}
        visible={active && routes.length > 0}
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
  deliveredMapRef,
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
  deliveredMapRef: React.MutableRefObject<Map<number, boolean>>
  highlighted: boolean
  paused: boolean
  onSelectDepartment: ((departmentKey: string) => void) | undefined
}) {
  const angle = capitalHouseAngle(lot.slot, lot.total)
  const ringRadius = capitalHouseRingRadius(lot.total)
  const position = useMemo<[number, number, number]>(() => {
    const x = view.x + Math.cos(angle) * ringRadius
    const z = view.z + Math.sin(angle) * ringRadius
    return [x, 0, z]
  }, [angle, ringRadius, view.x, view.z])

  const body = lot.active ? houseMaterials.activeBody : houseMaterials.idleBody
  const roof = lot.active ? houseMaterials.activeRoof : houseMaterials.idleRoof
  const beaconRef = useRef<THREE.Mesh>(null)
  const windowMatRef = useRef<THREE.MeshBasicMaterial>(null)

  useFrame((state) => {
    const delivered = paused || deliveredMapRef.current.get(lot.slot) === true
    const working = lot.active && delivered
    if (windowMatRef.current) {
      windowMatRef.current.color.set(working ? '#ffe18c' : '#3a4668')
    }
    if (!beaconRef.current) return
    beaconRef.current.visible = working
    if (!working) return
    const t = paused ? 0 : state.clock.elapsedTime
    beaconRef.current.position.y = 1.38 + Math.sin(t * 2.4 + lot.slot) * 0.08
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
      {/* Окна на обеих видимых в изометрии гранях (+Z и +X): загораются только после доставки задачи машиной. */}
      <mesh position={[0, 0.36, 0.4]}>
        <boxGeometry args={[0.32, 0.2, 0.02]} />
        <meshBasicMaterial ref={windowMatRef} color="#3a4668" />
      </mesh>
      <mesh position={[0.4, 0.36, 0]}>
        <boxGeometry args={[0.02, 0.2, 0.32]} />
        <meshBasicMaterial color={lot.active ? '#ffe18c' : '#3a4668'} />
      </mesh>
      {/* Парящий индикатор активности над домом работающего агента (появляется после приезда машины). */}
      {lot.active ? (
        <mesh ref={beaconRef} position={[0, 1.38, 0]} visible={paused}>
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

  // Одна крупная плоская надпись на каждый департамент на внешнем поясе его сектора (читаема без приближения).
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
    const labelR = view.radius * 0.795
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
        y: 0.03,
        z: view.z + Math.sin(angle) * labelR,
        rotZ
      }
    })
  }, [lots, view.x, view.z, view.radius])

  // Выделенные сектора города для каждой группы агентов (расширяются при увеличении числа слотов).
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
    const startOrigin = -Math.PI * 0.5
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
  const innerBoulevardR = Math.max(2.9, ringRadius - 1.25)
  const sectorInnerR = innerBoulevardR + 0.44
  const sectorOuterR = view.radius * 0.845

  return (
    <group>
      {/* Выделенные цветовые сектора групп агентов (начинаются за кольцом Королевы). */}
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
              <ringGeometry args={[5.55, 5.65, 24, 1, thetaStart, thetaLength]} />
              <meshBasicMaterial
                color={sector.accent}
                transparent
                opacity={isSelected ? 0.92 : 0.62}
                depthWrite={false}
              />
            </mesh>
            {[sector.startAngle, sector.endAngle].map((boundAngle, bIdx) => {
              const rMid = (sectorInnerR + 5.62) / 2
              const len = Math.max(0.4, 5.62 - sectorInnerR)
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
          highlighted={selectedDepartment === lot.departmentKey}
          paused={paused}
          onSelectDepartment={onSelectDepartment}
        />
      ))}
      {sectorLabels.map((sector) => (
        <GroundLabel3D
          key={`dept-label-${sector.key}`}
          text={sector.label}
          accentColor={sector.accent}
          position={[sector.x, sector.y, sector.z]}
          size={[3.2, 0.68]}
          rotationZ={sector.rotZ}
          fontSize={48}
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

/** Fixed top-down rig: pans the look target and lerps ortho zoom. */
function CameraRig({
  focus,
  overviewZoom,
  paused
}: {
  focus: { readonly x: number; readonly z: number } | null
  overviewZoom: number
  paused: boolean
}) {
  const { camera, gl } = useThree()
  const look = useRef(new THREE.Vector3(0, 0, 0))
  const desired = useRef(new THREE.Vector3(0, 0, 0))
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
    camera.position.copy(look.current).add(TOP_CAMERA_OFFSET)
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
  droneCount: _droneCount,
  queenActive = true,
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
  queenActive?: boolean
  departmentHouses?: readonly HiveCityHouseLot[]
  selectedDepartment?: string | null
  onSelectDepartment?: ((departmentKey: string) => void) | undefined
  onSelectIsland: (islandId: string) => void
  onSelectBuilding: (islandId: string, buildingId: string) => void
  onHoverBuilding: (buildingId: string | null) => void
}) {
  const groupRef = useRef<THREE.Group>(null)
  const deliveredMapRef = useRef<Map<number, boolean>>(new Map())
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

  return (
    <group ref={groupRef}>
      <IslandGround view={view} selected={selectedIsland} onSelect={() => onSelectIsland(view.island.id)} />
      {/* Уличная сеть города: проспекты, фонари, скверы, светофоры. */}
      <CityStreets3D view={view} skin={skin} paused={paused} />
      {departmentHouses !== undefined && departmentHouses.length > 0 ? (
        <>
          <QueenRoads3D
            view={view}
            lots={departmentHouses}
            skin={skin}
            deliveredMapRef={deliveredMapRef}
            active={queenActive}
            paused={paused}
          />
          <AgentModuleCouriers3D
            view={view}
            lots={departmentHouses}
            deliveredMapRef={deliveredMapRef}
            active={queenActive}
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
  queenActive = true,
  departmentHouses,
  selectedDepartment,
  onSelectDepartment,
  onSelectIsland,
  onSelectBuilding,
  onHoverBuilding,
  onBackgroundClick,
  onContextLost
}: HiveArchipelago3DProps): React.JSX.Element {
  const visibleSatellites = satellites.slice(0, CITY_MAP_MAX_SATELLITES)
  const focus = useMemo(() => {
    if (selectedIslandId === capital.island.id) return { x: 0, z: 0 }
    const selected = visibleSatellites.find((view) => view.island.id === selectedIslandId)
    return selected ? { x: selected.x, z: selected.z } : null
  }, [capital.island.id, selectedIslandId, visibleSatellites])

  const soloZoom = Math.max(8, Math.min(24, (16.5 / Math.max(10, capital.radius)) * OVERVIEW_ZOOM_SOLO))
  const overviewZoom = visibleSatellites.length > 0 ? OVERVIEW_ZOOM_ARCHIPELAGO : soloZoom

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
          position: TOP_CAMERA_OFFSET.toArray(),
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
        <CameraRig focus={focus} overviewZoom={overviewZoom} paused={paused} />
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
          />
          <ConnectingBridges satellites={visibleSatellites} paused={paused} />
        </CitySkinProvider>
      </Canvas>
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
  queenActive = true,
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
  readonly queenActive?: boolean
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
        queenActive={queenActive}
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
