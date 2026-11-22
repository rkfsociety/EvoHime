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
  capitalHouseRingRadius,
  displayIslandHue,
  fnv1a32,
  type CityBuildingView,
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

/** Fixed isometric camera offset (angle never changes; only zoom/pan). */
const CAMERA_OFFSET = new THREE.Vector3(34, 30, 34)

const OVERVIEW_ZOOM = 17
const FOCUS_ZOOM = 28

/** Reusable per-frame scratch object: no 60 Hz allocations inside useFrame. */
const FRAME_DUMMY = new THREE.Object3D()

/**
 * Visual height of a tower: low-weight buildings stay readable instead of
 * collapsing into flat plates (display-only; the data keeps `height_weight_bp`).
 */
function towerDisplayHeight(heightWeight: number): number {
  return Math.max(0.9, heightWeight * 3.8)
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

/**
 * Единый «кожный» набор города: сланцевые фасады с процедурными светящимися
 * окнами (CanvasTexture как `map` + `emissiveMap`), тёмная кровля, плита-
 * основание и позолота Королевы. Материалы разделяются всеми островами и
 * освобождаются при размонтировании Canvas. Окна — детерминированные
 * (`fnv1a32`), поэтому картинка стабильна между перерисовками.
 */
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
    const bases = ['#262e4d', '#232946', '#2a3153', '#252b44']
    context.fillStyle = bases[variant % bases.length] ?? '#262e4d'
    context.fillRect(0, 0, width, height)
    emissiveContext.fillStyle = '#000000'
    emissiveContext.fillRect(0, 0, width, height)
    const litColors = ['#ffca7a', '#ffb45e', '#ffe3ae', '#ffd98a']
    const cellWidth = width / columns
    const cellHeight = height / rows
    for (let row = 0; row < rows; row += 1) {
      for (let column = 0; column < columns; column += 1) {
        const seed = fnv1a32(`facade-${variant}-${row}-${column}`)
        if (seed % 100 < 40) {
          const color = litColors[(seed >>> 7) % litColors.length] ?? '#ffca7a'
          context.fillStyle = color
          emissiveContext.fillStyle = color
        } else {
          context.fillStyle = '#0d1124'
          emissiveContext.fillStyle = '#000000'
        }
        context.fillRect(column * cellWidth + cellWidth * 0.25, row * cellHeight + cellHeight * 0.28, cellWidth * 0.5, cellHeight * 0.44)
        emissiveContext.fillRect(column * cellWidth + cellWidth * 0.25, row * cellHeight + cellHeight * 0.28, cellWidth * 0.5, cellHeight * 0.44)
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
    emissiveIntensity: 1.0,
    roughness: 0.72,
    metalness: 0.2
  })
  return { material, albedo, emissive }
}

/** Shared per-canvas materials of the whole archipelago. */
interface CitySkin {
  readonly facades: readonly FacadeSet[]
  readonly roof: THREE.MeshStandardMaterial
  readonly plinth: THREE.MeshStandardMaterial
  readonly gold: THREE.MeshStandardMaterial
}

