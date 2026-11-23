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
  capitalMainAvenueRadius,
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
const MIN_ZOOM = 8
const MAX_ZOOM = 70

/** Reusable per-frame scratch objects: no 60 Hz allocations inside useFrame. */
const FRAME_DUMMY = new THREE.Object3D()

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
          if (event.button !== 0) return
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
  capital,
  satellites,
  paused: _paused
}: {
  capital: CityIslandView
  satellites: readonly CityIslandView[]
  paused: boolean
}) {
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

  const roadData = useMemo(() => {
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
    <group>
      {roadData.map((road) => (
        <group key={`bridge-group-${road.islandId}`}>
          {/* Несущий пролёт моста между шестиугольными гранями городов-сот. */}
          <mesh
            position={[road.deckCenter[0], -0.09, road.deckCenter[1]]}
            rotation={[0, -road.angle, 0]}
            material={deckMaterial}
          >
            <boxGeometry args={[road.deckLen, 0.18, 1.16]} />
          </mesh>
          {/* Подсветка краёв пролёта цветом города-спутника. */}
          <mesh
            position={[road.deckCenter[0], 0.015, road.deckCenter[1]]}
            rotation={[0, -road.angle, 0]}
          >
            <boxGeometry args={[road.deckLen, 0.025, 0.98]} />
            <meshBasicMaterial color={road.color} transparent opacity={0.45} />
          </mesh>
          {/* Наземная 4-полосная дорога между внешними кольцами столицы и спутника (без фиктивного трафика: 1 агент = 1 машина). */}
          <RoadSegment3D
            from={road.from}
            to={road.to}
            bedMaterial={skin.roadBed}
            curbMaterial={skin.roadCurb}
            lineMaterial={centerLineMaterial}
            width={0.84}
            lanes={4}
            y={0.02}
          />
          {/* Стыки с внешними кольцевыми проспектами обоих городов. */}
          <mesh
            position={[road.junctionCap[0], 0.032, road.junctionCap[1]]}
            rotation={[-Math.PI / 2, 0, -road.angle + Math.PI / 2]}
            material={skin.roadBed}
          >
            <planeGeometry args={[0.78, 0.08]} />
          </mesh>
          <mesh
            position={[road.junctionSat[0], 0.032, road.junctionSat[1]]}
            rotation={[-Math.PI / 2, 0, -road.angle + Math.PI / 2]}
            material={skin.roadBed}
          >
            <planeGeometry args={[0.78, 0.08]} />
          </mesh>
        </group>
      ))}
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
    if (event.button !== 0) return
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
        if ((event.buttons & 4) !== 0) return
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

/** Плоская табличка названия острова у передней грани плиты (смещается в сторону, если грань занята дорогой). */
function IslandNameplate({
  view,
  hasSouthBridge = false
}: {
  view: CityIslandView
  hasSouthBridge?: boolean
}) {
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
    <GroundLabel3D
      text={view.island.name}
      subtitle={subtitle}
      accentColor={accent}
      position={[labelX, 0.025, labelZ]}
      size={[4.8, 1.2]}
      fontSize={34}
    />
  )
}

/**
 * Отрезок дороги (2-полосный для заездных или 4-полосный для магистралей/перемычек):
 * контрастный тёмный асфальт, светлые бордюры по краям, осевая и межполосная разметка.
 */
function RoadSegment3D({
  from,
  to,
  bedMaterial,
  curbMaterial,
  lineMaterial,
  width = 0.52,
  lanes = 2,
  y = 0.025
}: {
  from: readonly [number, number]
  to: readonly [number, number]
  bedMaterial: THREE.Material
  curbMaterial?: THREE.Material
  lineMaterial: THREE.Material
  width?: number
  lanes?: 2 | 4
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
  const curbOffset = lanes === 4 ? width * 0.488 : width * 0.46
  return (
    <group position={[cx, y, cz]} rotation={[0, -angle, 0]}>
      <mesh material={bedMaterial}>
        <boxGeometry args={[length, 0.02, width]} />
      </mesh>
      {/* Левый и правый бордюры. */}
      <mesh position={[0, 0.014, -curbOffset]} material={curbMat}>
        <boxGeometry args={[length, 0.014, 0.032]} />
      </mesh>
      <mesh position={[0, 0.014, curbOffset]} material={curbMat}>
        <boxGeometry args={[length, 0.014, 0.032]} />
      </mesh>
      {/* Межполосная разметка для 4-полосной дороги. */}
      {lanes === 4 ? (
        <>
          <mesh position={[0, 0.015, -width * 0.244]} material={curbMat}>
            <boxGeometry args={[length, 0.012, 0.02]} />
          </mesh>
          <mesh position={[0, 0.015, width * 0.244]} material={curbMat}>
            <boxGeometry args={[length, 0.012, 0.02]} />
          </mesh>
        </>
      ) : null}
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
function CityStreets3D({
  view,
  skin,
  mainAvenueRadius = MAIN_AVENUES_RADIUS,
  paused = false
}: {
  view: CityIslandView
  skin: CitySkin
  mainAvenueRadius?: number
  paused?: boolean
}) {
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
      // Динамические кольцевые проспекты под любой масштаб города (шаг 3.2 между кольцами зданий).
      const rings: number[] = []
      const baseRingR = Math.max(MAIN_AVENUES_RADIUS, mainAvenueRadius)
      for (let r = baseRingR; r <= view.radius * 0.76; r += AVENUE_RING_STEP) {
        rings.push(Number(r.toFixed(2)))
      }
      if (rings.length === 0 || outer - (rings[rings.length - 1] ?? 0) > 1.4) {
        rings.push(Number(outer.toFixed(2)))
      }
      return rings
    }
    return [view.radius * 0.42, outer].filter((r) => r >= 2.2)
  }, [isCapital, mainAvenueRadius, view.radius])

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

  // Межкольцевой соединитель: строго ОДНА дорога между парой колец, максимально удалённая от всех зданий.
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
      const clearanceAt = (angle: number): number => {
        const cosA = Math.cos(angle)
        const sinA = Math.sin(angle)
        let minDist = Infinity
        for (let step = 0; step <= 4; step += 1) {
          const sampleR = startR + (step / 4) * (endR - startR)
          const sx = cosA * sampleR
          const sz = sinA * sampleR
          for (const b of view.buildings) {
            const d = Math.hypot(b.x - sx, b.z - sz)
            if (d < minDist) minDist = d
          }
        }
        return minDist
      }

      let bestAngle = QUEEN_GATE_ANGLE
      let bestMinClearance = isCapital ? clearanceAt(QUEEN_GATE_ANGLE) : -1
      if (bestMinClearance < 1.15) {
        bestMinClearance = -1
        let bestGateDist = Infinity
        for (let slot = 0; slot < 72; slot += 1) {
          const angle = (slot / 72) * Math.PI * 2
          const minDist = clearanceAt(angle)
          const gateDist = isCapital ? angularDistance(angle, QUEEN_GATE_ANGLE) : 0
          if (
            (minDist >= 1.35 && (bestMinClearance < 1.35 || gateDist < bestGateDist)) ||
            (bestMinClearance < 1.35 && minDist > bestMinClearance)
          ) {
            bestMinClearance = minDist
            bestAngle = angle
            bestGateDist = gateDist
          }
        }
      }
      if (bestMinClearance >= 1.15) {
        list.push({
          from: [view.x + Math.cos(bestAngle) * startR, view.z + Math.sin(bestAngle) * startR],
          to: [view.x + Math.cos(bestAngle) * endR, view.z + Math.sin(bestAngle) * endR],
          angle: bestAngle,
          r0: startR,
          r1: endR
        })
      }
    }
    return list
  }, [isCapital, ringRadii, view.buildings, view.x, view.z])

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
      {/* 4-полосные кольцевые проспекты с бордюрами, межполосной и осевой разметкой. */}
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
          {/* Межполосная разметка внутри каждого из 2 направлений. */}
          <mesh position={[view.x, 0.025, view.z]} rotation={[Math.PI / 2, 0, 0]} material={skin.roadCurb}>
            <torusGeometry args={[r - 0.205, 0.012, 6, 72]} />
          </mesh>
          <mesh position={[view.x, 0.025, view.z]} rotation={[Math.PI / 2, 0, 0]} material={skin.roadCurb}>
            <torusGeometry args={[r + 0.205, 0.012, 6, 72]} />
          </mesh>
          <mesh position={[view.x, 0.026, view.z]} rotation={[Math.PI / 2, 0, 0]} material={laneMaterial}>
            <torusGeometry args={[r, 0.03, 6, 72]} />
          </mesh>
          {isCapital ? (
            <RegulatedIntersection3D
              x={view.x}
              z={view.z}
              radius={r}
              angle={QUEEN_GATE_ANGLE}
              bedMaterial={skin.roadBed}
              paused={paused}
            />
          ) : null}
        </group>
      ))}
      {/* Межкольцевые 4-полосные проспекты. */}
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

