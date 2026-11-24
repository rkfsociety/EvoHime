import { useEffect, useMemo, useRef, type MutableRefObject } from 'react'
import { useFrame } from '@react-three/fiber'
import * as THREE from 'three'

export const QUEEN_GATE_ANGLE = Math.PI / 2
export const RING_CONNECTOR_ANGLES: readonly number[] = [Math.PI / 2, -Math.PI / 2, 0, Math.PI]
export const MAIN_AVENUES_RADIUS = 7.2
export const HIVE_CAR_SPEED = 1.65
export const AVENUE_RING_STEP = 3.2
export const QUEEN_PROCESS_SEC = 1.15
export const INTERSECTION_BOX_HALF_EXTENT = 0.42
export const INTERSECTION_STOP_HALF_EXTENT = 0.64
export const CAR_FRONT_BUMPER_OFFSET = 0.21
export const CAR_HALF_WIDTH = 0.082
export const CAR_HALF_LENGTH = 0.20

const TRAFFIC_LIGHT_CYCLE_SEC = 10.0

export type TrafficLightState =
  | 'ring_green'
  | 'ring_green_blink'
  | 'amber_to_radial'
  | 'radial_green'
  | 'radial_green_blink'
  | 'amber_to_ring'

export function getTrafficLightState(timeSec: number): TrafficLightState {
  const phase = ((timeSec % TRAFFIC_LIGHT_CYCLE_SEC) + TRAFFIC_LIGHT_CYCLE_SEC) % TRAFFIC_LIGHT_CYCLE_SEC
  if (phase < 3.2) return 'ring_green'
  if (phase < 4.2) return 'ring_green_blink'
  if (phase < 5.0) return 'amber_to_radial'
  if (phase < 8.2) return 'radial_green'
  if (phase < 9.2) return 'radial_green_blink'
  return 'amber_to_ring'
}

/** Возвращает true, если разрешён въезд на перекрёсток со стороны кольца (зелёный или зелёный мигающий по ПДД 6.2). */
export function isRingTrafficGreen(timeSec: number): boolean {
  const state = getTrafficLightState(timeSec)
  return state === 'ring_green' || state === 'ring_green_blink'
}