function createCitySkin(): CitySkin {
  return {
    facades: Array.from({ length: FACADE_VARIANTS }, (_, index) => createFacadeSet(index)),
    roof: new THREE.MeshStandardMaterial({ color: '#151b30', roughness: 0.85, metalness: 0.1 }),
    plinth: new THREE.MeshStandardMaterial({ color: '#1a2138', roughness: 0.9, metalness: 0.08 }),
    gold: new THREE.MeshStandardMaterial({
      color: new THREE.Color('#e8c874'),
      emissive: new THREE.Color('#e8b34a'),
      emissiveIntensity: 0.45,
      roughness: 0.3,
      metalness: 0.65
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
  }, [skin])
  return <CitySkinContext.Provider value={skin}>{children}</CitySkinContext.Provider>
}

function useCitySkin(): CitySkin {
  const contextSkin = useContext(CitySkinContext)
  // Страховка для рендера вне провайдера (тесты): набор не освобождается,
  // но контекст всегда смонтирован над реальным Canvas-режимом.
  return useMemo(() => contextSkin ?? createCitySkin(), [contextSkin])
}

/** Акцентный свет материалов острова: тонкие световые пояса зданий и кромка плиты. */
function useIslandAccent(hue: number): THREE.MeshStandardMaterial {
  const material = useMemo(
    () =>
      new THREE.MeshStandardMaterial({
        color: new THREE.Color().setHSL(hue / 360, 0.55, 0.5),
        emissive: new THREE.Color().setHSL(hue / 360, 0.75, 0.5),
        emissiveIntensity: 0.7,
        roughness: 0.4,
        metalness: 0.3
      }),
    [hue]
  )
  useEffect(() => () => material.dispose(), [material])
  return material
}

/** Верхняя плита острова: тёмный градиент, на столице — двор и радиальные улицы. */
function createStreetsTexture(view: CityIslandView): THREE.CanvasTexture {
  const size = 512
  const canvas = document.createElement('canvas')
  canvas.width = size
  canvas.height = size
  const context = canvas.getContext('2d')
  if (context !== null) {
    const gradient = context.createRadialGradient(size / 2, size / 2, 24, size / 2, size / 2, size / 2)
    gradient.addColorStop(0, '#182142')
    gradient.addColorStop(0.55, '#131936')
    gradient.addColorStop(1, '#0d1226')
    context.fillStyle = gradient
    context.fillRect(0, 0, size, size)
    if (view.island.role === 'capital') {
      const centre = size / 2
      const ringRadius = (CAPITAL_TOWER_INNER_RADIUS / view.radius) * centre
      // Мягкая площадь под дворцом Королевы.
      context.fillStyle = 'rgba(232, 200, 116, 0.06)'
      context.beginPath()
      context.arc(centre, centre, ringRadius * 0.42, 0, Math.PI * 2)
      context.fill()
      // Магистрали: лучи от площади до кольца кварталов.
      const spokeTotal = 24
      context.strokeStyle = 'rgba(126, 168, 255, 0.14)'
      context.lineWidth = 5
      for (let index = 0; index < spokeTotal; index += 1) {
        const angle = (Math.PI * 2 * index) / spokeTotal
        context.beginPath()
        context.moveTo(centre + Math.cos(angle) * ringRadius * 0.3, centre + Math.sin(angle) * ringRadius * 0.3)
        context.lineTo(centre + Math.cos(angle) * ringRadius, centre + Math.sin(angle) * ringRadius)
        context.stroke()
      }
      // Кольцо двора работников.
      context.strokeStyle = 'rgba(232, 200, 116, 0.22)'
      context.lineWidth = 4
      context.beginPath()
      context.arc(centre, centre, ringRadius, 0, Math.PI * 2)
      context.stroke()
    }
  }
  const texture = new THREE.CanvasTexture(canvas)
  texture.colorSpace = THREE.SRGBColorSpace
  return texture
}

function IslandGround({
  view,
  onSelect
}: {
  view: CityIslandView
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
        color: new THREE.Color().setHSL(accentHue / 360, 0.35, 0.12),
        roughness: 0.9,
        metalness: 0.1
      }),
    [accentHue]
  )
  const rim = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: new THREE.Color().setHSL(accentHue / 360, 0.6, 0.55),
        transparent: true,
        opacity: 0.7
      }),
    [accentHue]
  )
  const rockMaterial = useMemo(
    () => new THREE.MeshStandardMaterial({ color: '#151a2b', roughness: 0.95, metalness: 0.05 }),
    []
  )
  useEffect(() => () => {
    top.dispose()
    side.dispose()
    rim.dispose()
    rockMaterial.dispose()
  }, [top, side, rim, rockMaterial])

  // Скальное основание под плитой: остров читается как парящий осколок
  // земли, а не плоский ромб.
  const rockRefs = useRef<THREE.Group>(null)
  const rocks = useMemo(() => {
    const seed = fnv1a32(`${view.island.id}/rocks`)
    return Array.from({ length: 5 }, (_, index) => {
      const angle = ((seed >>> (index * 5)) % 360) / 180 * Math.PI
      const distance = 0.55 + ((seed >>> (index * 3 + 2)) % 100) / 100 * 0.25
      return {
        x: Math.cos(angle) * view.radius * distance,
        y: -0.7 - ((seed >>> (index * 4 + 1)) % 100) / 100 * view.radius * 0.25,
        z: Math.sin(angle) * view.radius * distance,
        size: 0.5 + ((seed >>> (index * 6)) % 100) / 100 * 0.7,
        rotation: (index * Math.PI) / 5
      }
    })
  }, [view.island.id, view.radius])

  return (
    <group ref={ref}>
      {/* Плита города: шестиугольная, верхняя поверхность с улицами. */}
      <mesh
        position={[0, -0.35, 0]}
        material={[side, top, side]}
        onPointerDown={(event: ThreeEvent<PointerEvent>) => {
          event.stopPropagation()
          onSelect()
        }}
      >
        <cylinderGeometry args={[view.radius, view.radius * 0.92, 0.7, 6]} />
      </mesh>
      {/* Тонкая светящаяся кромка по верхнему краю. */}
      <mesh position={[0, 0.02, 0]} scale={[view.radius * 1.02, 0.035, view.radius * 1.02]} material={rim}>
        <cylinderGeometry args={[1, 1, 1, 6]} />
      </mesh>
      {/* Скальное основание: перевёрнутая усечённая пирамида. */}
      <mesh position={[0, -0.7 - view.radius * 0.22, 0]} rotation={[Math.PI, 0, 0]} material={rockMaterial}>
        <cylinderGeometry args={[view.radius * 0.25, view.radius * 0.88, view.radius * 0.75, 6]} />
      </mesh>
      <group ref={rockRefs}>
        {rocks.map((rock, index) => (
          <mesh key={index} position={[rock.x, rock.y, rock.z]} rotation={[rock.rotation, rock.rotation * 0.7, 0]} material={rockMaterial}>
            <dodecahedronGeometry args={[rock.size, 0]} />
          </mesh>
        ))}
      </group>
    </group>
  )
}