export const QUEEN_GATE_ANGLE = Math.PI / 2
const TRAFFIC_LIGHT_CYCLE_SEC = 8.0

type TrafficLightState = 'ring_green' | 'amber_to_radial' | 'radial_green' | 'amber_to_ring'

function getTrafficLightState(timeSec: number): TrafficLightState {
  const phase = ((timeSec % TRAFFIC_LIGHT_CYCLE_SEC) + TRAFFIC_LIGHT_CYCLE_SEC) % TRAFFIC_LIGHT_CYCLE_SEC
  if (phase < 3.4) return 'ring_green'
  if (phase < 4.0) return 'amber_to_radial'
  if (phase < 7.4) return 'radial_green'
  return 'amber_to_ring'
}

/** Возвращает true, если разрешён въезд на перекрёсток со стороны кольца. */
function isRingTrafficGreen(timeSec: number): boolean {
  return getTrafficLightState(timeSec) === 'ring_green'
}

/** Возвращает true, если разрешён въезд на перекрёсток со стороны прямой магистрали. */
function isRadialTrafficGreen(timeSec: number): boolean {
  return getTrafficLightState(timeSec) === 'radial_green'
}

/**
 * Регулируемый перекрёсток на пересечении кольца и прямой магистрали от ворот Королевы:
 * разрывает сплошную разметку кольца для легальных разворотов и поворотов, рисует стоп-линии и светофоры.
 */
function RegulatedIntersection3D({
  x,
  z,
  radius,
  angle,
  bedMaterial,
  paused
}: {
  x: number
  z: number
  radius: number
  angle: number
  bedMaterial: THREE.Material
  paused: boolean
}) {
  const stopLineMat = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#e8ecf8',
        transparent: true,
        opacity: 0.88,
        depthWrite: false
      }),
    []
  )
  const poleMat = useMemo(
    () => new THREE.MeshStandardMaterial({ color: '#1b2238', roughness: 0.6, metalness: 0.4 }),
    []
  )
  const ringLampGreenMat = useMemo(() => new THREE.MeshBasicMaterial({ color: '#22c55e' }), [])
  const ringLampAmberMat = useMemo(() => new THREE.MeshBasicMaterial({ color: '#f59e0b' }), [])
  const ringLampRedMat = useMemo(() => new THREE.MeshBasicMaterial({ color: '#ef4444' }), [])
  const radialLampGreenMat = useMemo(() => new THREE.MeshBasicMaterial({ color: '#22c55e' }), [])
  const radialLampAmberMat = useMemo(() => new THREE.MeshBasicMaterial({ color: '#f59e0b' }), [])
  const radialLampRedMat = useMemo(() => new THREE.MeshBasicMaterial({ color: '#ef4444' }), [])
  const ringLampRefs = useRef<Array<THREE.Mesh | null>>([])
  const radialLampRefs = useRef<Array<THREE.Mesh | null>>([])

  useEffect(() => () => {
    stopLineMat.dispose()
    poleMat.dispose()
    ringLampGreenMat.dispose()
    ringLampAmberMat.dispose()
    ringLampRedMat.dispose()
    radialLampGreenMat.dispose()
    radialLampAmberMat.dispose()
    radialLampRedMat.dispose()
  }, [
    stopLineMat,
    poleMat,
    ringLampGreenMat,
    ringLampAmberMat,
    ringLampRedMat,
    radialLampGreenMat,
    radialLampAmberMat,
    radialLampRedMat
  ])

  useFrame((state) => {
    const light = getTrafficLightState(paused ? 0 : state.clock.elapsedTime)
    const rMat =
      light === 'ring_green'
        ? ringLampGreenMat
        : light === 'amber_to_radial' || light === 'amber_to_ring'
          ? ringLampAmberMat
          : ringLampRedMat
    const dMat =
      light === 'radial_green'
        ? radialLampGreenMat
        : light === 'amber_to_radial' || light === 'amber_to_ring'
          ? radialLampAmberMat
          : radialLampRedMat
    for (const lamp of ringLampRefs.current) {
      if (lamp) lamp.material = rMat
    }
    for (const lamp of radialLampRefs.current) {
      if (lamp) lamp.material = dMat
    }
  })

  const ix = x + Math.cos(angle) * radius
  const iz = z + Math.sin(angle) * radius
  // 4 отдельных светофора: 2 перед стоп-линиями кольца, 2 перед стоп-линиями радиальной дороги (отлично видны сверху!).
  const ringPoles: readonly [number, number][] = [
    [-0.51, 0.43],
    [0.51, -0.43]
  ]
  const radialPoles: readonly [number, number][] = [
    [-0.43, -0.51],
    [0.43, 0.51]
  ]

  return (
    <group position={[ix, 0, iz]} rotation={[0, -angle + Math.PI / 2, 0]}>
      {/* Чистая площадь перекрёстка без сплошных линий (место разворотов и поворотов). */}
      <mesh position={[0, 0.039, 0]} rotation={[-Math.PI / 2, 0, 0]} material={bedMaterial}>
        <planeGeometry args={[0.88, 0.88]} />
      </mesh>
      {/* Стоп-линии перед перекрёстком на правой стороне каждого направления. */}
      <mesh position={[-0.21, 0.041, -0.49]} rotation={[-Math.PI / 2, 0, 0]} material={stopLineMat}>
        <planeGeometry args={[0.38, 0.055]} />
      </mesh>
      <mesh position={[0.21, 0.041, 0.49]} rotation={[-Math.PI / 2, 0, 0]} material={stopLineMat}>
        <planeGeometry args={[0.38, 0.055]} />
      </mesh>
      <mesh position={[-0.49, 0.041, 0.21]} rotation={[-Math.PI / 2, 0, Math.PI / 2]} material={stopLineMat}>
        <planeGeometry args={[0.38, 0.055]} />
      </mesh>
      <mesh position={[0.49, 0.041, -0.21]} rotation={[-Math.PI / 2, 0, Math.PI / 2]} material={stopLineMat}>
        <planeGeometry args={[0.38, 0.055]} />
      </mesh>
      {/* Светофоры для кольцевого направления. */}
      {ringPoles.map(([cx, cz], idx) => (
        <group key={`tl-ring-${idx}`} position={[cx, 0, cz]}>
          <mesh position={[0, 0.16, 0]} material={poleMat}>
            <cylinderGeometry args={[0.024, 0.03, 0.32, 6]} />
          </mesh>
          <mesh
            ref={(el) => {
              ringLampRefs.current[idx] = el
            }}
            position={[0, 0.34, 0]}
            material={ringLampGreenMat}
          >
            <sphereGeometry args={[0.062, 10, 10]} />
          </mesh>
        </group>
      ))}
      {/* Светофоры для радиальной магистрали. */}
      {radialPoles.map(([cx, cz], idx) => (
        <group key={`tl-radial-${idx}`} position={[cx, 0, cz]}>
          <mesh position={[0, 0.16, 0]} material={poleMat}>
            <cylinderGeometry args={[0.024, 0.03, 0.32, 6]} />
          </mesh>
          <mesh
            ref={(el) => {
              radialLampRefs.current[idx] = el
            }}
            position={[0, 0.34, 0]}
            material={radialLampRedMat}
          >
            <sphereGeometry args={[0.062, 10, 10]} />
          </mesh>
        </group>
      ))}
    </group>
  )
}
const MAIN_AVENUES_RADIUS = 7.2
const HIVE_CAR_SPEED = 1.65
const AVENUE_RING_STEP = 3.2

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

/**
 * Траектория курьера на Главном кольце по ПДД РФ (правостороннее движение):
 * все выезды и заезды — только направо, пересечение сплошной осевой кольца
 * происходит исключительно в официальном разрыве сплошной `uTurnAngle`.
 */
type PathPose = { x: number; z: number; dx: number; dz: number }

interface PathSegment {
  readonly len: number
  readonly eval: (u: number) => PathPose
}