/** Возвращает true, если разрешён въезд на перекрёсток со стороны прямой магистрали (зелёный или зелёный мигающий по ПДД 6.2). */
export function isRadialTrafficGreen(timeSec: number): boolean {
  const state = getTrafficLightState(timeSec)
  return state === 'radial_green' || state === 'radial_green_blink'
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
  simClockRef,
  hasInnerRadial = true,
  hasOuterRadial = true
}: {
  x: number
  z: number
  radius: number
  angle: number
  bedMaterial: THREE.Material
  paused: boolean
  simClockRef?: MutableRefObject<number> | undefined
  hasInnerRadial?: boolean | undefined
  hasOuterRadial?: boolean | undefined
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
  const housingMat = useMemo(
    () => new THREE.MeshStandardMaterial({ color: '#0f1422', roughness: 0.7, metalness: 0.3 }),
    []
  )
  const lampRedOn = useMemo(() => new THREE.MeshBasicMaterial({ color: '#ff3b30' }), [])
  const lampAmberOn = useMemo(() => new THREE.MeshBasicMaterial({ color: '#ffb020' }), [])
  const lampGreenOn = useMemo(() => new THREE.MeshBasicMaterial({ color: '#22e564' }), [])
  const lampRedOff = useMemo(() => new THREE.MeshBasicMaterial({ color: '#3b1215' }), [])
  const lampAmberOff = useMemo(() => new THREE.MeshBasicMaterial({ color: '#3b2a0e' }), [])
  const lampGreenOff = useMemo(() => new THREE.MeshBasicMaterial({ color: '#0e331c' }), [])

  const ringRedRefs = useRef<Array<THREE.Mesh | null>>([])
  const ringAmberRefs = useRef<Array<THREE.Mesh | null>>([])
  const ringGreenRefs = useRef<Array<THREE.Mesh | null>>([])
  const radialRedRefs = useRef<Array<THREE.Mesh | null>>([])
  const radialAmberRefs = useRef<Array<THREE.Mesh | null>>([])
  const radialGreenRefs = useRef<Array<THREE.Mesh | null>>([])
  const lastSignalKeyRef = useRef<string>('')

  useEffect(() => () => {
    stopLineMat.dispose()
    poleMat.dispose()
    housingMat.dispose()
    lampRedOn.dispose()
    lampAmberOn.dispose()
    lampGreenOn.dispose()
    lampRedOff.dispose()
    lampAmberOff.dispose()
    lampGreenOff.dispose()
  }, [
    stopLineMat,
    poleMat,
    housingMat,
    lampRedOn,
    lampAmberOn,
    lampGreenOn,
    lampRedOff,
    lampAmberOff,
    lampGreenOff
  ])

  useFrame((state) => {
    const elapsed = paused ? 0 : simClockRef ? simClockRef.current : state.clock.elapsedTime
    const light = getTrafficLightState(elapsed)
    const blinkOn = Math.floor(elapsed * 6) % 2 === 0
    const signalKey = `${light}:${blinkOn ? 1 : 0}`
    if (lastSignalKeyRef.current === signalKey) return
    lastSignalKeyRef.current = signalKey

    const ringRed = light === 'radial_green' || light === 'radial_green_blink' || light === 'amber_to_ring' ? lampRedOn : lampRedOff
    const ringAmber =
      light === 'amber_to_radial' || light === 'amber_to_ring' ? lampAmberOn : lampAmberOff
    const ringGreen =
      light === 'ring_green' || (light === 'ring_green_blink' && blinkOn) ? lampGreenOn : lampGreenOff

    const radialRed = light === 'ring_green' || light === 'ring_green_blink' || light === 'amber_to_radial' ? lampRedOn : lampRedOff
    const radialAmber =
      light === 'amber_to_radial' || light === 'amber_to_ring' ? lampAmberOn : lampAmberOff
    const radialGreen =
      light === 'radial_green' || (light === 'radial_green_blink' && blinkOn) ? lampGreenOn : lampGreenOff

    for (const lamp of ringRedRefs.current) {
      if (lamp) lamp.material = ringRed
    }
    for (const lamp of ringAmberRefs.current) {
      if (lamp) lamp.material = ringAmber
    }
    for (const lamp of ringGreenRefs.current) {
      if (lamp) lamp.material = ringGreen
    }
    for (const lamp of radialRedRefs.current) {
      if (lamp) lamp.material = radialRed
    }
    for (const lamp of radialAmberRefs.current) {
      if (lamp) lamp.material = radialAmber
    }
    for (const lamp of radialGreenRefs.current) {
      if (lamp) lamp.material = radialGreen
    }
  })

  const ix = x + Math.cos(angle) * radius
  const iz = z + Math.sin(angle) * radius
  const ringHeads: readonly {
    poleX: number
    poleZ: number
    headX: number
    headZ: number
    rotY: number
  }[] = [
    { poleX: -0.48, poleZ: 0.46, headX: -0.48, headZ: 0.21, rotY: 0 },
    { poleX: 0.48, poleZ: -0.46, headX: 0.48, headZ: -0.21, rotY: Math.PI }
  ]
  const radialHeads: readonly {
    poleX: number
    poleZ: number
    headX: number
    headZ: number
    rotY: number
  }[] = [
    { poleX: -0.46, poleZ: -0.48, headX: -0.21, headZ: -0.48, rotY: -Math.PI / 2 },
    { poleX: 0.46, poleZ: 0.48, headX: 0.21, headZ: 0.48, rotY: Math.PI / 2 }
  ]

  const renderTrafficHead = (
    key: string,
    head: { poleX: number; poleZ: number; headX: number; headZ: number; rotY: number },
    idx: number,
    redRefs: MutableRefObject<Array<THREE.Mesh | null>>,
    amberRefs: MutableRefObject<Array<THREE.Mesh | null>>,
    greenRefs: MutableRefObject<Array<THREE.Mesh | null>>,
    initGreen: boolean
  ) => {
    const dx = head.headX - head.poleX
    const dz = head.headZ - head.poleZ
    const armLen = Math.hypot(dx, dz)
    const armX = (head.poleX + head.headX) * 0.5
    const armZ = (head.poleZ + head.headZ) * 0.5
    const armAlongX = Math.abs(dx) >= Math.abs(dz)
    return (
      <group key={key}>
        <mesh position={[head.poleX, 0.22, head.poleZ]} material={poleMat}>
          <cylinderGeometry args={[0.022, 0.028, 0.44, 6]} />
        </mesh>
        <mesh position={[armX, 0.43, armZ]} material={poleMat}>
          <boxGeometry args={[armAlongX ? armLen : 0.026, 0.024, armAlongX ? 0.026 : armLen]} />
        </mesh>
        <group position={[head.headX, 0.44, head.headZ]} rotation={[0, head.rotY, 0]}>
          <mesh material={housingMat}>
            <boxGeometry args={[0.085, 0.055, 0.29]} />
          </mesh>
          <mesh
            ref={(el) => {
              redRefs.current[idx] = el
            }}
            position={[0, 0.032, -0.09]}
            material={initGreen ? lampRedOff : lampRedOn}
          >
            <cylinderGeometry args={[0.034, 0.034, 0.022, 12]} />
          </mesh>
          <mesh
            ref={(el) => {
              amberRefs.current[idx] = el
            }}
            position={[0, 0.032, 0]}
            material={lampAmberOff}
          >
            <cylinderGeometry args={[0.034, 0.034, 0.022, 12]} />
          </mesh>
          <mesh
            ref={(el) => {
              greenRefs.current[idx] = el
            }}
            position={[0, 0.032, 0.09]}
            material={initGreen ? lampGreenOn : lampGreenOff}
          >
            <cylinderGeometry args={[0.034, 0.034, 0.022, 12]} />
          </mesh>
        </group>
      </group>
    )
  }

  return (
    <group position={[ix, 0, iz]} rotation={[0, -angle + Math.PI / 2, 0]}>
      <mesh position={[0, 0.039, 0]} rotation={[-Math.PI / 2, 0, 0]} material={bedMaterial}>
        <planeGeometry args={[0.88, 0.88]} />
      </mesh>
      {hasInnerRadial ? (
        <mesh
          position={[-0.21, 0.041, -INTERSECTION_STOP_HALF_EXTENT]}
          rotation={[-Math.PI / 2, 0, 0]}
          material={stopLineMat}
        >
          <planeGeometry args={[0.38, 0.055]} />
        </mesh>
      ) : null}
      {hasOuterRadial ? (
        <mesh
          position={[0.21, 0.041, INTERSECTION_STOP_HALF_EXTENT]}
          rotation={[-Math.PI / 2, 0, 0]}
          material={stopLineMat}
        >
          <planeGeometry args={[0.38, 0.055]} />
        </mesh>
      ) : null}
      <mesh
        position={[-INTERSECTION_STOP_HALF_EXTENT, 0.041, 0.21]}
        rotation={[-Math.PI / 2, 0, Math.PI / 2]}
        material={stopLineMat}
      >
        <planeGeometry args={[0.38, 0.055]} />
      </mesh>
      <mesh
        position={[INTERSECTION_STOP_HALF_EXTENT, 0.041, -0.21]}
        rotation={[-Math.PI / 2, 0, Math.PI / 2]}
        material={stopLineMat}
      >
        <planeGeometry args={[0.38, 0.055]} />
      </mesh>
      {ringHeads.map((head, idx) =>
        renderTrafficHead(
          `tl-ring-${idx}`,
          head,
          idx,
          ringRedRefs,
          ringAmberRefs,
          ringGreenRefs,
          true
        )
      )}
      {radialHeads
        .filter((_, idx) => (idx === 0 ? hasInnerRadial : hasOuterRadial))
        .map((head, idx) =>
        renderTrafficHead(
          `tl-radial-${idx}`,
          head,
          idx,
          radialRedRefs,
          radialAmberRefs,
          radialGreenRefs,
          false
        )
      )}
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

export const LEFT_LANE_OFFSET = 0.13
export const RIGHT_LANE_OFFSET = 0.29

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
  // ПДД РФ п. 8.5, 8.6: направо — в правую полосу (0.29), налево — в левую (0.13);
  // съезд направо — с правой полосы, съезд налево — с левой.
  const entryRightTurn = startIsInner === (ringSign === 1)
  const exitRightTurn = startIsInner !== (ringSign === 1)
  const entryLaneR = ringR - ringSign * (entryRightTurn ? RIGHT_LANE_OFFSET : LEFT_LANE_OFFSET)
  const exitLaneR = ringR - ringSign * (exitRightTurn ? RIGHT_LANE_OFFSET : LEFT_LANE_OFFSET)
  const midLaneR = (entryLaneR + exitLaneR) * 0.5
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
  const t1cx = viewX + Math.cos(startAngle) * entryLaneR - r1dz * laneOffset
  const t1cz = viewZ + Math.sin(startAngle) * entryLaneR + r1dx * laneOffset
  const out1A = startAngle + ringSign * lead1
  const t1bx = viewX + Math.cos(out1A) * entryLaneR
  const t1bz = viewZ + Math.sin(out1A) * entryLaneR

  const arcSpan = Math.max(0, arc - lead1 - lead2)

  const t2InA = endAngle - ringSign * lead2
  const t2Ax = viewX + Math.cos(t2InA) * exitLaneR
  const t2Az = viewZ + Math.sin(t2InA) * exitLaneR
  const t2Cx = viewX + Math.cos(endAngle) * exitLaneR - r2dz * laneOffset
  const t2Cz = viewZ + Math.sin(endAngle) * exitLaneR + r2dx * laneOffset
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
      len: arcSpan * midLaneR,
      eval: (u) => {
        const curA = out1A + ringSign * u * arcSpan
        const smooth = u * u * (3 - 2 * u)
        const curR = entryLaneR + (exitLaneR - entryLaneR) * smooth
        return {
          x: viewX + Math.cos(curA) * curR,
          z: viewZ + Math.sin(curA) * curR,
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

export type RoadPriority = 'main' | 'driveway'

export interface VehiclePoseState {
  lastX?: number
  lastZ?: number
  lastDirX?: number
  lastDirZ?: number
  lastVisible?: boolean
  roadPriority?: RoadPriority
  blockedSec?: number
  yieldingTo?: number | undefined
  inIntersection?: boolean
}

export function isVehicleAheadBlocked(
  x: number,
  z: number,
  dirX: number,
  dirZ: number,
  selfIndex: number,
  peers: readonly VehiclePoseState[],
  minGap = 0.56,
  selfPriority: RoadPriority = 'main'
): boolean {
  const effectiveGap = Math.max(minGap, 1.15)
  const effectiveGapSq = effectiveGap * effectiveGap
  const selfState = peers[selfIndex]
  const selfBlocked = selfState?.blockedSec ?? 0
  const selfInIntersection = selfState?.inIntersection === true
  for (let i = 0; i < peers.length; i += 1) {
    if (i === selfIndex) continue
    const other = peers[i]
    if (!other || !other.lastVisible) continue
    if (other.lastX === undefined || other.lastZ === undefined) continue
    const dx = other.lastX - x
    const dz = other.lastZ - z
    const distSq = dx * dx + dz * dz
    if (distSq >= effectiveGapSq) continue
    const dist = Math.sqrt(distSq)
    const dot = dx * dirX + dz * dirZ
    const cross = Math.abs(dx * -dirZ + dz * dirX)
    const otherDirX = other.lastDirX ?? dirX
    const otherDirZ = other.lastDirZ ?? dirZ
    const otherPriority: RoadPriority = other.roadPriority ?? 'main'
    const sameDir =
      other.lastDirX !== undefined && other.lastDirZ !== undefined
        ? other.lastDirX * dirX + other.lastDirZ * dirZ
        : 1
    if (sameDir > 0.25) {
      const strictlyAhead = dot > 0.04 || (dist <= 0.04 && i < selfIndex)
      if (dist < minGap && strictlyAhead && cross < 0.22) {
        return true
      }
    } else if (sameDir > -0.55) {
      // ПДД РФ п. 13.7: машина, уже выехавшая на перекрёсток, завершает манёвр и не уступает стоящим за стоп-линией.
      if (selfInIntersection && !other.inIntersection) {
        continue
      }
      // ПДД РФ п. 13.8: въезжающий на перекрёсток обязан уступить машине, завершающей манёвр.
      if (!selfInIntersection && other.inIntersection === true && dist < 0.85 && dot > 0.02) {
        if (selfState) selfState.yieldingTo = i
        return true
      }
      if (other.yieldingTo === selfIndex) {
        continue
      }
      if (selfPriority === 'driveway' && otherPriority === 'main') {
        // ПДД РФ п. 8.3: выезд с прилегающей территории уступает всем приближающимся по главной дороге (кольцу).
        const otherApproaching = -dx * otherDirX + -dz * otherDirZ > -0.2
        if (dist < 1.15 && dot > -0.25 && otherApproaching) {
          if (selfState) selfState.yieldingTo = i
          return true
        }
      } else if (selfPriority === 'main' && otherPriority === 'driveway') {
        // Главная дорога имеет приоритет и не тормозит перед стоящим на выезде, пока полоса физически свободна.
        if (cross < 0.14 && dot > 0.04 && dist < 0.44) {
          if (selfState) selfState.yieldingTo = i
          return true
        }
      } else {
        if (selfBlocked > 0.5 && (other.blockedSec ?? 0) > 0.5) {
          if (selfIndex < i) {
            continue
          }
          if (selfState) selfState.yieldingTo = i
          return true
        }
        // Равные приоритеты: ближний конфликт или короткий прогноз (0.18с, ~0.30м) + помеха справа (ПДД РФ п. 8.9).
        const nearConflict = dist < 0.54 && dot > 0.02 && cross < 0.28
        let predictedConflict = false
        if (!nearConflict && dot > 0.02 && dist < 0.72) {
          const t = 0.18
          const sx = x + dirX * HIVE_CAR_SPEED * t
          const sz = z + dirZ * HIVE_CAR_SPEED * t
          const ox = other.lastX + otherDirX * HIVE_CAR_SPEED * t
          const oz = other.lastZ + otherDirZ * HIVE_CAR_SPEED * t
          if (doCarBoxesOverlap(sx, sz, dirX, dirZ, ox, oz, otherDirX, otherDirZ)) {
            predictedConflict = true
          }
        }
        if (nearConflict || predictedConflict) {
          const otherDot = -dx * otherDirX + -dz * otherDirZ
          if (otherDot <= 0.02) {
            if (selfState) selfState.yieldingTo = i
            return true
          }
          const rightness = dx * -dirZ + dz * dirX
          if (rightness > 0.05) {
            if (selfState) selfState.yieldingTo = i
            return true
          }
          if (Math.abs(rightness) <= 0.05 && selfIndex > i) {
            if (selfState) selfState.yieldingTo = i
            return true
          }
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
  _entryX: number,
  _entryZ: number,
  dirX: number,
  dirZ: number,
  selfIndex: number,
  peers: readonly VehiclePoseState[],
  halfBox = INTERSECTION_BOX_HALF_EXTENT
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

  const axisSeparates = (ux: number, uz: number): boolean => {
    const projDist = Math.abs(dx * ux + dz * uz)
    const ra = halfLength * Math.abs(adx * ux + adz * uz) + halfWidth * Math.abs(-adz * ux + adx * uz)
    const rb = halfLength * Math.abs(bdx * ux + bdz * uz) + halfWidth * Math.abs(-bdz * ux + bdx * uz)
    return projDist >= ra + rb
  }

  if (axisSeparates(adx, adz)) return false
  if (axisSeparates(-adz, adx)) return false
  if (axisSeparates(bdx, bdz)) return false
  if (axisSeparates(-bdz, bdx)) return false
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
  const broadRadiusSq = broadRadius * broadRadius
  for (let i = 0; i < peers.length; i += 1) {
    if (i === selfIndex) continue
    const other = peers[i]
    if (!other || !other.lastVisible) continue
    if (other.lastX === undefined || other.lastZ === undefined) continue
    if (prevX !== undefined && prevZ !== undefined) {
      const aheadDot = (other.lastX - prevX) * dirX + (other.lastZ - prevZ) * dirZ
      if (aheadDot < -0.05) continue
    }
    const ox = other.lastX - x
    const oz = other.lastZ - z
    const nextDistSq = ox * ox + oz * oz
    if (nextDistSq >= broadRadiusSq) continue
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
        const nextDist = Math.sqrt(nextDistSq)
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
  _totalLength: number,
  candidateProgress: number,
  selfIndex: number,
  peers: readonly VehiclePoseState[],
  curX: number,
  curZ: number,
  _lookaheadDist = 0.08
): boolean {
  const clampedP = Math.min(1, Math.max(0, candidateProgress))
  const nextPose = evalSegmentedPose(segments, clampedP)
  return isPoseCollidingWithPeers(
    nextPose.x,
    nextPose.z,
    nextPose.dx,
    nextPose.dz,
    selfIndex,
    peers,
    curX,
    curZ
  )
}