function ConnectingBridges({
  satellites,
  paused
}: {
  satellites: readonly CityIslandView[]
  paused: boolean
}) {
  const pulsesRef = useRef<THREE.InstancedMesh>(null)
  const linesRef = useRef<THREE.Group>(null)
  const pulseMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: new THREE.Color('#9fd0ff'),
        transparent: true,
        opacity: 0.85
      }),
    []
  )
  useEffect(() => () => pulseMaterial.dispose(), [pulseMaterial])

  const bridgeGeometries = useMemo(
    () =>
      satellites.map((view) => {
        const from = new THREE.Vector3(0, 0.6, 0)
        const to = new THREE.Vector3(view.x, 0.6, view.z)
        const mid = from.clone().add(to).multiplyScalar(0.5)
        mid.y += 4
        const curve = new THREE.QuadraticBezierCurve3(from, mid, to)
        return { islandId: view.island.id, curve, length: curve.getLength() }
      }),
    [satellites]
  )
  const pulseCount = Math.max(0, satellites.length)

  useFrame((state) => {
    const pulses = pulsesRef.current
    if (pulses && pulseCount > 0) {
      const dummy = FRAME_DUMMY
      for (let index = 0; index < pulses.count && index < bridgeGeometries.length; index += 1) {
        const bridge = bridgeGeometries[index]
        if (!bridge) continue
        const speed = paused ? 0 : 0.14
        const t = paused ? 0.35 : (state.clock.elapsedTime * (speed * 6)) % 1
        const point = bridge.curve.getPoint(t)
        dummy.position.copy(point)
        dummy.scale.setScalar(0.32)
        dummy.updateMatrix()
        pulses.setMatrixAt(index, dummy.matrix)
      }
      pulses.instanceMatrix.needsUpdate = true
    }
  })

  return (
    <group ref={linesRef}>
      {bridgeGeometries.map((bridge) => (
        <mesh key={`bridge-line-${bridge.islandId}`}>
          <tubeGeometry args={[bridge.curve, 24, 0.07, 6, false]} />
          <meshBasicMaterial color="#4b6ea9" transparent opacity={0.5} />
        </mesh>
      ))}
      <instancedMesh
        ref={pulsesRef}
        args={[undefined, pulseMaterial, pulseCount]}
        visible={pulseCount > 0}
      >
        <sphereGeometry args={[1, 8, 8]} />
      </instancedMesh>
    </group>
  )
}


/**
 * Артикулированная башня одного модуля: детерминированный вариант силуэта
 * (уступы, две башни, шпиль или террасы) из seed `island/building`, светящиеся
 * пояса уровнями и тёплая крыша. Вместо одного процедурного куба — 3-7 мешей;
 * счёт зданий ограничен контрактом карты.
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
  const variant = useMemo(
    () => fnv1a32(`${view.island.id}/${item.building.id}/silhouette`) % 4,
    [view.island.id, item.building.id]
  )
  const height = towerDisplayHeight(item.height)
  const width = 1.05
  const x = view.x + item.x
  const z = view.z + item.z
  // Простая запись слоёв силуэта: [ширина, доля полной высоты, материал, поворот, сдвиг X].
  const layers = useMemo<Array<{ w: number; h: number; accent: boolean; rot: number; y: number; dx: number }>>(() => {
    const band = 0.05
    switch (variant) {
      case 0: // классическая башня с уступами и поясами
      {
        const main = height - band * 2
        return [
          { w: width * 1.08, h: main * 0.55, accent: false, rot: 0, y: main * 0.275, dx: 0 },
          { w: width * 0.86, h: band, accent: true, rot: 0, y: main * 0.55 + band / 2, dx: 0 },
          { w: width * 0.78, h: main * 0.3, accent: false, rot: 0, y: main * 0.55 + band + main * 0.15, dx: 0 },
          { w: width * 0.6, h: band, accent: true, rot: 0, y: main * 0.85 + band / 2, dx: 0 },
          { w: width * 0.52, h: main * 0.15, accent: false, rot: 0, y: main * 0.85 + band + main * 0.075, dx: 0 }
        ]
      }
      case 1: // две башни-близнеца на общем стилобате
      {
        const podium = height * 0.22
        const tower = height - podium
        return [
          { w: width * 1.24, h: podium, accent: false, rot: 0, y: podium / 2, dx: 0 },
          { w: width * 0.52, h: tower, accent: false, rot: 0, y: podium + tower / 2, dx: -width * 0.6 },
          { w: width * 0.52, h: tower * 0.72, accent: false, rot: 0, y: podium + (tower * 0.72) / 2, dx: width * 0.6 }
        ]
      }
      case 2: // плита со шпилем
      {
        const main = height * 0.8
        const spire = height * 0.2
        return [
          { w: width * 1.14, h: main, accent: false, rot: 0, y: main / 2, dx: 0 },
          { w: width * 0.7, h: main * 0.12, accent: true, rot: 0, y: main + main * 0.06, dx: 0 },
          { w: width * 0.16, h: spire, accent: false, rot: 0, y: main + main * 0.12 + spire / 2, dx: 0 }
        ]
      }
      default: // террасный холм
      {
        const step = height / 3
        return [
          { w: width * 1.24, h: step, accent: false, rot: Math.PI / 4, y: step / 2, dx: 0 },
          { w: width * 0.94, h: step, accent: false, rot: Math.PI / 4, y: step * 1.5, dx: 0 },
          { w: width * 0.6, h: step, accent: true, rot: Math.PI / 4, y: step * 2.5, dx: 0 }
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
  // Материалы бокса: боковые грани — фасад с окнами, крыша — тёмная.
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
      {lit ? (
        <mesh position={[0, height / 2 + 0.05, 0]}>
          <boxGeometry args={[width * 1.24, height + 0.3, width * 1.24]} />
          <meshBasicMaterial color="#ffd98a" wireframe transparent opacity={0.35} />
        </mesh>
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

/** Text plates as CanvasTexture sprites; textures are disposed on unmount. */
function IslandNameplate({ view }: { view: CityIslandView }) {
  const texture = useMemo(() => {
    const canvas = document.createElement('canvas')
    canvas.width = 256
    canvas.height = 64
    const context = canvas.getContext('2d')
    if (context) {
      const radius = 12
      context.fillStyle = 'rgba(9, 11, 22, 0.78)'
      if (typeof context.roundRect === 'function') {
        context.beginPath()
        context.roundRect(2, 2, canvas.width - 4, canvas.height - 4, radius)
        context.fill()
        context.strokeStyle = `hsl(${displayIslandHue(view.island.id, view.island.role)} 72% 62% / 0.75)`
        context.lineWidth = 2
        context.stroke()
      } else {
        context.fillRect(0, 0, canvas.width, canvas.height)
        context.strokeStyle = `hsl(${displayIslandHue(view.island.id, view.island.role)}, 72%, 62%)`
        context.lineWidth = 2
        context.strokeRect(2, 2, canvas.width - 4, canvas.height - 4)
      }
      context.fillStyle = '#e6ecff'
      context.textAlign = 'center'
      context.textBaseline = 'middle'
      // Ужимаем кегль, чтобы длинные имена репозиториев не обрезались краем.
      let fontPx = 26
      context.font = `600 ${fontPx}px system-ui, sans-serif`
      const maxWidth = canvas.width - 32
      while (fontPx > 14 && context.measureText(view.island.name).width > maxWidth) {
        fontPx -= 2
        context.font = `600 ${fontPx}px system-ui, sans-serif`
      }
      context.fillText(view.island.name, canvas.width / 2, canvas.height / 2 + 2, maxWidth)
    }
    const created = new THREE.CanvasTexture(canvas)
    created.colorSpace = THREE.SRGBColorSpace
    return created
  }, [view.island.name, view.island.id, view.island.role])

  useEffect(() => () => texture.dispose(), [texture])

  return (
    <sprite position={[view.x, 3.0, view.z + view.radius + 3.4]} scale={[6, 1.5, 1]}>
      <spriteMaterial map={texture} transparent depthWrite={false} />
    </sprite>
  )
}

