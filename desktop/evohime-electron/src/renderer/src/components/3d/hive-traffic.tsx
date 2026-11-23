import { useEffect, useMemo, useRef, type MutableRefObject } from 'react'
import { useFrame } from '@react-three/fiber'
import * as THREE from 'three'

export const QUEEN_GATE_ANGLE = Math.PI / 2
export const MAIN_AVENUES_RADIUS = 7.2
export const HIVE_CAR_SPEED = 1.65
export const AVENUE_RING_STEP = 3.2
export const QUEEN_PROCESS_SEC = 1.15
export const INTERSECTION_STOP_HALF_EXTENT = 0.49
export const CAR_FRONT_BUMPER_OFFSET = 0.21
export const CAR_HALF_WIDTH = 0.082
export const CAR_HALF_LENGTH = 0.20

const TRAFFIC_LIGHT_CYCLE_SEC = 9.0

export type TrafficLightState = 'ring_green' | 'amber_to_radial' | 'radial_green' | 'amber_to_ring'

export function getTrafficLightState(timeSec: number): TrafficLightState {
  const phase = ((timeSec % TRAFFIC_LIGHT_CYCLE_SEC) + TRAFFIC_LIGHT_CYCLE_SEC) % TRAFFIC_LIGHT_CYCLE_SEC
  if (phase < 3.5) return 'ring_green'
  if (phase < 4.5) return 'amber_to_radial'
  if (phase < 8.0) return 'radial_green'
  return 'amber_to_ring'
}

/** Возвращает true, если разрешён въезд на перекрёсток со стороны кольца. */
export function isRingTrafficGreen(timeSec: number): boolean {
  return getTrafficLightState(timeSec) === 'ring_green'
}

/** Возвращает true, если разрешён въезд на перекрёсток со стороны прямой магистрали. */
export function isRadialTrafficGreen(timeSec: number): boolean {
  return getTrafficLightState(timeSec) === 'radial_green'
}

/**
 * Отрезок дороги (2-полосный для заездных или 4-полосный для магистралей/перемычек):
 * контрастный тёмный асфальт, светлые бордюры по краям, осевая и межполосная разметка.
 */