function approxBezierLength(
  ax: number,
  az: number,
  cx: number,
  cz: number,
  bx: number,
  bz: number
): number {
  return Math.max(
    0.05,
    (Math.hypot(cx - ax, cz - az) + Math.hypot(bx - cx, bz - cz) + Math.hypot(bx - ax, bz - az)) * 0.5
  )
}

function evalSegmentedPose(segments: readonly PathSegment[], t: number): PathPose {
  let total = 0
  for (const seg of segments) {
    if (seg.len > 1e-4) total += seg.len
  }
  if (total <= 1e-4 || segments.length === 0) {
    return segments[0]?.eval(0) ?? { x: 0, z: 0, dx: 0, dz: 1 }
  }
  const target = Math.min(total, Math.max(0, t) * total)
  let acc = 0
  for (const seg of segments) {
    if (seg.len <= 1e-4) continue
    if (target <= acc + seg.len) {
      return seg.eval(Math.min(1, Math.max(0, (target - acc) / seg.len)))
    }
    acc += seg.len
  }
  return segments[segments.length - 1]!.eval(1)
}

function buildMainRingPddSegments(
  viewX: number,
  viewZ: number,
  startAngle: number,
  startR: number,
  endAngle: number,
  endR: number,
  ringR: number,
  uTurnAngle: number,
  startIsInner: boolean,
  laneOffset: number
): readonly PathSegment[] {
  // На перекрёстке у прямой магистрали (startAngle или endAngle совпадает с uTurnAngle)
  // разрешён поворот сразу в нужное полукольцо без лишнего нулевого разворота.
  const ccwArc = posArc(endAngle - startAngle)
  const cwArc = posArc(startAngle - endAngle)
  const chooseCcw =
    Math.abs(posArc(startAngle - uTurnAngle)) < 0.02 || Math.abs(posArc(endAngle - uTurnAngle)) < 0.02
      ? ccwArc <= cwArc
      : startIsInner
  const ringSign = chooseCcw ? 1 : -1
  const laneR = ringR - ringSign * 0.13
  const arc = chooseCcw ? ccwArc : cwArc

  const r1Sign = startIsInner ? 1 : -1
  const r1dx = Math.cos(startAngle) * r1Sign
  const r1dz = Math.sin(startAngle) * r1Sign
  const r2Sign = startIsInner ? 1 : -1
  const r2dx = Math.cos(endAngle) * r2Sign
  const r2dz = Math.sin(endAngle) * r2Sign

  const border1R = startIsInner ? ringR - 0.41 : ringR + 0.41
  const border2R = startIsInner ? ringR + 0.41 : ringR - 0.41
  const lead1 = Math.min(0.1, arc * 0.4)
  const lead2 = Math.min(0.1, arc * 0.4)

  const t1ax = viewX + Math.cos(startAngle) * border1R - r1dz * laneOffset
  const t1az = viewZ + Math.sin(startAngle) * border1R + r1dx * laneOffset
  const t1cx = viewX + Math.cos(startAngle) * laneR - r1dz * laneOffset
  const t1cz = viewZ + Math.sin(startAngle) * laneR + r1dx * laneOffset
  const out1A = startAngle + ringSign * lead1
  const t1bx = viewX + Math.cos(out1A) * laneR
  const t1bz = viewZ + Math.sin(out1A) * laneR

  const arcSpan = Math.max(0, arc - lead1 - lead2)

  const t2InA = endAngle - ringSign * lead2
  const t2Ax = viewX + Math.cos(t2InA) * laneR
  const t2Az = viewZ + Math.sin(t2InA) * laneR
  const t2Cx = viewX + Math.cos(endAngle) * laneR - r2dz * laneOffset
  const t2Cz = viewZ + Math.sin(endAngle) * laneR + r2dx * laneOffset
  const t2Bx = viewX + Math.cos(endAngle) * border2R - r2dz * laneOffset
  const t2Bz = viewZ + Math.sin(endAngle) * border2R + r2dx * laneOffset

  return [
    {
      len: Math.abs(border1R - startR),
      eval: (u) => {
        const r = startR + u * (border1R - startR)
        return {
          x: viewX + Math.cos(startAngle) * r - r1dz * laneOffset,
          z: viewZ + Math.sin(startAngle) * r + r1dx * laneOffset,
          dx: r1dx,
          dz: r1dz
        }
      }
    },
    {
      len: approxBezierLength(t1ax, t1az, t1cx, t1cz, t1bx, t1bz),
      eval: (u) => evalTurnBezier(t1ax, t1az, t1cx, t1cz, t1bx, t1bz, u)
    },
    {
      len: arcSpan * laneR,
      eval: (u) => {
        const curA = out1A + ringSign * u * arcSpan
        return {
          x: viewX + Math.cos(curA) * laneR,
          z: viewZ + Math.sin(curA) * laneR,
          dx: -Math.sin(curA) * ringSign,
          dz: Math.cos(curA) * ringSign
        }
      }
    },
    {
      len: approxBezierLength(t2Ax, t2Az, t2Cx, t2Cz, t2Bx, t2Bz),
      eval: (u) => evalTurnBezier(t2Ax, t2Az, t2Cx, t2Cz, t2Bx, t2Bz, u)
    },
    {
      len: Math.abs(endR - border2R),
      eval: (u) => {
        const r = border2R + u * (endR - border2R)
        return {
          x: viewX + Math.cos(endAngle) * r - r2dz * laneOffset,
          z: viewZ + Math.sin(endAngle) * r + r2dx * laneOffset,
          dx: r2dx,
          dz: r2dz
        }
      }
    }
  ]
}
const QUEEN_PROCESS_SEC = 1.15

interface VehiclePoseState {
  lastX?: number
  lastZ?: number
  lastDirX?: number
  lastDirZ?: number
  lastVisible?: boolean
}

function isVehicleAheadBlocked(
  x: number,
  z: number,
  dirX: number,
  dirZ: number,
  selfIndex: number,
  peers: readonly VehiclePoseState[],
  minGap = 0.56
): boolean {
  for (let i = 0; i < peers.length; i += 1) {
    if (i === selfIndex) continue
    const other = peers[i]
    if (!other || !other.lastVisible) continue
    if (other.lastX === undefined || other.lastZ === undefined) continue
    const dx = other.lastX - x
    const dz = other.lastZ - z
    const dist = Math.hypot(dx, dz)
    if (dist >= minGap) continue
    const dot = dx * dirX + dz * dirZ
    const cross = Math.abs(dx * -dirZ + dz * dirX)
    const sameDir =
      other.lastDirX !== undefined && other.lastDirZ !== undefined
        ? other.lastDirX * dirX + other.lastDirZ * dirZ
        : 1
    const strictlyAhead = dot > 0.04 || (dist <= 0.04 && i < selfIndex)
    if (strictlyAhead && cross < 0.22 && sameDir > 0.25) {
      return true
    }
  }
  return false
}

interface AgentCarSim extends VehiclePoseState {
  phase:
    | 'delivering'
    | 'at_home_briefing'
    | 'to_module'
    | 'at_module'
    | 'from_module'
    | 'at_home_study'
    | 'to_ring'
    | 'outer_to_uturn'
    | 'along_ring'
    | 'queued'
    | 'processing'
    | 'returning_ring'
    | 'cooldown'
  progress: number
  curAngle: number
  cooldown: number
  curLaneOffset: number
  moduleStep: number
  moduleTotalSteps: number
  moduleIdx: number
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
  const mainAvenueR = capitalMainAvenueRadius(lots[0]?.total ?? lots.length)
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

  const moduleBuildings = useMemo(
    () => view.buildings.filter((item) => item.building.kind !== 'citadel'),
    [view.buildings]
  )

  const meshRef = useRef<THREE.InstancedMesh>(null)
  const gateLampRef = useRef<THREE.Mesh>(null)
  const instanceColor = useMemo(() => new THREE.Color(), [])

  const carsRef = useRef<AgentCarSim[]>([])
  const queueRef = useRef<number[]>([])
  const processingLotRef = useRef<{ lotIndex: number; remaining: number } | null>(null)
  const layoutSignatureRef = useRef<string>('')