/** Плоский отрезок дороги на плите острова: тонкий бокс, выровненный по прямой. */
function GroundSegment({
  from,
  to,
  material,
  width = 0.18
}: {
  from: readonly [number, number]
  to: readonly [number, number]
  material: THREE.Material
  width?: number
}) {
  const dx = to[0] - from[0]
  const dz = to[1] - from[1]
  const length = Math.hypot(dx, dz)
  if (length < 0.05) return <></>
  const angle = Math.atan2(dz, dx)
  return (
    <mesh
      position={[(from[0] + to[0]) / 2, 0.035, (from[1] + to[1]) / 2]}
      rotation={[0, -angle, 0]}
      material={material}
    >
      <boxGeometry args={[length, 0.035, width]} />
    </mesh>
  )
}

interface InfoPacket {
  readonly color: string
  /** 0 = старт, 1 = финиш; направление зашито в позиционной формуле. */
  readonly speed: number
  readonly phase: number
  readonly lot: number
  readonly kind: 'to-queen' | 'from-queen' | 'ring'
}

/**
 * Дороги столицы и движение информации: лучи от двора Королевы к домам
 * агентов, кольцевая дорога между домами и светящиеся пакеты, бегающие в обе
 * стороны (агент → Королева, Королева → агент, агент → агент).
 */