export function RoadSegment3D({
  from,
  to,
  bedMaterial,
  curbMaterial,
  lineMaterial,
  width = 0.34,
  lanes = 2,
  y = 0.02
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
  if (length < 0.05) return null
  const cx = (from[0] + to[0]) / 2
  const cz = (from[1] + to[1]) / 2
  const angle = Math.atan2(dz, dx)
  const curbMat = curbMaterial ?? lineMaterial
  const curbOffset = width * 0.46
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
 * Регулируемый перекрёсток на пересечении кольца и прямой магистрали от ворот Королевы:
 * разрывает сплошную разметку кольца для легальных разворотов и поворотов, рисует стоп-линии и светофоры.
 */
export function RegulatedIntersection3D({
  x,
  z,
  radius,
  angle,
  bedMaterial,
  paused,
  simClockRef
}: {
  x: number
  z: number
  radius: number
  angle: number
  bedMaterial: THREE.Material
  paused: boolean
  simClockRef?: MutableRefObject<number> | undefined
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
    const elapsed = paused ? 0 : simClockRef ? simClockRef.current : state.clock.elapsedTime
    const light = getTrafficLightState(elapsed)
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
      <mesh position={[0, 0.039, 0]} rotation={[-Math.PI / 2, 0, 0]} material={bedMaterial}>
        <planeGeometry args={[0.88, 0.88]} />
      </mesh>
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

export function computeQueenGateStopAngle(innerBoulevardR: number): number {
  const gateClearanceArc = (0.32 + 0.18 + 0.06) / Math.max(1.5, innerBoulevardR)
  return QUEEN_GATE_ANGLE - gateClearanceArc
}

export function computeRingEntryStopProgress(innerBoulevardR: number, homeEdgeR: number): number {
  const span = homeEdgeR - innerBoulevardR
  if (span <= 0.02) return 0.2
  const stopR = Math.min(homeEdgeR - 0.02, innerBoulevardR + 0.42 + 0.18 + 0.04)
  return Math.max(0.05, Math.min(0.88, (homeEdgeR - stopR) / span))
}

export function angularDistance(a: number, b: number): number {
  let d = a - b
  while (d > Math.PI) d -= Math.PI * 2
  while (d < -Math.PI) d += Math.PI * 2
  return Math.abs(d)
}

/**
 * Квадратичная кривая Безье и единичная касательная для плавного поворота
 * машины на перекрёстке без мгновенного скачка позиции или угла.
 */
export function evalTurnBezier(
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

export function posArc(delta: number): number {
  let d = delta
  while (d < 0) d += Math.PI * 2
  while (d >= Math.PI * 2) d -= Math.PI * 2
  return d
}

export type PathPose = { x: number; z: number; dx: number; dz: number }

export interface PathSegment {
  readonly len: number
  readonly eval: (u: number) => PathPose
}

export function approxBezierLength(
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

export function evalSegmentedPose(segments: readonly PathSegment[], t: number): PathPose {
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

export function buildMainRingPddSegments(
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
  const lead1 = Math.min(0.49 / Math.max(2, ringR), arc * 0.42)
  const lead2 = Math.min(0.49 / Math.max(2, ringR), arc * 0.42)

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

export interface VehiclePoseState {
  lastX?: number
  lastZ?: number
  lastDirX?: number
  lastDirZ?: number
  lastVisible?: boolean
}

export function isVehicleAheadBlocked(
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
    const otherDirX = other.lastDirX ?? dirX
    const otherDirZ = other.lastDirZ ?? dirZ
    const sameDir =
      other.lastDirX !== undefined && other.lastDirZ !== undefined
        ? other.lastDirX * dirX + other.lastDirZ * dirZ
        : 1
    if (sameDir > 0.25) {
      const strictlyAhead = dot > 0.04 || (dist <= 0.04 && i < selfIndex)
      if (strictlyAhead && cross < 0.22) {
        return true
      }
    } else if (sameDir > -0.45) {
      if (dist < 0.54 && dot > 0.02 && cross < 0.28) {
        const otherDot = -dx * otherDirX + -dz * otherDirZ
        if (otherDot <= 0.02) {
          return true
        }
        if (dot > otherDot + 0.015 || (Math.abs(dot - otherDot) <= 0.015 && selfIndex > i)) {
          return true
        }
      }
    }
  }
  return false
}

/**
 * Проверяет, свободен ли перекрёсток (ix, iz) от машин конфликтующих направлений
 * или машин, перекрывающих точку въезда (ПДД 13.8), перед пересечением стоп-линии.
 */
export function isIntersectionClearForEntry(
  ix: number,
  iz: number,
  entryX: number,
  entryZ: number,
  dirX: number,
  dirZ: number,
  selfIndex: number,
  peers: readonly VehiclePoseState[],
  halfBox = INTERSECTION_STOP_HALF_EXTENT
): boolean {
  for (let i = 0; i < peers.length; i += 1) {
    if (i === selfIndex) continue
    const other = peers[i]
    if (!other || !other.lastVisible) continue
    if (other.lastX === undefined || other.lastZ === undefined) continue
    if (Math.abs(other.lastX - ix) < halfBox && Math.abs(other.lastZ - iz) < halfBox) {
      const sameDir = (other.lastDirX ?? dirX) * dirX + (other.lastDirZ ?? dirZ) * dirZ
      if (sameDir < 0.65) {
        return false
      }
      if (Math.hypot(other.lastX - entryX, other.lastZ - entryZ) < 0.62) {
        return false
      }
    }
  }
  return true
}

/**
 * Точная проверка пересечения двух ориентированных 2D-прямоугольников машин (2D OBB / SAT).
 */
export function doCarBoxesOverlap(
  ax: number,
  az: number,
  aDirX: number,
  aDirZ: number,
  bx: number,
  bz: number,
  bDirX: number,
  bDirZ: number,
  halfWidth = CAR_HALF_WIDTH,
  halfLength = CAR_HALF_LENGTH
): boolean {
  const aLen = Math.hypot(aDirX, aDirZ) || 1
  const adx = aDirX / aLen
  const adz = aDirZ / aLen
  const bLen = Math.hypot(bDirX, bDirZ) || 1
  const bdx = bDirX / bLen
  const bdz = bDirZ / bLen

  const dx = bx - ax
  const dz = bz - az

  const axes: readonly [number, number][] = [
    [adx, adz],
    [-adz, adx],
    [bdx, bdz],
    [-bdz, bdx]
  ]

  for (let i = 0; i < axes.length; i += 1) {
    const axis = axes[i]!
    const ux = axis[0]
    const uz = axis[1]
    const projDist = Math.abs(dx * ux + dz * uz)
    const ra = halfLength * Math.abs(adx * ux + adz * uz) + halfWidth * Math.abs(-adz * ux + adx * uz)
    const rb = halfLength * Math.abs(bdx * ux + bdz * uz) + halfWidth * Math.abs(-bdz * ux + bdx * uz)
    if (projDist >= ra + rb) {
      return false
    }
  }
  return true
}

/**
 * Проверяет, сталкивается ли поза (x, z, dirX, dirZ) с габаритом любой другой видимой машины.
 * При переданных prevX/prevZ шаг, строго увеличивающий дистанцию до задетой машины, разрешается.
 */
export function isPoseCollidingWithPeers(
  x: number,
  z: number,
  dirX: number,
  dirZ: number,
  selfIndex: number,
  peers: readonly VehiclePoseState[],
  prevX?: number,
  prevZ?: number
): boolean {
  const broadRadius = CAR_HALF_LENGTH * 2 + 0.08
  for (let i = 0; i < peers.length; i += 1) {
    if (i === selfIndex) continue
    const other = peers[i]
    if (!other || !other.lastVisible) continue
    if (other.lastX === undefined || other.lastZ === undefined) continue
    const nextDist = Math.hypot(other.lastX - x, other.lastZ - z)
    if (nextDist >= broadRadius) continue
    if (
      doCarBoxesOverlap(
        x,
        z,
        dirX,
        dirZ,
        other.lastX,
        other.lastZ,
        other.lastDirX ?? dirX,
        other.lastDirZ ?? dirZ
      )
    ) {
      if (prevX !== undefined && prevZ !== undefined) {
        const prevDist = Math.hypot(other.lastX - prevX, other.lastZ - prevZ)
        if (nextDist > prevDist + 0.002) {
          continue
        }
      }
      return true
    }
  }
  return false
}

/**
 * Проверяет, безопасен ли следующий шаг по сегментному маршруту (включая упреждение по траектории),
 * чтобы машина не поворачивала к зданию и не заезжала в коридор, занятый другой машиной.
 */
export function isPathStepBlocked(
  segments: readonly PathSegment[],
  totalLength: number,
  candidateProgress: number,
  selfIndex: number,
  peers: readonly VehiclePoseState[],
  curX: number,
  curZ: number,
  lookaheadDist = 0.08
): boolean {
  const clampedP = Math.min(1, Math.max(0, candidateProgress))
  const nextPose = evalSegmentedPose(segments, clampedP)
  if (isPoseCollidingWithPeers(nextPose.x, nextPose.z, nextPose.dx, nextPose.dz, selfIndex, peers, curX, curZ)) {
    return true
  }
  if (clampedP < 1) {
    const lookP = Math.min(1, clampedP + lookaheadDist / Math.max(1.5, totalLength))
    const lookPose = evalSegmentedPose(segments, lookP)
    if (isPoseCollidingWithPeers(lookPose.x, lookPose.z, lookPose.dx, lookPose.dz, selfIndex, peers, curX, curZ)) {
      return true
    }
  }
  return false
}