  useLayoutEffect(() => {
    const layoutSignature = lots
      .map((lot) => `${lot.slot}:${lot.total}:${lot.departmentKey}`)
      .join('|')
    const layoutChanged =
      layoutSignatureRef.current !== layoutSignature || carsRef.current.length !== lots.length

    if (layoutChanged) {
      layoutSignatureRef.current = layoutSignature
      carsRef.current = lots.map((lot, idx) => ({
        phase: lot.active ? 'delivering' : 'cooldown',
        progress: 0,
        curAngle: QUEEN_GATE_ANGLE,
        cooldown: idx * 0.38,
        curLaneOffset: 0.11,
        moduleStep: 0,
        moduleTotalSteps: 1 + (idx % 2),
        moduleIdx: idx % Math.max(1, moduleBuildings.length)
      }))
      queueRef.current = []
      processingLotRef.current = null
    } else {
      lots.forEach((lot, idx) => {
        if (!lot.active) {
          deliveredMapRef.current.delete(lot.slot)
          const car = carsRef.current[idx]
          if (car && car.phase !== 'cooldown') {
            car.phase = 'cooldown'
            car.progress = 0
            car.cooldown = (idx % 6) * 0.28
            const qPos = queueRef.current.indexOf(idx)
            if (qPos >= 0) queueRef.current.splice(qPos, 1)
            if (processingLotRef.current?.lotIndex === idx) {
              processingLotRef.current = null
            }
          }
        }
      })
    }

    const mesh = meshRef.current
    if (!mesh) return
    lots.forEach((lot, index) => {
      instanceColor.set(lot.active ? lot.accent : '#5ea8ff')
      mesh.setColorAt(index, instanceColor)
    })
    if (mesh.instanceColor !== null) mesh.instanceColor.needsUpdate = true
  }, [lots, instanceColor, deliveredMapRef])