function QueenRoads3D({
  view,
  lots,
  paused
}: {
  view: CityIslandView
  lots: readonly HiveCityHouseLot[]
  paused: boolean
}) {
  const ringRadius = Math.min(
    capitalHouseRingRadius(lots[0]?.total ?? 0),
    CAPITAL_TOWER_INNER_RADIUS - 1.2
  )
  const spokeRoadMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#f0d088',
        transparent: true,
        opacity: 0.4,
        depthWrite: false
      }),
    []
  )
  const ringRoadMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#f0d088',
        transparent: true,
        opacity: 0.3,
        depthWrite: false
      }),
    []
  )
  useEffect(() => () => {
    spokeRoadMaterial.dispose()
    ringRoadMaterial.dispose()
  }, [spokeRoadMaterial, ringRoadMaterial])

  const packets = useMemo<readonly InfoPacket[]>(() => {
    const list: InfoPacket[] = []
    lots.forEach((lot, lotIndex) => {
      const seed = fnv1a32(`${view.island.id}/road-${lot.departmentKey}-${lot.slot}`)
      list.push({
        color: lot.accent,
        speed: 0.18 + (seed % 100) / 1000,
        phase: (seed % 997) / 997,
        lot: lotIndex,
        kind: 'to-queen'
      })
      const backSeed = fnv1a32(`${view.island.id}/road-${lot.departmentKey}-${lot.slot}-back`)
      list.push({
        color: '#ffe9b0',
        speed: 0.18 + (backSeed % 100) / 1000,
        phase: (backSeed % 997) / 997,
        lot: lotIndex,
        kind: 'from-queen'
      })
      const ringSeed = fnv1a32(`${view.island.id}/road-${lot.departmentKey}-${lot.slot}-ring`)
      list.push({
        color: lot.accent,
        speed: 0.1 + (ringSeed % 60) / 1000,
        phase: (ringSeed % 997) / 997,
        lot: lotIndex,
        kind: 'ring'
      })
    })
    return list
  }, [lots, view.island.id])

  const houseAngles = useMemo(
    () =>
      lots.map((lot) => (lot.slot / Math.max(1, lot.total)) * Math.PI * 2),
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
    packets.forEach((packet, index) => {
      if (index >= mesh.count) return
      const angle = houseAngles[packet.lot]
      if (angle === undefined) return
      let x: number
      let z: number
      const progress = paused ? packet.phase : (packet.phase + time * packet.speed) % 1
      if (packet.kind === 'ring') {
        const nextAngle =
          ((packet.lot + 1 < lots.length ? houseAngles[packet.lot + 1] : (houseAngles[0] ?? angle) + Math.PI * 2) ?? angle)
        const target = nextAngle > angle ? nextAngle : nextAngle + Math.PI * 2
        const current = angle + (target - angle) * progress
        x = view.x + Math.cos(current) * ringRadius
        z = view.z + Math.sin(current) * ringRadius
        dummy.scale.setScalar(0.13)
      } else {
        const roadStart = 2.5
        const outward = packet.kind === 'from-queen' ? progress : 1 - progress
        const radius = roadStart + outward * (ringRadius - roadStart)
        x = view.x + Math.cos(angle) * radius
        z = view.z + Math.sin(angle) * radius
        dummy.scale.setScalar(0.17)
      }
      dummy.position.set(x, 0.22, z)
      dummy.updateMatrix()
      mesh.setMatrixAt(index, dummy.matrix)
    })
    mesh.instanceMatrix.needsUpdate = true
  })

  if (lots.length === 0) return <></>

  return (
    <group>
      {/* Лучи от двора к каждому дому агента. */}
      {lots.map((lot, index) => {
        const angle = houseAngles[index]
        if (angle === undefined) return null
        return (
          <GroundSegment
            key={`road-${lot.departmentKey}-${lot.slot}`}
            from={[view.x + Math.cos(angle) * 2.5, view.z + Math.sin(angle) * 2.5]}
            to={[view.x + Math.cos(angle) * ringRadius, view.z + Math.sin(angle) * ringRadius]}
            material={spokeRoadMaterial}
          />
        )
      })}
      {/* Кольцевая дорога между домами агентов. */}
      <mesh position={[view.x, 0.035, view.z]} rotation={[Math.PI / 2, 0, 0]} material={ringRoadMaterial}>
        <torusGeometry args={[ringRadius, 0.05, 6, 48]} />
      </mesh>
      {/* Пакеты информации вдоль лучей и кольца. */}
      <instancedMesh ref={meshRef} args={[undefined, undefined, Math.max(1, packets.length)]} visible={packets.length > 0} frustumCulled={false}>
        <sphereGeometry args={[1, 6, 6]} />
        <meshBasicMaterial color="#ffffff" transparent opacity={0.9} />
      </instancedMesh>
    </group>
  )
}

/** Шоссе между зданиями модулей: связи данных острова как дороги с пакетами. */
function ModuleRoads3D({
  view,
  paused
}: {
  view: CityIslandView
  paused: boolean
}) {
  const roads = useMemo(() => {
    const byId = new Map(view.buildings.map((item) => [item.building.id, item]))
    const edges = [...view.island.edges]
      .sort((left, right) => right.weight - left.weight)
      .slice(0, 40)
    const segments: Array<{ from: [number, number]; to: [number, number]; width: number }> = []
    for (const edge of edges) {
      const from = byId.get(edge.from_id)
      const to = byId.get(edge.to_id)
      if (from === undefined || to === undefined) continue
      segments.push({
        from: [view.x + from.x, view.z + from.z],
        to: [view.x + to.x, view.z + to.z],
        width: 0.07 + (edge.weight / 100) * 0.13
      })
    }
    return segments
  }, [view])

  const roadMaterial = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: '#9db4e8',
        transparent: true,
        opacity: 0.3,
        depthWrite: false
      }),
    []
  )
  useEffect(() => () => roadMaterial.dispose(), [roadMaterial])

  const meshRef = useRef<THREE.InstancedMesh>(null)
  const pulseMaterial = useMemo(
    () => new THREE.MeshBasicMaterial({ color: '#ffffff', transparent: true, opacity: 0.85 }),
    []
  )
  useEffect(() => () => pulseMaterial.dispose(), [pulseMaterial])

  useFrame((state) => {
    const mesh = meshRef.current
    if (!mesh) return
    const time = paused ? 0 : state.clock.elapsedTime
    const dummy = FRAME_DUMMY
    roads.forEach((road, index) => {
      if (index >= mesh.count) return
      const seed = fnv1a32(`${view.island.id}/pulse-${index}`)
      const speed = 0.14 + (seed % 80) / 1000
      const progress = paused ? ((seed % 997) / 997) : ((seed % 997) / 997 + time * speed) % 1
      dummy.position.set(
        road.from[0] + (road.to[0] - road.from[0]) * progress,
        0.14,
        road.from[1] + (road.to[1] - road.from[1]) * progress
      )
      dummy.scale.setScalar(0.11)
      dummy.updateMatrix()
      mesh.setMatrixAt(index, dummy.matrix)
    })
    mesh.instanceMatrix.needsUpdate = true
  })

  if (roads.length === 0) return <></>

  return (
    <group>
      {roads.map((road, index) => (
        <GroundSegment
          key={`module-road-${index}`}
          from={road.from}
          to={road.to}
          material={roadMaterial}
          width={road.width}
        />
      ))}
      <instancedMesh
        ref={meshRef}
        args={[undefined, pulseMaterial, Math.max(1, roads.length)]}
        visible={roads.length > 0}
        frustumCulled={false}
      >
        <sphereGeometry args={[1, 6, 6]} />
      </instancedMesh>
    </group>
  )
}

