import { useEffect, useLayoutEffect, useMemo, useRef } from 'react'
import { Canvas, useFrame, useThree, type ThreeEvent } from '@react-three/fiber'
import * as THREE from 'three'

import {
  CITY_MAP_MAX_SATELLITES,
  fnv1a32,
  type CityIslandView
} from '@shared/hive-city-map'

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

const OVERVIEW_ZOOM = 9
const FOCUS_ZOOM = 22

/** Reusable per-frame scratch object: no 60 Hz allocations inside useFrame. */
const FRAME_DUMMY = new THREE.Object3D()

export interface HiveArchipelago3DProps {
  readonly capital: CityIslandView
  readonly satellites: readonly CityIslandView[]
  readonly paused: boolean
  readonly selectedIslandId: string | null
  readonly selectedBuildingId: string | null
  readonly hoveredBuildingId: string | null
  readonly droneCount: number
  readonly onSelectIsland: (islandId: string) => void
  readonly onSelectBuilding: (islandId: string, buildingId: string) => void
  readonly onHoverBuilding: (buildingId: string | null) => void
  readonly onBackgroundClick: () => void
  readonly onContextLost: () => void
}

/** Shared materials for one island, disposed on unmount. */
function useIslandMaterials(accentHue: number, statusReady: boolean): {
  readonly body: THREE.MeshStandardMaterial
  readonly accent: THREE.MeshStandardMaterial
  readonly ground: THREE.MeshBasicMaterial
} {
  const materials = useMemo(() => {
    const body = new THREE.MeshStandardMaterial({
      color: new THREE.Color().setHSL(0, 0, 0.34),
      emissive: new THREE.Color().setHSL(accentHue / 360, 0.8, 0.12),
      roughness: 0.62,
      metalness: 0.18
    })
    const accent = new THREE.MeshStandardMaterial({
      color: new THREE.Color().setHSL(accentHue / 360, 0.7, 0.34),
      emissive: new THREE.Color().setHSL(accentHue / 360, 0.9, 0.34),
      roughness: 0.4,
      metalness: 0.3
    })
    const ground = new THREE.MeshBasicMaterial({
      color: new THREE.Color().setHSL(accentHue / 360, 0.7, 0.2),
      transparent: true,
      opacity: statusReady ? 0.24 : 0.08
    })
    return { body, accent, ground }
  }, [accentHue, statusReady])

  useEffect(() => {
    return () => {
      materials.body.dispose()
      materials.accent.dispose()
      materials.ground.dispose()
    }
  }, [materials])

  return materials
}