  useFrame((state, delta) => {
    const mesh = meshRef.current
    if (!mesh) return
    const dt = paused ? 0 : Math.min(delta, 0.05)
    const elapsedSec = paused ? 0 : state.clock.elapsedTime
    const ringGreen = isRingTrafficGreen(elapsedSec)
    const radialGreen = isRadialTrafficGreen(elapsedSec)
    const dummy = FRAME_DUMMY
    const laneOffset = 0.13
    const gateEntryR = 2.45
    const homeEdgeR = Math.max(innerBoulevardR + 0.35, ringRadius - 0.55)
    const homeGarageR = Math.max(homeEdgeR + 0.35, ringRadius)
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
    const RING_ENTRY_STOP_PROGRESS = computeRingEntryStopProgress(innerBoulevardR, homeGarageR)

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
    const buildQueenOutPath = (
      homeAngle: number
    ): { segments: readonly PathSegment[]; totalLength: number; stopProgress: number } => {
      const gdx = Math.cos(QUEEN_GATE_ANGLE)
      const gdz = Math.sin(QUEEN_GATE_ANGLE)
      const hdx = Math.cos(homeAngle)
      const hdz = Math.sin(homeAngle)
      const arcFromGate = posArc(QUEEN_GATE_ANGLE - homeAngle)
      const outerRForU = (u: number): number => {
        if (arcFromGate <= Math.PI * 0.35) return innerBoulevardR + 0.3
        const s = Math.min(1, Math.max(0, (u - 0.45) / 0.32))
        const smooth = s * s * (3 - 2 * s)
        return innerBoulevardR + 0.13 + smooth * 0.17
      }
      const outerRDerivForU = (u: number): number => {
        if (arcFromGate <= Math.PI * 0.35 || u <= 0.45 || u >= 0.77) return 0
        const s = (u - 0.45) / 0.32
        return ((6 * s * (1 - s)) / 0.32) * 0.17
      }
      const gateTurnR = Math.max(gateEntryR + 0.12, innerBoulevardR - 0.5)
      const lead1 = Math.min(0.14, arcFromGate * 0.4)
      const lead2 = Math.min(0.1, arcFromGate * 0.35)

      const t1ax = view.x + gdx * gateTurnR - gdz * laneOffset
      const t1az = view.z + gdz * gateTurnR + gdx * laneOffset
      const t1cx = view.x + gdx * innerBoulevardR - gdz * laneOffset
      const t1cz = view.z + gdz * innerBoulevardR + gdx * laneOffset
      const out1A = QUEEN_GATE_ANGLE - lead1
      const t1StartLaneR = outerRForU(0)
      const t1bx = view.x + Math.cos(out1A) * t1StartLaneR
      const t1bz = view.z + Math.sin(out1A) * t1StartLaneR

      const outerArcSpan = Math.max(0, arcFromGate - lead1 - lead2)

      const t2InA = homeAngle + lead2
      const t2InLaneR = outerRForU(1)
      const t2Ax = view.x + Math.cos(t2InA) * t2InLaneR
      const t2Az = view.z + Math.sin(t2InA) * t2InLaneR
      const t2Cx = view.x + hdx * t2InLaneR - hdz * laneOffset
      const t2Cz = view.z + hdz * t2InLaneR + hdx * laneOffset
      const rOut = Math.min(homeEdgeR, innerBoulevardR + 0.56)
      const t2Bx = view.x + hdx * rOut - hdz * laneOffset
      const t2Bz = view.z + hdz * rOut + hdx * laneOffset

      const gateApproachLen = Math.max(0.05, gateTurnR - gateEntryR)
      const segments: PathSegment[] = [
        {
          len: gateApproachLen,
          eval: (u) => {
            const r = gateEntryR + u * (gateTurnR - gateEntryR)
            return {
              x: view.x + gdx * r - gdz * laneOffset,
              z: view.z + gdz * r + gdx * laneOffset,
              dx: gdx,
              dz: gdz
            }
          }
        },
        {
          len: approxBezierLength(t1ax, t1az, t1cx, t1cz, t1bx, t1bz),
          eval: (u) => evalTurnBezier(t1ax, t1az, t1cx, t1cz, t1bx, t1bz, u)
        },
        {
          len: outerArcSpan * (innerBoulevardR + 0.22),
          eval: (u) => {
            const curA = QUEEN_GATE_ANGLE - lead1 - outerArcSpan * u
            const laneR = outerRForU(u)
            const dRdu = outerRDerivForU(u)
            const dAdu = -Math.max(0.05, outerArcSpan)
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
        },
        {
          len: approxBezierLength(t2Ax, t2Az, t2Cx, t2Cz, t2Bx, t2Bz),
          eval: (u) => evalTurnBezier(t2Ax, t2Az, t2Cx, t2Cz, t2Bx, t2Bz, u)
        },
        {
          len: Math.max(0.05, homeGarageR - rOut),
          eval: (u) => {
            const r = rOut + u * (homeGarageR - rOut)
            return {
              x: view.x + hdx * r - hdz * laneOffset,
              z: view.z + hdz * r + hdx * laneOffset,
              dx: hdx,
              dz: hdz
            }
          }
        }
      ]
      const totalLength = Math.max(
        1.5,
        segments.reduce((sum, seg) => sum + seg.len, 0)
      )
      return { segments, totalLength, stopProgress: gateApproachLen / totalLength }
    }

    const pickTargetModule = (homeAngle: number, step: number) => {
      if (moduleBuildings.length === 0) return undefined
      const hx = Math.cos(homeAngle) * ringRadius
      const hz = Math.sin(homeAngle) * ringRadius
      const sorted = [...moduleBuildings].sort((a, b) => {
        const da = Math.hypot(a.x - hx, a.z - hz)
        const db = Math.hypot(b.x - hx, b.z - hz)
        return da - db
      })
      return sorted[step % sorted.length]
    }

    const buildHomeToModulePath = (
      homeAngle: number,
      target: { x: number; z: number }
    ): {
      segments: readonly PathSegment[]
      totalLength: number
      ringStopProgress: number
      avenueStopProgress: number
    } => {
      const targetAngle = Math.atan2(target.z, target.x)
      const targetR = Math.hypot(target.x, target.z)
      const targetEntryR = Math.max(mainAvenueR + 0.41, targetR - 0.25)
      const outerLaneR = innerBoulevardR + 0.28
      const innerBorderR = innerBoulevardR + 0.41
      const bridgeLen = Math.max(0.1, mainAvenueR - 0.41 - innerBorderR)
      const outHdx = -Math.cos(homeAngle)
      const outHdz = -Math.sin(homeAngle)
      const outBdx = Math.cos(QUEEN_GATE_ANGLE)
      const outBdz = Math.sin(QUEEN_GATE_ANGLE)
      const outArc = posArc(homeAngle - QUEEN_GATE_ANGLE)
      const seg0Len = Math.max(0.1, homeGarageR - innerBorderR)
      const seg1Len = outArc * outerLaneR
      const segments: PathSegment[] = [
        {
          len: seg0Len,
          eval: (u) => {
            const r = homeGarageR - u * (homeGarageR - innerBorderR)
            return {
              x: view.x + Math.cos(homeAngle) * r - outHdz * laneOffset,
              z: view.z + Math.sin(homeAngle) * r + outHdx * laneOffset,
              dx: outHdx,
              dz: outHdz
            }
          }
        },
        {
          len: seg1Len,
          eval: (u) => {
            const curA = homeAngle - outArc * u
            return {
              x: view.x + Math.cos(curA) * outerLaneR,
              z: view.z + Math.sin(curA) * outerLaneR,
              dx: Math.sin(curA),
              dz: -Math.cos(curA)
            }
          }
        },
        {
          len: bridgeLen,
          eval: (u) => {
            const r = innerBorderR + u * (mainAvenueR - 0.41 - innerBorderR)
            return {
              x: view.x + Math.cos(QUEEN_GATE_ANGLE) * r - outBdz * laneOffset,
              z: view.z + Math.sin(QUEEN_GATE_ANGLE) * r + outBdx * laneOffset,
              dx: outBdx,
              dz: outBdz
            }
          }
        },
        ...buildMainRingPddSegments(
          view.x,
          view.z,
          QUEEN_GATE_ANGLE,
          mainAvenueR - 0.41,
          targetAngle,
          targetEntryR,
          mainAvenueR,
          QUEEN_GATE_ANGLE,
          true,
          laneOffset
        )
      ]
      const totalLength = Math.max(
        1.5,
        segments.reduce((sum, seg) => sum + seg.len, 0)
      )
      const ringStopDist = Math.max(0, seg0Len + seg1Len - 0.48)
      const avenueStopDist = Math.max(ringStopDist + 0.2, seg0Len + seg1Len + bridgeLen - 0.12)
      return {
        segments,
        totalLength,
        ringStopProgress: ringStopDist / totalLength,
        avenueStopProgress: avenueStopDist / totalLength
      }
    }

    const buildModuleToHomePath = (
      homeAngle: number,
      target: { x: number; z: number }
    ): {
      segments: readonly PathSegment[]
      totalLength: number
      avenueStopProgress: number
      ringStopProgress: number
    } => {
      const targetAngle = Math.atan2(target.z, target.x)
      const targetR = Math.hypot(target.x, target.z)
      const targetEntryR = Math.max(mainAvenueR + 0.41, targetR - 0.25)
      const outerLaneR = innerBoulevardR + 0.28
      const innerBorderR = innerBoulevardR + 0.41
      const bridgeLen = Math.max(0.1, mainAvenueR - 0.41 - innerBorderR)
      const inBdx = -Math.cos(QUEEN_GATE_ANGLE)
      const inBdz = -Math.sin(QUEEN_GATE_ANGLE)
      const inHdx = Math.cos(homeAngle)
      const inHdz = Math.sin(homeAngle)
      const inArc = posArc(QUEEN_GATE_ANGLE - homeAngle)
      const mainRingSegs = buildMainRingPddSegments(
        view.x,
        view.z,
        targetAngle,
        targetEntryR,
        QUEEN_GATE_ANGLE,
        mainAvenueR - 0.41,
        mainAvenueR,
        QUEEN_GATE_ANGLE,
        false,
        laneOffset
      )
      const mainRingLen = mainRingSegs.reduce((sum, seg) => sum + seg.len, 0)
      const segments: PathSegment[] = [
        ...mainRingSegs,
        {
          len: bridgeLen,
          eval: (u) => {
            const r = mainAvenueR - 0.41 - u * (mainAvenueR - 0.41 - innerBorderR)
            return {
              x: view.x + Math.cos(QUEEN_GATE_ANGLE) * r - inBdz * laneOffset,
              z: view.z + Math.sin(QUEEN_GATE_ANGLE) * r + inBdx * laneOffset,
              dx: inBdx,
              dz: inBdz
            }
          }
        },
        {
          len: inArc * outerLaneR,
          eval: (u) => {
            const curA = QUEEN_GATE_ANGLE - inArc * u
            return {
              x: view.x + Math.cos(curA) * outerLaneR,
              z: view.z + Math.sin(curA) * outerLaneR,
              dx: Math.sin(curA),
              dz: -Math.cos(curA)
            }
          }
        },
        {
          len: Math.max(0.1, homeGarageR - innerBorderR),
          eval: (u) => {
            const r = innerBorderR + u * (homeGarageR - innerBorderR)
            return {
              x: view.x + Math.cos(homeAngle) * r - inHdz * laneOffset,
              z: view.z + Math.sin(homeAngle) * r + inHdx * laneOffset,
              dx: inHdx,
              dz: inHdz
            }
          }
        }
      ]
      const totalLength = Math.max(
        1.5,
        segments.reduce((sum, seg) => sum + seg.len, 0)
      )
      const avenueStopDist = Math.max(0, mainRingLen - 0.48)
      const ringStopDist = Math.max(avenueStopDist + 0.2, mainRingLen + bridgeLen - 0.12)
      return {
        segments,
        totalLength,
        avenueStopProgress: avenueStopDist / totalLength,
        ringStopProgress: ringStopDist / totalLength
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

      if (car.phase === 'delivering') {
        const queenOutPath = buildQueenOutPath(homeAngle)
        if (car.cooldown > 0) {
          car.cooldown -= dt
          visible = false
        } else {
          const curPose = evalSegmentedPose(queenOutPath.segments, Math.min(1, Math.max(0, car.progress)))
          const aheadBlocked =
            car.lastVisible === true &&
            isVehicleAheadBlocked(curPose.x, curPose.z, curPose.dx, curPose.dz, index, cars)
          if (!aheadBlocked) {
            const nextP = car.progress + dt * (HIVE_CAR_SPEED / queenOutPath.totalLength)
            if (!radialGreen && car.progress <= queenOutPath.stopProgress && nextP >= queenOutPath.stopProgress) {
              car.progress = queenOutPath.stopProgress
            } else {
              car.progress = nextP
            }
          }
          if (car.progress >= 1) {
            deliveredMapRef.current.set(lot.slot, true)
            car.moduleStep = 0
            car.moduleTotalSteps = 1 + (index % 2)
            car.phase = 'at_home_briefing'
            car.progress = 0
            car.cooldown = 0.45
          }
        }
        const t = Math.min(1, Math.max(0, car.progress))
        const pose = evalSegmentedPose(queenOutPath.segments, t)
        x = pose.x
        z = pose.z
        dirX = pose.dx
        dirZ = pose.dz
      } else if (car.phase === 'at_home_briefing') {
        visible = false
        dirX = -Math.cos(homeAngle)
        dirZ = -Math.sin(homeAngle)
        x = view.x + Math.cos(homeAngle) * homeGarageR - dirZ * laneOffset
        z = view.z + Math.sin(homeAngle) * homeGarageR + dirX * laneOffset
        car.cooldown -= dt
        if (car.cooldown <= 0) {
          car.phase = moduleBuildings.length > 0 ? 'to_module' : 'to_ring'
          car.progress = 0
        }
      } else if (car.phase === 'to_module') {
        const target = pickTargetModule(homeAngle, car.moduleStep)
        if (!target) {
          car.phase = 'to_ring'
          car.progress = 0
        } else {
          const modOutPath = buildHomeToModulePath(homeAngle, target)
          const curPose = evalSegmentedPose(modOutPath.segments, Math.min(1, Math.max(0, car.progress)))
          const aheadBlocked =
            car.lastVisible === true &&
            isVehicleAheadBlocked(curPose.x, curPose.z, curPose.dx, curPose.dz, index, cars)
          if (!aheadBlocked) {
            const nextP = car.progress + dt * (HIVE_CAR_SPEED / modOutPath.totalLength)
            if (
              !ringGreen &&
              car.progress <= modOutPath.ringStopProgress &&
              nextP >= modOutPath.ringStopProgress
            ) {
              car.progress = modOutPath.ringStopProgress
            } else if (
              !radialGreen &&
              car.progress <= modOutPath.avenueStopProgress &&
              nextP >= modOutPath.avenueStopProgress
            ) {
              car.progress = modOutPath.avenueStopProgress
            } else {
              car.progress = nextP
            }
          }
          if (car.progress >= 1) {
            car.phase = 'at_module'
            car.progress = 0
            car.cooldown = 0.65
          }
          const pose = evalSegmentedPose(modOutPath.segments, Math.min(1, Math.max(0, car.progress)))
          x = pose.x
          z = pose.z
          dirX = pose.dx
          dirZ = pose.dz
        }
      } else if (car.phase === 'at_module') {
        const target = pickTargetModule(homeAngle, car.moduleStep)
        visible = false
        if (target) {
          const modOutPath = buildHomeToModulePath(homeAngle, target)
          const pose = evalSegmentedPose(modOutPath.segments, 1)
          x = pose.x
          z = pose.z
          dirX = pose.dx
          dirZ = pose.dz
        }
        car.cooldown -= dt
        if (car.cooldown <= 0) {
          car.phase = 'from_module'
          car.progress = 0
        }
      } else if (car.phase === 'from_module') {
        const target = pickTargetModule(homeAngle, car.moduleStep)
        if (!target) {
          car.phase = 'to_ring'
          car.progress = 0
        } else {
          const modInPath = buildModuleToHomePath(homeAngle, target)
          const curPose = evalSegmentedPose(modInPath.segments, Math.min(1, Math.max(0, car.progress)))
          const aheadBlocked =
            car.lastVisible === true &&
            isVehicleAheadBlocked(curPose.x, curPose.z, curPose.dx, curPose.dz, index, cars)
          if (!aheadBlocked) {
            const nextP = car.progress + dt * (HIVE_CAR_SPEED / modInPath.totalLength)
            if (
              !ringGreen &&
              car.progress <= modInPath.avenueStopProgress &&
              nextP >= modInPath.avenueStopProgress
            ) {
              car.progress = modInPath.avenueStopProgress
            } else if (
              !radialGreen &&
              car.progress <= modInPath.ringStopProgress &&
              nextP >= modInPath.ringStopProgress
            ) {
              car.progress = modInPath.ringStopProgress
            } else {
              car.progress = nextP
            }
          }
          if (car.progress >= 1) {
            car.phase = 'at_home_study'
            car.progress = 0
            car.cooldown = 0.75
          }
          const pose = evalSegmentedPose(modInPath.segments, Math.min(1, Math.max(0, car.progress)))
          x = pose.x
          z = pose.z
          dirX = pose.dx
          dirZ = pose.dz
        }
      } else if (car.phase === 'at_home_study') {
        visible = false
        dirX = -Math.cos(homeAngle)
        dirZ = -Math.sin(homeAngle)
        x = view.x + Math.cos(homeAngle) * homeGarageR - dirZ * laneOffset
        z = view.z + Math.sin(homeAngle) * homeGarageR + dirX * laneOffset
        car.cooldown -= dt
        if (car.cooldown <= 0) {
          car.moduleStep += 1
          if (car.moduleStep < car.moduleTotalSteps && moduleBuildings.length > 0) {
            car.phase = 'to_module'
            car.progress = 0
          } else {
            car.phase = 'to_ring'
            car.progress = 0
            car.cooldown = 0.15
          }
        }
      } else if (car.phase === 'to_ring') {
        const hdx = -Math.cos(homeAngle)
        const hdz = -Math.sin(homeAngle)
        if (car.cooldown > 0) {
          car.cooldown -= dt
          visible = false
          dirX = hdx
          dirZ = hdz
          x = view.x + Math.cos(homeAngle) * homeGarageR - dirZ * laneOffset
          z = view.z + Math.sin(homeAngle) * homeGarageR + dirX * laneOffset
        } else {
          // Выезд от дома агента ТОЛЬКО НАПРАВО (-angle) на внешнее полукольцо (без пересечения сплошной!).
          const ringBlocked = cars.some((other, oIdx) => {
            if (oIdx === index) return false
            if (other.lastVisible && other.lastX !== undefined && other.lastZ !== undefined) {
              const entryX = view.x + Math.cos(homeAngle) * (innerBoulevardR + 0.22)
              const entryZ = view.z + Math.sin(homeAngle) * (innerBoulevardR + 0.22)
              if (Math.hypot(other.lastX - entryX, other.lastZ - entryZ) < 0.72) {
                return true
              }
            }
            if (other.phase === 'outer_to_uturn') {
              return angularDistance(other.curAngle, homeAngle) < MIN_CAR_ARC_GAP * 1.25
            }
            if ((other.phase === 'delivering' || other.phase === 'returning_ring') && other.progress >= 0.25 && other.progress < 0.78) {
              const otherHome = houseAngles[oIdx]
              if (otherHome === undefined) return false
              const otherArc = posArc(QUEEN_GATE_ANGLE - otherHome)
              const u = (other.progress - 0.25) / 0.53
              const otherA = QUEEN_GATE_ANGLE - 0.1 - Math.max(0, otherArc - 0.2) * u
              return angularDistance(otherA, homeAngle) < MIN_CAR_ARC_GAP * 1.25
            }
            return false
          })
          const toRingLen = Math.max(0.8, (homeGarageR - innerBoulevardR) + 0.45)
          const nextProgress = car.progress + dt * (HIVE_CAR_SPEED / toRingLen)
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
            const r = homeGarageR - Math.min(1, car.progress) * (homeGarageR - innerBoulevardR)
            dirX = hdx
            dirZ = hdz
            x = view.x + Math.cos(homeAngle) * r - dirZ * laneOffset
            z = view.z + Math.sin(homeAngle) * r + dirX * laneOffset
          } else {
            const turnU = (car.progress - RING_ENTRY_STOP_PROGRESS) / Math.max(0.05, 1 - RING_ENTRY_STOP_PROGRESS)
            const stopR = homeGarageR - RING_ENTRY_STOP_PROGRESS * (homeGarageR - innerBoulevardR)
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
        // Движение по внешнему полукольцу (-angle) до стоп-линии регулируемого перекрёстка и плавный заезд в створ ворот.
        const arcToIntersection = posArc(car.curAngle - QUEEN_GATE_ANGLE)
        const intersectionStopArc = 0.52 / innerBoulevardR
        let maxTravel = arcToIntersection
        if (!ringGreen && arcToIntersection >= intersectionStopArc - 0.015) {
          maxTravel = Math.min(maxTravel, Math.max(0, arcToIntersection - intersectionStopArc))
        }
        for (let oIdx = 0; oIdx < cars.length; oIdx += 1) {
          if (oIdx === index) continue
          const other = cars[oIdx]
          if (!other || other.phase !== 'outer_to_uturn') continue
          const otherArc = posArc(other.curAngle - QUEEN_GATE_ANGLE)
          if (otherArc < arcToIntersection) {
            maxTravel = Math.min(maxTravel, arcToIntersection - otherArc - MIN_CAR_ARC_GAP)
          }
        }
        // Не выезжаем за стоп-линию перекрёстка, если полоса въезда к воротам Королевы уже занята.
        const uTurnBlocked =
          arcToIntersection <= intersectionStopArc + 0.02 &&
          cars.some((other, oIdx) => oIdx !== index && other.phase === 'queued')
        const curRingX = view.x + Math.cos(car.curAngle) * (innerBoulevardR + car.curLaneOffset)
        const curRingZ = view.z + Math.sin(car.curAngle) * (innerBoulevardR + car.curLaneOffset)
        const curRingDx = Math.sin(car.curAngle)
        const curRingDz = -Math.cos(car.curAngle)
        if (
          car.lastVisible &&
          isVehicleAheadBlocked(curRingX, curRingZ, curRingDx, curRingDz, index, cars)
        ) {
          maxTravel = 0
        }
        const outerAngularSpeed = HIVE_CAR_SPEED / Math.max(1.5, innerBoulevardR + car.curLaneOffset)
        const allowedStep = uTurnBlocked ? 0 : Math.min(Math.max(0, maxTravel), dt * outerAngularSpeed)
        car.curAngle -= allowedStep
        const newArcToIntersection = Math.max(0, arcToIntersection - allowedStep)
        if (newArcToIntersection <= 0.01 && !uTurnBlocked) {
          car.phase = 'queued'
          car.progress = 0
          car.curAngle = QUEEN_GATE_ANGLE
          car.curLaneOffset = 0.11
          if (!queue.includes(index)) queue.push(index)
        }
        // Поворот с кольца на полосу въезда в ворота происходит строго ПОСЛЕ стоп-линии перекрёстка.
        const desiredOuterOffset = 0.13
        const uTurnZone =
          newArcToIntersection < intersectionStopArc ? 1 - newArcToIntersection / intersectionStopArc : 0
        if (uTurnZone > 0) {
          const gdx = -Math.cos(QUEEN_GATE_ANGLE)
          const gdz = -Math.sin(QUEEN_GATE_ANGLE)
          const enterR = innerBoulevardR - 0.52
          const ax = view.x + Math.cos(QUEEN_GATE_ANGLE + intersectionStopArc) * (innerBoulevardR + 0.13)
          const az = view.z + Math.sin(QUEEN_GATE_ANGLE + intersectionStopArc) * (innerBoulevardR + 0.13)
          const cx = view.x + Math.cos(QUEEN_GATE_ANGLE) * innerBoulevardR
          const cz = view.z + Math.sin(QUEEN_GATE_ANGLE) * innerBoulevardR
          const bx = view.x + Math.cos(QUEEN_GATE_ANGLE) * enterR - gdz * laneOffset
          const bz = view.z + Math.sin(QUEEN_GATE_ANGLE) * enterR + gdx * laneOffset
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
      } else if (car.phase === 'queued') {
        const gdx = -Math.cos(QUEEN_GATE_ANGLE)
        const gdz = -Math.sin(QUEEN_GATE_ANGLE)
        const enterR = innerBoulevardR - 0.52
        dirX = gdx
        dirZ = gdz
        x = view.x + Math.cos(QUEEN_GATE_ANGLE) * enterR - dirZ * laneOffset
        z = view.z + Math.sin(QUEEN_GATE_ANGLE) * enterR + dirX * laneOffset
      } else if (car.phase === 'along_ring') {
        car.phase = 'queued'
        if (!queue.includes(index)) queue.push(index)
        const gdx = -Math.cos(QUEEN_GATE_ANGLE)
        const gdz = -Math.sin(QUEEN_GATE_ANGLE)
        const enterR = innerBoulevardR - 0.52
        dirX = gdx
        dirZ = gdz
        x = view.x + Math.cos(QUEEN_GATE_ANGLE) * enterR - dirZ * laneOffset
        z = view.z + Math.sin(QUEEN_GATE_ANGLE) * enterR + dirX * laneOffset
      } else if (car.phase === 'processing') {
        const enterR = innerBoulevardR - 0.52
        const procLen = Math.max(0.4, enterR - gateEntryR)
        car.progress = Math.min(1, car.progress + dt * (HIVE_CAR_SPEED / procLen))
        const gdx = -Math.cos(QUEEN_GATE_ANGLE)
        const gdz = -Math.sin(QUEEN_GATE_ANGLE)
        const r = enterR - car.progress * (enterR - gateEntryR)
        dirX = gdx
        dirZ = gdz
        x = view.x + Math.cos(QUEEN_GATE_ANGLE) * r - dirZ * laneOffset
        z = view.z + Math.sin(QUEEN_GATE_ANGLE) * r + dirX * laneOffset
      } else if (car.phase === 'returning_ring') {
        // Возврат по выездной полосе ворот через регулируемый перекрёсток на внешнюю полосу кольца (-angle).
        const queenOutPath = buildQueenOutPath(homeAngle)
        const curPose = evalSegmentedPose(queenOutPath.segments, Math.min(1, Math.max(0, car.progress)))
        const aheadBlocked =
          car.lastVisible === true &&
          isVehicleAheadBlocked(curPose.x, curPose.z, curPose.dx, curPose.dz, index, cars)
        if (!aheadBlocked) {
          const nextP = car.progress + dt * (HIVE_CAR_SPEED / queenOutPath.totalLength)
          if (!radialGreen && car.progress <= queenOutPath.stopProgress && nextP >= queenOutPath.stopProgress) {
            car.progress = queenOutPath.stopProgress
          } else {
            car.progress = nextP
          }
        }
        if (car.progress >= 1) {
          deliveredMapRef.current.set(lot.slot, true)
          car.moduleStep = 0
          car.moduleTotalSteps = 1 + ((car.moduleTotalSteps + 1) % 2)
          car.phase = 'at_home_briefing'
          car.progress = 0
          car.cooldown = 0.55
        }
        const t = Math.min(1, car.progress)
        const pose = evalSegmentedPose(queenOutPath.segments, t)
        x = pose.x
        z = pose.z
        dirX = pose.dx
        dirZ = pose.dz
      } else {
        // Ожидание внутри гаража дома перед следующей поездкой к файлу или к Королеве с отчётом.
        visible = false
        dirX = -Math.cos(homeAngle)
        dirZ = -Math.sin(homeAngle)
        x = view.x + Math.cos(homeAngle) * homeGarageR - dirZ * laneOffset
        z = view.z + Math.sin(homeAngle) * homeGarageR + dirX * laneOffset
        car.cooldown -= dt
        if (car.cooldown <= 0) {
          car.phase = moduleBuildings.length > 0 ? 'to_module' : 'to_ring'
          car.progress = 0
        }
      }

      car.lastVisible = visible
      car.lastX = x
      car.lastZ = z
      car.lastDirX = dirX
      car.lastDirZ = dirZ

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
        width={0.84}
        lanes={4}
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
      {/* Межкольцевой 4-полосный проспект строго по прямой от ворот Королевы (связывает внутреннее кольцо и Главный проспект). */}
      {(() => {
        const bridgeAngle = QUEEN_GATE_ANGLE
        const startR = innerBoulevardR + 0.41
        const endR = Math.max(startR + 0.3, mainAvenueR - 0.41)
        return (
          <group>
            <RoadSegment3D
              from={[view.x + Math.cos(bridgeAngle) * startR, view.z + Math.sin(bridgeAngle) * startR]}
              to={[view.x + Math.cos(bridgeAngle) * endR, view.z + Math.sin(bridgeAngle) * endR]}
              bedMaterial={skin.roadBed}
              curbMaterial={skin.roadCurb}
              lineMaterial={spokeLineMaterial}
              width={0.84}
              lanes={4}
              y={0.023}
            />
            <mesh
              position={[view.x + Math.cos(bridgeAngle) * startR, 0.037, view.z + Math.sin(bridgeAngle) * startR]}
              rotation={[-Math.PI / 2, 0, -bridgeAngle + Math.PI / 2]}
              material={skin.roadBed}
            >
              <planeGeometry args={[0.78, 0.08]} />
            </mesh>
            <mesh
              position={[view.x + Math.cos(bridgeAngle) * endR, 0.037, view.z + Math.sin(bridgeAngle) * endR]}
              rotation={[-Math.PI / 2, 0, -bridgeAngle + Math.PI / 2]}
              material={skin.roadBed}
            >
              <planeGeometry args={[0.78, 0.08]} />
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
      {/* Регулируемый перекрёсток со светофором и разворотом перед воротами Королевы. */}
      <RegulatedIntersection3D
        x={view.x}
        z={view.z}
        radius={innerBoulevardR}
        angle={QUEEN_GATE_ANGLE}
        bedMaterial={skin.roadBed}
        paused={paused}
      />
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
        <planeGeometry args={[0.78, 0.08]} />
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
    readonly activeBody: THREE.MeshStandardMaterial
    readonly activeRoof: THREE.MeshStandardMaterial
    readonly idleBody: THREE.MeshStandardMaterial
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

  const body = lot.active ? houseMaterials.activeBody : houseMaterials.idleBody
  const roof = lot.active ? houseMaterials.activeRoof : houseMaterials.idleRoof
  const beaconRef = useRef<THREE.Mesh>(null)
  const coreRingMatRef = useRef<THREE.MeshBasicMaterial>(null)
  const garageLampMatRef = useRef<THREE.MeshBasicMaterial>(null)

  useFrame((state) => {
    const delivered = paused || deliveredMapRef.current.get(lot.slot) === true
    const working = lot.active && delivered
    if (coreRingMatRef.current) {
      coreRingMatRef.current.color.set(working ? '#ffe6a3' : lot.accent)
      coreRingMatRef.current.opacity = working ? 0.96 : lot.active ? 0.65 : 0.28
    }
    if (garageLampMatRef.current) {
      garageLampMatRef.current.color.set(working ? '#7ef2c4' : '#3a4668')
    }
    if (!beaconRef.current) return
    beaconRef.current.visible = working
    if (!working) return
    const t = paused ? 0 : state.clock.elapsedTime
    beaconRef.current.position.y = 1.08 + Math.sin(t * 2.4 + lot.slot) * 0.06
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
        if (event.button !== 0 || onSelectHouse === undefined) return
        event.stopPropagation()
        onSelectHouse()
      }}
    >
      {/* Шестигранный фундамент-платформа со светящимся контуром департамента. */}
      <mesh position={[0, 0.03, 0]} rotation={[0, Math.PI / 6, 0]}>
        <cylinderGeometry args={[0.56, 0.6, 0.06, 6]} />
        <meshBasicMaterial color="#151d36" />
      </mesh>
      <mesh position={[0, 0.065, 0]} rotation={[0, Math.PI / 6, 0]}>
        <cylinderGeometry args={[0.52, 0.54, 0.018, 6]} />
        <meshBasicMaterial color={lot.accent} transparent opacity={lot.active ? 0.78 : 0.34} />
      </mesh>

      {/* Видимый гараж-въезд со стороны дороги (-X в локальных координатах дома). */}
      <mesh position={[-0.4, 0.16, 0]} material={body}>
        <boxGeometry args={[0.28, 0.24, 0.38]} />
      </mesh>
      <mesh position={[-0.53, 0.14, 0]}>
        <boxGeometry args={[0.04, 0.18, 0.26]} />
        <meshBasicMaterial color="#090d1b" />
      </mesh>
      <mesh position={[-0.44, 0.29, 0]}>
        <boxGeometry args={[0.2, 0.03, 0.32]} />
        <meshBasicMaterial ref={garageLampMatRef} color="#3a4668" />
      </mesh>

      {/* Основной 6-гранный корпус мини-хаба. */}
      <mesh position={[0, 0.3, 0]} rotation={[0, Math.PI / 6, 0]} material={body}>
        <cylinderGeometry args={[0.42, 0.46, 0.46, 6]} />
      </mesh>
      {/* Светящийся акцентный пояс департамента. */}
      <mesh position={[0, 0.55, 0]} rotation={[0, Math.PI / 6, 0]}>
        <cylinderGeometry args={[0.45, 0.45, 0.055, 6]} />
        <meshBasicMaterial color={lot.accent} transparent opacity={lot.active ? 0.92 : 0.42} />
      </mesh>
      {/* Ступенчатая 6-гранная крыша с чётким контуром и цветным знаком департамента сверху. */}
      <mesh position={[0, 0.69, 0]} rotation={[0, Math.PI / 6, 0]} material={roof}>
        <cylinderGeometry args={[0.25, 0.43, 0.22, 6]} />
      </mesh>
      <mesh position={[0, 0.805, 0]} rotation={[0, Math.PI / 6, 0]}>
        <cylinderGeometry args={[0.2, 0.22, 0.025, 6]} />
        <meshBasicMaterial color={lot.accent} transparent opacity={lot.active ? 0.96 : 0.48} />
      </mesh>
      <mesh position={[0, 0.82, 0]} rotation={[Math.PI / 2, 0, Math.PI / 6]}>
        <torusGeometry args={[0.23, 0.028, 6, 6]} />
        <meshBasicMaterial ref={coreRingMatRef} color={lot.accent} transparent opacity={0.5} />
      </mesh>
      {/* Парящий индикатор активности над мини-хабом работающего агента. */}
      {lot.active ? (
        <mesh ref={beaconRef} position={[0, 1.38, 0]} visible={paused}>
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

  // Одна крупная плоская надпись на каждый департамент строго внутри своего сектора (над домами агентов).
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
      {/* Выделенные цветовые сектора групп агентов (начинаются за кольцом Королевы и вмещают дома и подпись). */}
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

  const evaIconTexture = useMemo(() => {
    const canvas = document.createElement('canvas')
    canvas.width = 256
    canvas.height = 256
    const ctx = canvas.getContext('2d')
    if (ctx) {
      // Тёмный медальон-подложка, чтобы 5-лепестковый знак Евы чётко читался поверх золотого маяка.
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
      <mesh position={[0, 0.25, 0]} rotation={[0, Math.PI / 6, 0]} material={skin.plinth}>
        <cylinderGeometry args={[2.3, 2.45, 0.5, 6]} />
      </mesh>
      <mesh position={[0, 0.53, 0]} rotation={[Math.PI / 2, 0, 0]} material={skin.gold}>
        <torusGeometry args={[1.64, 0.055, 8, 32]} />
      </mesh>
      <mesh position={[0, 1.6, 0]} rotation={[0, Math.PI / 6, 0]} material={tierMaterials}>
        <cylinderGeometry args={[1.35, 1.62, 2.2, 6]} />
      </mesh>
      <mesh position={[0, 2.72, 0]} rotation={[Math.PI / 2, 0, 0]} material={skin.gold}>
        <torusGeometry args={[1.28, 0.045, 8, 32]} />
      </mesh>
      <mesh position={[0, 3.62, 0]} rotation={[0, Math.PI / 6, 0]} material={tierMaterials}>
        <cylinderGeometry args={[1.05, 1.22, 1.8, 6]} />
      </mesh>
      <mesh position={[0, 4.54, 0]} rotation={[Math.PI / 2, 0, 0]} material={skin.gold}>
        <torusGeometry args={[1.0, 0.04, 8, 32]} />
      </mesh>
      <mesh position={[0, 5.28, 0]} rotation={[0, Math.PI / 6, 0]} material={tierMaterials}>
        <cylinderGeometry args={[0.8, 0.98, 1.5, 6]} />
      </mesh>
      <mesh position={[0, 6.08, 0]} rotation={[0, Math.PI / 6, 0]} material={skin.gold}>
        <coneGeometry args={[0.72, 1.1, 6]} />
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
      <mesh position={[0, 7.45, 0]} rotation={[-Math.PI / 2, 0, 0]}>
        <planeGeometry args={[1.65, 1.65]} />
        <meshBasicMaterial map={evaIconTexture} transparent depthWrite={false} />
      </mesh>
    </group>
  )
}

/** Fixed top-down rig: pans the look target and lerps ortho zoom without resetting on selection. */
function CameraRig({
  overviewZoom,
  paused
}: {
  overviewZoom: number
  paused: boolean
}) {
  const { camera, gl } = useThree()
  const look = useRef(new THREE.Vector3(0, 0, 0))
  const desired = useRef(new THREE.Vector3(0, 0, 0))
  const desiredZoom = useRef(overviewZoom)
  const panRef = useRef<{ active: boolean; lastX: number; lastY: number }>({
    active: false,
    lastX: 0,
    lastY: 0
  })

  useEffect(() => {
    const dom = gl.domElement
    const host = dom.parentElement ?? dom
    const onWheel = (event: WheelEvent): void => {
      event.preventDefault()
      const scale = event.deltaY < 0 ? 1.12 : 1 / 1.12
      desiredZoom.current = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, desiredZoom.current * scale))
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
  }, [camera, gl])

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
  hasSouthBridge = false,
  onSelectIsland,
  onSelectBuilding,
  onHoverBuilding,
  onBackgroundClick
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
  onHoverBuilding: (buildingId: string | null) => void
  onBackgroundClick: () => void
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
      {/* Уличная сеть города: проспекты, фонари, скверы, светофоры. */}
      <CityStreets3D
        view={view}
        skin={skin}
        mainAvenueRadius={
          departmentHouses !== undefined && departmentHouses.length > 0
            ? capitalMainAvenueRadius(departmentHouses.length)
            : MAIN_AVENUES_RADIUS
        }
        paused={paused}
      />
      {departmentHouses !== undefined && departmentHouses.length > 0 ? (
        <QueenRoads3D
          view={view}
          lots={departmentHouses}
          skin={skin}
          deliveredMapRef={deliveredMapRef}
          active={queenActive}
          paused={paused}
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
  const soloZoom = Math.max(8, Math.min(24, (16.5 / Math.max(10, capital.radius)) * OVERVIEW_ZOOM_SOLO))
  const overviewZoom = visibleSatellites.length > 0 ? OVERVIEW_ZOOM_ARCHIPELAGO : soloZoom

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
        <CameraRig overviewZoom={overviewZoom} paused={paused} />
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
          />
          <ConnectingBridges capital={capital} satellites={visibleSatellites} paused={paused} />
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
  onHoverBuilding,
  onBackgroundClick
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
  readonly onBackgroundClick: () => void
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
        />
      ))}
    </group>
  )
}