/** One worker house on the ring around the queen tower; lit when active. */
function DepartmentHouse3D({
  view,
  lot,
  houseMaterials,
  highlighted,
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
  onSelectDepartment: ((departmentKey: string) => void) | undefined
}) {
  const angle = (lot.slot / lot.total) * Math.PI * 2
  // Дома работников живут на кольце между башней Королевы (центр) и
  // кварталами проекта, внутри внутренней зоны столицы.
  const ringRadius = Math.min(capitalHouseRingRadius(lot.total), CAPITAL_TOWER_INNER_RADIUS - 1.2)
  const position = useMemo<[number, number, number]>(() => {
    const x = view.x + Math.cos(angle) * ringRadius
    const z = view.z + Math.sin(angle) * ringRadius
    return [x, 0.05, z]
  }, [angle, ringRadius, view.x, view.z])
  const body = lot.active ? houseMaterials.activeBody : houseMaterials.idleBody
  const roof = lot.active ? houseMaterials.activeRoof : houseMaterials.idleRoof

  return (
    <group
      position={position}
      // R3F трактует dashed-props как вложенные пути (`data-dept` → group.data.dept)
      // и падает при применении; DOM-атрибуты трёхмерным объектам назначаются
      // только через userData.
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
      <mesh position={[0, 0.03, 0]}>
        <boxGeometry args={[1.35, 0.06, 1.35]} />
        <meshBasicMaterial color={lot.accent} transparent opacity={lot.active ? 0.16 : 0.07} />
      </mesh>
      <mesh material={body}>
        <boxGeometry args={[1.05, 0.68, 1.05]} />
      </mesh>
      <mesh
        material={roof}
        position={[0, 0.34 + 0.42, 0]}
        rotation={[0, Math.PI / 4, 0]}
      >
        <coneGeometry args={[0.92, 0.6, 4]} />
      </mesh>
      {/* Труба: детерминированная сторона крыши. */}
      <mesh position={[0.28, 0.34 + 0.42, -0.28]}>
        <boxGeometry args={[0.16, 0.34, 0.16]} />
        <meshStandardMaterial color="#181c2c" roughness={0.9} />
      </mesh>
      {/* Тёплое окно: светится у активного работника. */}
      <mesh position={[0, 0.34, 0.53]}>
        <boxGeometry args={[0.34, 0.2, 0.02]} />
        <meshBasicMaterial color={lot.active ? '#ffd98a' : '#242a40'} />
      </mesh>
      {highlighted ? (
        <mesh position={[0, 0.34, 0]}>
          <boxGeometry args={[1.35, 0.8, 1.35]} />
          <meshBasicMaterial color="#ffe9b0" wireframe transparent opacity={0.6} />
        </mesh>
      ) : null}
    </group>
  )
}

/** Маяк спутника: мягкое дышащее гало над центром острова без цитадели. */
function BeaconGlow3D({ view }: { view: CityIslandView }) {
  const coreRef = useRef<THREE.Mesh>(null)
  const haloRef = useRef<THREE.Mesh>(null)
  useFrame((state) => {
    const t = state.clock.elapsedTime
    if (coreRef.current) coreRef.current.scale.setScalar(0.36 + 0.06 * Math.sin(t * 1.9))
    if (haloRef.current) haloRef.current.scale.setScalar(1 + 0.07 * Math.sin(t * 1.2))
  })
  return (
    <group position={[view.x, 1.7, view.z]}>
      <mesh ref={coreRef}>
        <sphereGeometry args={[0.42, 10, 10]} />
        <meshBasicMaterial color="#ffe9b0" transparent opacity={0.85} />
      </mesh>
      <mesh ref={haloRef}>
        <sphereGeometry args={[0.95, 12, 12]} />
        <meshBasicMaterial color="#8d82e8" transparent opacity={0.14} />
      </mesh>
    </group>
  )
}

