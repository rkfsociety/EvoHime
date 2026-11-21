import { useLayoutEffect, useMemo, useRef } from 'react'
import * as THREE from 'three'

export interface HexPlatformProps {
  /** Акцентный цвет свечения контура платформы в зависимости от статуса Улья. */
  readonly accentColor: string
}

const GRID_RADIUS = 4
const HEX_RADIUS = 0.56
const HEX_GAP = 0.05
const HEX_HEIGHT = 0.14

interface HexCoord {
  readonly x: number
  readonly z: number
  readonly ring: number
}

function buildHexGridCoords(radius: number): HexCoord[] {
  const coords: HexCoord[] = []
  const step = HEX_RADIUS + HEX_GAP
  const sqrt3 = Math.sqrt(3)

  for (let q = -radius; q <= radius; q += 1) {
    const r1 = Math.max(-radius, -q - radius)
    const r2 = Math.min(radius, -q + radius)
    for (let r = r1; r <= r2; r += 1) {
      const x = step * sqrt3 * (q + r / 2)
      const z = step * 1.5 * r
      const ring = Math.max(Math.abs(q), Math.abs(r), Math.abs(q + r))
      coords.push({ x, z, ring })
    }
  }

  return coords
}

/**
 * Базовая гексагональная платформа Улья на базе `InstancedMesh`.
 * Состоит из 61 тёмной шестиугольной плитки, тонкой гексагональной сетки-контура
 * и мягкого свечения по внешнему периметру.
 */
export function HexPlatform({ accentColor }: HexPlatformProps): React.JSX.Element {
  const tilesRef = useRef<THREE.InstancedMesh>(null)
  const edgesRef = useRef<THREE.InstancedMesh>(null)

  const coords = useMemo(() => buildHexGridCoords(GRID_RADIUS), [])

  const tileGeometry = useMemo(() => new THREE.CylinderGeometry(HEX_RADIUS, HEX_RADIUS * 1.03, HEX_HEIGHT, 6), [])
  const edgeGeometry = useMemo(() => {
    const geo = new THREE.RingGeometry(HEX_RADIUS * 0.92, HEX_RADIUS * 0.99, 6)
    geo.rotateX(-Math.PI / 2)
    geo.rotateY(Math.PI / 6)
    return geo
  }, [])
  const perimeterGeometry = useMemo(() => {
    const outerRadius = (HEX_RADIUS + HEX_GAP) * (GRID_RADIUS * 1.78 + 0.9)
    const geo = new THREE.RingGeometry(outerRadius - 0.06, outerRadius, 6)
    geo.rotateX(-Math.PI / 2)
    return geo
  }, [])

  useLayoutEffect(() => {
    const tiles = tilesRef.current
    const edges = edgesRef.current
    if (!tiles || !edges) return

    const dummy = new THREE.Object3D()
    const color = new THREE.Color()

    coords.forEach((coord, index) => {
      const elevation = coord.ring === 0 ? 0.03 : coord.ring <= 2 ? 0.01 : 0
      dummy.position.set(coord.x, -HEX_HEIGHT / 2 + elevation, coord.z)
      dummy.rotation.set(0, 0, 0)
      dummy.scale.set(1, 1, 1)
      dummy.updateMatrix()
      tiles.setMatrixAt(index, dummy.matrix)

      // Лёгкое затемнение от центра к краям платформы.
      const shade = coord.ring === 0 ? '#1e293b' : coord.ring <= 2 ? '#152036' : '#0f172a'
      color.set(shade)
      tiles.setColorAt(index, color)

      dummy.position.set(coord.x, elevation + 0.004, coord.z)
      dummy.updateMatrix()
      edges.setMatrixAt(index, dummy.matrix)
    })

    tiles.instanceMatrix.needsUpdate = true
    if (tiles.instanceColor) {
      tiles.instanceColor.needsUpdate = true
    }
    edges.instanceMatrix.needsUpdate = true
  }, [coords])

  return (
    <group name="hive-base-platform">
      {/* Плитки гексагональной сетки (InstancedMesh для 1 draw call). */}
      <instancedMesh
        ref={tilesRef}
        args={[tileGeometry, undefined, coords.length]}
        receiveShadow
      >
        <meshStandardMaterial
          roughness={0.76}
          metalness={0.35}
        />
      </instancedMesh>

      {/* Тонкие контуры каждого шестиугольника. */}
      <instancedMesh
        ref={edgesRef}
        args={[edgeGeometry, undefined, coords.length]}
      >
        <meshBasicMaterial
          color={accentColor}
          transparent
          opacity={0.18}
          toneMapped={false}
        />
      </instancedMesh>

      {/* Едва заметное свечение по внешнему краю платформы. */}
      <mesh geometry={perimeterGeometry} position={[0, -0.02, 0]}>
        <meshBasicMaterial
          color={accentColor}
          transparent
          opacity={0.28}
          side={THREE.DoubleSide}
          toneMapped={false}
        />
      </mesh>

      {/* Тонкая подложка-сетка под платформой для изометрической глубины. */}
      <gridHelper args={[14, 14, '#1e293b', '#131c31']} position={[0, -0.15, 0]} />
    </group>
  )
}