function IslandGround({
  view,
  material,
  onSelect
}: {
  view: CityIslandView
  material: THREE.MeshBasicMaterial
  onSelect: () => void
}) {
  const ref = useRef<THREE.Mesh>(null)
  useLayoutEffect(() => {
    const mesh = ref.current
    if (!mesh) return
    mesh.position.set(view.x, 0, view.z)
    mesh.scale.set(view.radius * 2, 0.12, view.radius * 2)
  }, [view])
  return (
    <mesh
      ref={ref}
      material={material}
      onPointerDown={(event: ThreeEvent<PointerEvent>) => {
        event.stopPropagation()
        onSelect()
      }}
    >
      <boxGeometry args={[1, 1, 1]} />
    </mesh>
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
 * Towers of one island: a single InstancedMesh scaled per building
 * (`height_weight_bp`), plus one outline mesh for the highlighted tower.
 */
function IslandTowers({
  view,
  materials,
  selectedBuildingId,
  hoveredBuildingId,
  paused,
  onSelectBuilding,
  onHoverBuilding
}: {
  view: CityIslandView
  materials: { readonly body: THREE.MeshStandardMaterial; readonly accent: THREE.MeshStandardMaterial }
  selectedBuildingId: string | null
  hoveredBuildingId: string | null
  paused: boolean
  onSelectBuilding: (islandId: string, buildingId: string) => void
  onHoverBuilding: (buildingId: string | null) => void
}) {
  const bodyRef = useRef<THREE.InstancedMesh>(null)
  const outlineRef = useRef<THREE.Mesh>(null)
  const count = view.buildings.length

  const matrices = useMemo(() => {
    const dummy = new THREE.Object3D()
    return view.buildings.map((item) => {
      const width = item.building.kind === 'citadel' ? 1.5 : 1.05
      dummy.position.set(view.x + item.x, item.height * 1.9, view.z + item.z)
      dummy.scale.set(width, item.height * 3.8, width)
      dummy.updateMatrix()
      return dummy.matrix.clone()
    })
  }, [view])

  useLayoutEffect(() => {
    const mesh = bodyRef.current
    if (!mesh) return
    matrices.forEach((matrix, index) => mesh.setMatrixAt(index, matrix))
    mesh.instanceMatrix.needsUpdate = true
    mesh.computeBoundingSphere()
  }, [matrices])

  // Soft window flicker: a deterministic per-island shimmer of emissive
  // intensity (no per-building lights, no post-processing).
  const flickerSeed = useMemo(
    () => (fnv1a32(view.island.id) % 1000) / 1000,
    [view.island.id]
  )
  useFrame((state) => {
    const material = materials.accent
    if (paused) {
      material.emissiveIntensity = 1
      return
    }
    const t = state.clock.elapsedTime
    material.emissiveIntensity = 0.85 + 0.3 * Math.sin(t * 1.7 + flickerSeed * Math.PI * 2)
  })

  const highlightedIndex = useMemo(() => {
    if (selectedBuildingId === null && hoveredBuildingId === null) return null
    const target = hoveredBuildingId ?? selectedBuildingId
    const index = view.buildings.findIndex((item) => item.building.id === target)
    return index === -1 ? null : index
  }, [hoveredBuildingId, selectedBuildingId, view.buildings])

  useLayoutEffect(() => {
    const outline = outlineRef.current
    if (!outline) return
    if (highlightedIndex === null) {
      outline.visible = false
      return
    }
    const item = view.buildings[highlightedIndex]
    if (!item) {
      outline.visible = false
      return
    }
    const width = item.building.kind === 'citadel' ? 1.7 : 1.25
    outline.position.set(view.x + item.x, item.height * 1.9, view.z + item.z)
    outline.scale.set(width, item.height * 3.8 + 0.3, width)
    outline.visible = true
  }, [highlightedIndex, view])

  const handlePointerDown = (event: ThreeEvent<PointerEvent>): void => {
    event.stopPropagation()
    const instanceId = 'instanceId' in event ? event.instanceId : undefined
    const item = instanceId === undefined ? undefined : view.buildings[instanceId]
    if (item) {
      onSelectBuilding(view.island.id, item.building.id)
    } else {
      onSelectBuilding(view.island.id, view.island.id)
    }
  }

  if (count === 0) return <></>
  return (
    <group>
      <instancedMesh
        ref={bodyRef}
        args={[undefined, materials.body, count]}
        onPointerDown={handlePointerDown}
        onPointerMove={(event: ThreeEvent<PointerEvent>) => {
          event.stopPropagation()
          const instanceId = 'instanceId' in event ? event.instanceId : undefined
          const item = instanceId === undefined ? undefined : view.buildings[instanceId]
          onHoverBuilding(item ? item.building.id : null)
        }}
        onPointerOut={() => onHoverBuilding(null)}
      >
        {/* Без геометрии-потомка instancedMesh получает пустой BufferGeometry
            и не рисует ни одного instance — город остаётся невидимым. */}
        <boxGeometry args={[1, 1, 1]} />
      </instancedMesh>
      <mesh ref={outlineRef} visible={false}>
        <boxGeometry args={[1, 1, 1]} />
        <meshBasicMaterial color="#ffe9b0" wireframe />
      </mesh>
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
        context.strokeStyle = `hsl(${view.island.accent_hue} 72% 62% / 0.75)`
        context.lineWidth = 2
        context.stroke()
      } else {
        context.fillRect(0, 0, canvas.width, canvas.height)
        context.strokeStyle = `hsl(${view.island.accent_hue}, 72%, 62%)`
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
  }, [view.island.name, view.island.accent_hue])

  useEffect(() => () => texture.dispose(), [texture])

  return (
    <sprite position={[view.x, 2.2, view.z + view.radius + 1.5]} scale={[6, 1.5, 1]}>
      <spriteMaterial map={texture} transparent depthWrite={false} />
    </sprite>
  )
}

/** Citadel: breathing halo and emissive core reacting to the hive status. */
function CitadelCore({ view, paused }: { view: CityIslandView; paused: boolean }) {
  const coreRef = useRef<THREE.Mesh>(null)
  const haloRef = useRef<THREE.Mesh>(null)
  const material = useMemo(
    () =>
      new THREE.MeshBasicMaterial({
        color: new THREE.Color('#ffe9b0'),
        transparent: true,
        opacity: 0.92
      }),
    []
  )
  useEffect(() => () => material.dispose(), [material])

  useFrame((state) => {
    const t = state.clock.elapsedTime
    if (!paused && coreRef.current) {
      coreRef.current.scale.setScalar(0.42 + 0.08 * Math.sin(t * 2.1))
    }
    if (!paused && haloRef.current) {
      haloRef.current.scale.setScalar(1 + 0.06 * Math.sin(t * 1.3))
    }
  })

  const position = useMemo<[number, number, number]>(() => {
    const citadel = view.buildings.find((item) => item.building.kind === 'citadel')
    return [view.x + (citadel?.x ?? 0), (citadel?.height ?? 0.6) * 3.8 + 1.9, view.z + (citadel?.z ?? 0)]
  }, [view])

  return (
    <group>
      <mesh ref={coreRef} position={position} material={material}>
        <sphereGeometry args={[1, 12, 12]} />
      </mesh>
      <mesh ref={haloRef} position={[position[0], position[1], position[2]]}>
        <sphereGeometry args={[1.35, 14, 14]} />
        <meshBasicMaterial color="#8d82e8" transparent opacity={0.16} />
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
  onSelectIsland,
  onSelectBuilding,
  onHoverBuilding
}: {
  view: CityIslandView
  paused: boolean
  selectedIsland: boolean
  selectedBuildingId: string | null
  hoveredBuildingId: string | null
  onSelectIsland: (islandId: string) => void
  onSelectBuilding: (islandId: string, buildingId: string) => void
  onHoverBuilding: (buildingId: string | null) => void
}) {
  const groupRef = useRef<THREE.Group>(null)
  const materials = useIslandMaterials(view.island.accent_hue, view.island.status === 'ready')

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
      <IslandGround view={view} material={materials.ground} onSelect={() => onSelectIsland(view.island.id)} />
      <IslandTowers
        view={view}
        materials={materials}
        selectedBuildingId={selectedIsland ? selectedBuildingId : null}
        hoveredBuildingId={view.island.status === 'ready' ? hoveredBuildingId : null}
        paused={paused}
        onSelectBuilding={onSelectBuilding}
        onHoverBuilding={onHoverBuilding}
      />
      <CitadelCore view={view} paused={paused} />
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
        <ambientLight intensity={0.55} />
        <directionalLight position={[30, 40, 10]} intensity={1.1} />
        <CameraRig focus={focus} paused={paused} />

        <FloatingCityGroup
          capital={capital}
          satellites={visibleSatellites}
          paused={paused}
          selectedIslandId={selectedIslandId}
          selectedBuildingId={selectedBuildingId}
          hoveredBuildingId={hoveredBuildingId}
          onSelectIsland={onSelectIsland}
          onSelectBuilding={onSelectBuilding}
          onHoverBuilding={onHoverBuilding}
        />

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