/** Starfield: static deterministic points on a far shell; no motion cost. */
function Starfield(): React.JSX.Element | null {
  const geometry = useMemo(() => {
    const positions = new Float32Array(300 * 3)
    for (let index = 0; index < 300; index += 1) {
      const seedA = (fnv1a32(`star-a-${index}`) % 3600) / 3600 * Math.PI * 2
      const seedB = (fnv1a32(`star-b-${index}`) % 1000) / 1000
      const radius = 130 + seedB * 60
      const y = (fnv1a32(`star-y-${index}`) % 1000) / 1000 * 90 + 6
      positions[index * 3] = Math.cos(seedA) * radius
      positions[index * 3 + 1] = y
      positions[index * 3 + 2] = Math.sin(seedA) * radius
    }
    const geometry = new THREE.BufferGeometry()
    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3))
    return geometry
  }, [])
  useEffect(() => () => geometry.dispose(), [geometry])
  return (
    <points geometry={geometry}>
      <pointsMaterial color="#9fb4ff" size={1.4} sizeAttenuation={false} transparent opacity={0.6} />
    </points>
  )
}

/** Gable houses of one island: every hive worker capacity slot gets its own. */
function DepartmentHouses3D({
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
  const houseMaterials = useMemo(
    () => ({
      activeBody: new THREE.MeshStandardMaterial({
        color: new THREE.Color('#39415c'),
        roughness: 0.6,
        metalness: 0.1
      }),
      activeRoof: new THREE.MeshStandardMaterial({
        color: new THREE.Color('#e0b341'),
        emissive: new THREE.Color('#e0b341'),
        emissiveIntensity: 0.75,
        roughness: 0.4
      }),
      idleBody: new THREE.MeshStandardMaterial({
        color: new THREE.Color('#232838'),
        roughness: 0.75
      }),
      idleRoof: new THREE.MeshStandardMaterial({
        color: new THREE.Color('#3c4359'),
        roughness: 0.7
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
          onSelectDepartment={onSelectDepartment}
        />
      ))}
    </group>
  )
}

/**
 * Шпиль Королевы: восьмигранные ярусы со светящимися фасадами, золотые пояса,
 * конус-корона, кристалл-маяк с пульсом и вертикальный луч света. Доминирует
 * над кварталами, но не превращается в цветное пятно: тело — сланец, золото —
 * только пояса и корона.
 */
function QueenTower3D({ view, skin }: { view: CityIslandView; skin: CitySkin }) {
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

  // Пульс маяка: дыхание кристалла, гало и луча, останавливается на паузе.
  useFrame((state) => {
    const t = state.clock.elapsedTime
    if (beaconRef.current) {
      beaconRef.current.scale.setScalar(1 + 0.14 * Math.sin(t * 2.1))
    }
    if (haloRef.current) {
      haloRef.current.scale.setScalar(1 + 0.08 * Math.sin(t * 1.3))
    }
    if (beamRef.current) {
      const material = beamRef.current.material as THREE.MeshBasicMaterial
      material.opacity = 0.1 + 0.05 * Math.sin(t * 1.7)
    }
  })

  return (
    <group position={[plan.x, 0, plan.z]}>
      {/* Шестиугольный стилобат с золотым ободом. */}
      <mesh position={[0, 0.25, 0]} material={skin.plinth}>
        <cylinderGeometry args={[2.3, 2.4, 0.5, 6]} />
      </mesh>
      <mesh position={[0, 0.53, 0]} rotation={[Math.PI / 2, 0, 0]} material={skin.gold}>
        <torusGeometry args={[1.62, 0.05, 8, 32]} />
      </mesh>
      {/* Три восьмигранных яруса, сужающихся кверху. */}
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
      {/* Корона: золотой конус и кристалл-маяк. */}
      <mesh position={[0, 6.08, 0]} material={skin.gold}>
        <coneGeometry args={[0.72, 1.1, 8]} />
      </mesh>
      <mesh ref={beaconRef} position={[0, 6.95, 0]}>
        <icosahedronGeometry args={[0.3, 0]} />
        <meshBasicMaterial color="#ffe9b0" />
      </mesh>
      <mesh ref={haloRef} position={[0, 6.95, 0]}>
        <sphereGeometry args={[0.7, 14, 14]} />
        <meshBasicMaterial color="#ffe9b0" transparent opacity={0.16} depthWrite={false} />
      </mesh>
      <mesh ref={beamRef} position={[0, 9.4, 0]}>
        <cylinderGeometry args={[0.09, 0.14, 4.6, 6, 1, true]} />
        <meshBasicMaterial color="#ffd98a" transparent opacity={0.12} blending={THREE.AdditiveBlending} depthWrite={false} />
      </mesh>
    </group>
  )
}

/** Agent drones orbit the capital; heuristic observer-only visualization. */
function AgentDrones({ count, paused }: { count: number; paused: boolean }) {
  const ref = useRef<THREE.InstancedMesh>(null)
  const material = useMemo(
    () => new THREE.MeshBasicMaterial({ color: new THREE.Color('#7ef2c4') }),
    []
  )
  useEffect(() => () => material.dispose(), [material])

  const seeds = useMemo(() => {
    const bounded = Math.max(0, Math.min(64, count))
    return Array.from({ length: bounded }, (_, index) => ({
      radius: 16 + ((fnv1a32(`drone-${index}`) % 60) / 60) * 10,
      speed: 0.5 + ((fnv1a32(`drone-speed-${index}`) % 100) / 100) * 0.8,
      phase: (fnv1a32(`drone-phase-${index}`) % 360) / 360 * Math.PI * 2,
      height: 2.5 + ((fnv1a32(`drone-height-${index}`) % 50) / 50) * 5
    }))
  }, [count])

  useFrame((state) => {
    const mesh = ref.current
    if (!mesh) return
    const dummy = FRAME_DUMMY
    const t = paused ? 0 : state.clock.elapsedTime
    for (let index = 0; index < mesh.count && index < seeds.length; index += 1) {
      const seed = seeds[index]
      if (!seed) continue
      const angle = seed.phase + t * seed.speed
      dummy.position.set(Math.cos(angle) * seed.radius, seed.height, Math.sin(angle) * seed.radius)
      dummy.scale.setScalar(0.32)
      dummy.updateMatrix()
      mesh.setMatrixAt(index, dummy.matrix)
    }
    mesh.instanceMatrix.needsUpdate = true
  })

  if (seeds.length === 0) return <></>
  return (
    <instancedMesh ref={ref} args={[undefined, material, seeds.length]}>
      <sphereGeometry args={[1, 6, 6]} />
    </instancedMesh>
  )
}

/** Fixed-angle isometric rig: pans the look target and lerps the ortho zoom. */
function CameraRig({ focus, paused }: { focus: { readonly x: number; readonly z: number } | null; paused: boolean }) {
  const { camera } = useThree()
  const look = useRef(new THREE.Vector3(0, 0, 0))
  const desired = useRef(new THREE.Vector3(0, 0, 0))
  const desiredZoom = useRef(OVERVIEW_ZOOM)

  useEffect(() => {
    if (focus) {
      desired.current.set(focus.x, 0, focus.z)
      desiredZoom.current = FOCUS_ZOOM
    } else {
      desired.current.set(0, 0, 0)
      desiredZoom.current = OVERVIEW_ZOOM
    }
  }, [focus])

  useFrame((_, delta) => {
    const factor = paused ? 1 : 1 - Math.exp(-5 * Math.min(delta, 0.1))
    look.current.lerp(desired.current, factor)
    camera.position.copy(look.current).add(CAMERA_OFFSET)
    camera.lookAt(look.current)
    const ortho = camera as THREE.OrthographicCamera
    if (typeof ortho.zoom === 'number') {
      const nextZoom = ortho.zoom + (desiredZoom.current - ortho.zoom) * factor
      ortho.zoom = nextZoom
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
  /** Слоты-дома работников; задаются только для столицы. */
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

  return (
    <group ref={groupRef}>
      <IslandGround view={view} onSelect={() => onSelectIsland(view.island.id)} />
      {/* Дороги под застройкой: связи модулей и двор Королевы. */}
      <ModuleRoads3D view={view} paused={paused} />
      {departmentHouses !== undefined && departmentHouses.length > 0 ? (
        <QueenRoads3D view={view} lots={departmentHouses} paused={paused} />
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
        <QueenTower3D view={view} skin={skin} />
      ) : (
        <BeaconGlow3D view={view} />
      )}
      {departmentHouses !== undefined && departmentHouses.length > 0 ? (
        <DepartmentHouses3D
          view={view}
          lots={departmentHouses}
          selectedDepartment={selectedDepartment ?? null}
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
  const visibleSatellites = satellites.slice(0, CITY_MAP_MAX_SATELLITES)
  const focus = useMemo(() => {
    if (selectedIslandId === capital.island.id) return { x: 0, z: 0 }
    const selected = visibleSatellites.find((view) => view.island.id === selectedIslandId)
    return selected ? { x: selected.x, z: selected.z } : null
  }, [capital.island.id, selectedIslandId, visibleSatellites])

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
          zoom: OVERVIEW_ZOOM,
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
        <color attach="background" args={['#070812']} />
        <ambientLight intensity={0.3} />
        <hemisphereLight args={['#4b5da0', '#0a0c16', 0.5]} />
        {/* Холодное «лунное» освещение: тепло дают только окна и золото. */}
        <directionalLight position={[26, 38, 14]} intensity={1.0} color="#dfe8ff" />
        <directionalLight position={[-20, 16, -18]} intensity={0.3} color="#7aa0ff" />
        {/* Тёплый источник над дворцом Королевы: город читается объёмом. */}
        <pointLight position={[0, 12, 0]} intensity={40} distance={50} decay={1.6} color="#ffd98a" />
        <CameraRig focus={focus} paused={paused} />
        <Starfield />

        <CitySkinProvider>
        <FloatingCityGroup
          capital={capital}
          satellites={visibleSatellites}
          paused={paused}
          selectedIslandId={selectedIslandId}
          selectedBuildingId={selectedBuildingId}
          hoveredBuildingId={hoveredBuildingId}
          departmentHouses={departmentHouses ?? []}
          selectedDepartment={selectedDepartment ?? null}
          onSelectDepartment={onSelectDepartment}
          onSelectIsland={onSelectIsland}
          onSelectBuilding={onSelectBuilding}
          onHoverBuilding={onHoverBuilding}
        />
        </CitySkinProvider>

        <ConnectingBridges satellites={visibleSatellites} paused={paused} />
        <AgentDrones count={droneCount} paused={paused} />
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
