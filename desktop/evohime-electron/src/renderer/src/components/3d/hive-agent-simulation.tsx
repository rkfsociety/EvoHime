import { useEffect, useLayoutEffect, useMemo, useRef, type MutableRefObject } from 'react'
import { useFrame } from '@react-three/fiber'
import * as THREE from 'three'
import {
  capitalHouseAngle,
  capitalHouseRingRadius,
  capitalMainAvenueRadius,
  type CityIslandView
} from '@shared/hive-city-map'
import type { HiveCityHouseLot } from './HiveCityScene'
import { FRAME_DUMMY, type CitySkin } from './hive-city-skin'
import {
  CAR_FRONT_BUMPER_OFFSET,
  HIVE_CAR_SPEED,
  INTERSECTION_STOP_HALF_EXTENT,
  QUEEN_GATE_ANGLE,
  QUEEN_PROCESS_SEC,
  RegulatedIntersection3D,
  RoadSegment3D,
  angularDistance,
  approxBezierLength,
  buildMainRingPddSegments,
  computeQueenGateStopAngle,
  computeRingEntryStopProgress,
  evalSegmentedPose,
  evalTurnBezier,
  isRadialTrafficGreen,
  isRingTrafficGreen,
  isIntersectionClearForEntry,
  isPathStepBlocked,
  isPoseCollidingWithPeers,
  isVehicleAheadBlocked,
  posArc,
  type PathSegment,
  type VehiclePoseState
} from './hive-traffic'

export interface AgentCarSim extends VehiclePoseState {
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

interface PrecomputedQueenOutPath {
  readonly segments: readonly PathSegment[]
  readonly totalLength: number
  readonly stopProgress: number
}

interface PrecomputedModuleOutPath {
  readonly segments: readonly PathSegment[]
  readonly totalLength: number
  readonly ringStopProgress: number
  readonly avenueStopProgress: number
}

interface PrecomputedModuleInPath {
  readonly segments: readonly PathSegment[]
  readonly totalLength: number
  readonly avenueStopProgress: number
  readonly ringStopProgress: number
}

interface PrecomputedHouseRoutes {
  readonly queenOutPath: PrecomputedQueenOutPath
  readonly moduleSteps: ReadonlyArray<{
    readonly outPath: PrecomputedModuleOutPath
    readonly inPath: PrecomputedModuleInPath
  }>
}

/**
 * Дороги у двора Королевы и единая симуляция машин агентов (`1 агент = 1 машина`):
 * - ЕДИНСТВЕННЫЙ въезд в Цитадель Королевы (спереди, по +Z);
 * - кольцевой бульвар, к которому подключены съезды от каждого дома;
 * - полный последовательный цикл машины каждого активного агента:
 *   Королева -> Дом -> Модуль (файл) -> Дом -> Следующий файл или Отчёт Королеве;
 * - все кривые Безье и сегменты маршрутов предрассчитаны в `useMemo` без аллокаций в `useFrame`.
 */
export function QueenRoads3D({
  view,
  lots,
  skin,
  deliveredMapRef,
  active,
  paused,
  timeScale = 1,
  simClockRef
}: {
  view: CityIslandView
  lots: readonly HiveCityHouseLot[]
  skin: CitySkin
  deliveredMapRef: React.MutableRefObject<Map<number, boolean>>
  active: boolean
  paused: boolean
  timeScale?: number | undefined
  simClockRef?: MutableRefObject<number> | undefined
}) {
  const ringRadius = capitalHouseRingRadius(lots[0]?.total ?? 0)
  const mainAvenueR = capitalMainAvenueRadius(lots[0]?.total ?? lots.length)
  const innerBoulevardR = Math.max(2.9, ringRadius - 1.25)
  const laneOffset = 0.13
  const gateEntryR = 2.45
  const homeEdgeR = Math.max(innerBoulevardR + 0.35, ringRadius - 0.55)
  const homeGarageR = Math.max(homeEdgeR + 0.35, ringRadius)

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

  // Предрассчитываем все траектории для каждого дома вне useFrame (0 аллокаций в 60 Гц цикле).
  const precomputedRoutes = useMemo<readonly PrecomputedHouseRoutes[]>(() => {
    const innerIntersectionLeadArc = INTERSECTION_STOP_HALF_EXTENT / Math.max(2, innerBoulevardR)

    const buildQueenOutPath = (homeAngle: number): PrecomputedQueenOutPath => {
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
      const gateTurnR = Math.max(gateEntryR + 0.12, innerBoulevardR - INTERSECTION_STOP_HALF_EXTENT)
      const lead1 = Math.min(innerIntersectionLeadArc, arcFromGate * 0.42)
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
      const gateStopDist = Math.max(0, gateApproachLen - CAR_FRONT_BUMPER_OFFSET)
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
      return { segments, totalLength, stopProgress: gateStopDist / totalLength }
    }

    const buildHomeToModulePath = (
      homeAngle: number,
      target: { x: number; z: number }
    ): PrecomputedModuleOutPath => {
      const targetAngle = Math.atan2(target.z, target.x)
      const targetR = Math.hypot(target.x, target.z)
      const targetEntryR = Math.max(mainAvenueR + 0.41, targetR - 0.25)
      const outerLaneR = innerBoulevardR + 0.28
      const innerBorderR = innerBoulevardR + 0.41
      const bridgeStartR = innerBoulevardR + INTERSECTION_STOP_HALF_EXTENT
      const bridgeEndR = mainAvenueR - 0.41
      const bridgeLen = Math.max(0.1, bridgeEndR - bridgeStartR)
      const outHdx = -Math.cos(homeAngle)
      const outHdz = -Math.sin(homeAngle)
      const outBdx = Math.cos(QUEEN_GATE_ANGLE)
      const outBdz = Math.sin(QUEEN_GATE_ANGLE)
      const fullOutArc = posArc(homeAngle - QUEEN_GATE_ANGLE)
      const turnLeadArc = Math.min(innerIntersectionLeadArc, fullOutArc * 0.42)
      const ringArcSpan = Math.max(0, fullOutArc - turnLeadArc)
      const turnStartA = QUEEN_GATE_ANGLE + turnLeadArc
      const bridgeLaneOffset = 0.26
      const tAx = view.x + Math.cos(turnStartA) * outerLaneR
      const tAz = view.z + Math.sin(turnStartA) * outerLaneR
      const tCx = view.x + outBdx * outerLaneR - outBdz * bridgeLaneOffset
      const tCz = view.z + outBdz * outerLaneR + outBdx * bridgeLaneOffset
      const tBx = view.x + outBdx * bridgeStartR - outBdz * bridgeLaneOffset
      const tBz = view.z + outBdz * bridgeStartR + outBdx * bridgeLaneOffset
      const seg0Len = Math.max(0.1, homeGarageR - innerBorderR)
      const seg1Len = ringArcSpan * outerLaneR
      const segTurnLen = approxBezierLength(tAx, tAz, tCx, tCz, tBx, tBz)
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
            const curA = homeAngle - ringArcSpan * u
            return {
              x: view.x + Math.cos(curA) * outerLaneR,
              z: view.z + Math.sin(curA) * outerLaneR,
              dx: Math.sin(curA),
              dz: -Math.cos(curA)
            }
          }
        },
        {
          len: segTurnLen,
          eval: (u) => evalTurnBezier(tAx, tAz, tCx, tCz, tBx, tBz, u)
        },
        {
          len: bridgeLen,
          eval: (u) => {
            const r = bridgeStartR + u * (bridgeEndR - bridgeStartR)
            return {
              x: view.x + Math.cos(QUEEN_GATE_ANGLE) * r - outBdz * bridgeLaneOffset,
              z: view.z + Math.sin(QUEEN_GATE_ANGLE) * r + outBdx * bridgeLaneOffset,
              dx: outBdx,
              dz: outBdz
            }
          }
        },
        ...buildMainRingPddSegments(
          view.x,
          view.z,
          QUEEN_GATE_ANGLE,
          bridgeEndR,
          targetAngle,
          targetEntryR,
          mainAvenueR,
          QUEEN_GATE_ANGLE,
          true,
          bridgeLaneOffset
        )
      ]
      const totalLength = Math.max(
        1.5,
        segments.reduce((sum, seg) => sum + seg.len, 0)
      )
      const ringStopDist = Math.max(0, seg0Len + seg1Len - CAR_FRONT_BUMPER_OFFSET)
      const avenueStopDist = Math.max(
        ringStopDist + 0.25,
        seg0Len + seg1Len + segTurnLen + bridgeLen - (INTERSECTION_STOP_HALF_EXTENT - 0.41 + CAR_FRONT_BUMPER_OFFSET)
      )
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
    ): PrecomputedModuleInPath => {
      const targetAngle = Math.atan2(target.z, target.x)
      const targetR = Math.hypot(target.x, target.z)
      const targetEntryR = Math.max(mainAvenueR + 0.41, targetR - 0.25)
      const outerLaneR = innerBoulevardR + 0.28
      const innerBorderR = innerBoulevardR + 0.41
      const bridgeStartR = mainAvenueR - 0.41
      const bridgeEndR = innerBoulevardR + INTERSECTION_STOP_HALF_EXTENT
      const bridgeLen = Math.max(0.1, bridgeStartR - bridgeEndR)
      const inBdx = -Math.cos(QUEEN_GATE_ANGLE)
      const inBdz = -Math.sin(QUEEN_GATE_ANGLE)
      const inHdx = Math.cos(homeAngle)
      const inHdz = Math.sin(homeAngle)
      const fullInArc = posArc(QUEEN_GATE_ANGLE - homeAngle)
      const turnLeadArc = Math.min(innerIntersectionLeadArc, fullInArc * 0.42)
      const ringArcSpan = Math.max(0, fullInArc - turnLeadArc)
      const turnEndA = QUEEN_GATE_ANGLE - turnLeadArc
      const bridgeLaneOffset = 0.26
      const tAx = view.x + Math.cos(QUEEN_GATE_ANGLE) * bridgeEndR - inBdz * bridgeLaneOffset
      const tAz = view.z + Math.sin(QUEEN_GATE_ANGLE) * bridgeEndR + inBdx * bridgeLaneOffset
      const tCx = view.x + Math.cos(QUEEN_GATE_ANGLE) * outerLaneR - inBdz * bridgeLaneOffset
      const tCz = view.z + Math.sin(QUEEN_GATE_ANGLE) * outerLaneR + inBdx * bridgeLaneOffset
      const tBx = view.x + Math.cos(turnEndA) * outerLaneR
      const tBz = view.z + Math.sin(turnEndA) * outerLaneR
      const segTurnLen = approxBezierLength(tAx, tAz, tCx, tCz, tBx, tBz)
      const mainRingSegs = buildMainRingPddSegments(
        view.x,
        view.z,
        targetAngle,
        targetEntryR,
        QUEEN_GATE_ANGLE,
        bridgeStartR,
        mainAvenueR,
        QUEEN_GATE_ANGLE,
        false,
        bridgeLaneOffset
      )
      // Первый отрезок + кривая + дуга кольца до начала поворота на мост.
      const mainRingBeforeTurnLen =
        (mainRingSegs[0]?.len ?? 0) + (mainRingSegs[1]?.len ?? 0) + (mainRingSegs[2]?.len ?? 0)
      const mainRingLen = mainRingSegs.reduce((sum, seg) => sum + seg.len, 0)
      const segments: PathSegment[] = [
        ...mainRingSegs,
        {
          len: bridgeLen,
          eval: (u) => {
            const r = bridgeStartR - u * (bridgeStartR - bridgeEndR)
            return {
              x: view.x + Math.cos(QUEEN_GATE_ANGLE) * r - inBdz * bridgeLaneOffset,
              z: view.z + Math.sin(QUEEN_GATE_ANGLE) * r + inBdx * bridgeLaneOffset,
              dx: inBdx,
              dz: inBdz
            }
          }
        },
        {
          len: segTurnLen,
          eval: (u) => evalTurnBezier(tAx, tAz, tCx, tCz, tBx, tBz, u)
        },
        {
          len: ringArcSpan * outerLaneR,
          eval: (u) => {
            const curA = turnEndA - ringArcSpan * u
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
      const avenueStopDist = Math.max(0, mainRingBeforeTurnLen - CAR_FRONT_BUMPER_OFFSET)
      const ringStopDist = Math.max(avenueStopDist + 0.25, mainRingLen + bridgeLen - CAR_FRONT_BUMPER_OFFSET)
      return {
        segments,
        totalLength,
        avenueStopProgress: avenueStopDist / totalLength,
        ringStopProgress: ringStopDist / totalLength
      }
    }

    return houseAngles.map((homeAngle) => {
      const hx = Math.cos(homeAngle) * ringRadius
      const hz = Math.sin(homeAngle) * ringRadius
      const sorted = [...moduleBuildings]
        .sort((a, b) => Math.hypot(a.x - hx, a.z - hz) - Math.hypot(b.x - hx, b.z - hz))
        .slice(0, 3)
      return {
        queenOutPath: buildQueenOutPath(homeAngle),
        moduleSteps: sorted.map((target) => ({
          outPath: buildHomeToModulePath(homeAngle, target),
          inPath: buildModuleToHomePath(homeAngle, target)
        }))
      }
    })
  }, [houseAngles, innerBoulevardR, mainAvenueR, moduleBuildings, ringRadius, view.x, view.z, homeEdgeR, homeGarageR])

  const meshRef = useRef<THREE.InstancedMesh>(null)
  const gateLampRef = useRef<THREE.Mesh>(null)
  const instanceColor = useMemo(() => new THREE.Color(), [])

  const carsRef = useRef<AgentCarSim[]>([])
  const queueRef = useRef<number[]>([])
  const processingLotRef = useRef<{ lotIndex: number; remaining: number } | null>(null)
  const layoutSignatureRef = useRef<string>('')
  const simTimeRef = useRef<number>(0)

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
  }, [lots, instanceColor, deliveredMapRef, moduleBuildings.length])

  useFrame((_, delta) => {
    const mesh = meshRef.current
    if (!mesh) return
    const dummy = FRAME_DUMMY
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
      lots.forEach((lot, index) => {
        if (lot.active) deliveredMapRef.current.set(lot.slot, true)
        if (index >= mesh.count) return
        const homeAngle = houseAngles[index]
        if (!lot.active || homeAngle === undefined) {
          dummy.scale.setScalar(0)
          dummy.updateMatrix()
          mesh.setMatrixAt(index, dummy.matrix)
          return
        }
        const x = view.x + Math.cos(homeAngle) * (innerBoulevardR + 0.13)
        const z = view.z + Math.sin(homeAngle) * (innerBoulevardR + 0.13)
        dummy.position.set(x, 0.12, z)
        dummy.lookAt(x - Math.sin(homeAngle), 0.12, z + Math.cos(homeAngle))
        dummy.scale.set(0.15, 0.11, 0.36)
        dummy.updateMatrix()
        mesh.setMatrixAt(index, dummy.matrix)
      })
      mesh.instanceMatrix.needsUpdate = true
      return
    }

    const clampedScale = Math.max(1, Math.min(100, timeScale))
    const totalDt = Math.min(delta, 0.05) * clampedScale
    const subSteps = Math.min(48, Math.max(1, Math.ceil(totalDt / 0.04)))
    const dt = totalDt / subSteps

    const MIN_CAR_ARC_GAP = Math.max(0.15, (0.36 + 0.14) / innerBoulevardR)
    const GATE_STOP_ANGLE = computeQueenGateStopAngle(innerBoulevardR)
    const RING_ENTRY_STOP_PROGRESS = computeRingEntryStopProgress(innerBoulevardR, homeGarageR)
    const innerIx = view.x + Math.cos(QUEEN_GATE_ANGLE) * innerBoulevardR
    const innerIz = view.z + Math.sin(QUEEN_GATE_ANGLE) * innerBoulevardR
    const avenueIx = view.x + Math.cos(QUEEN_GATE_ANGLE) * mainAvenueR
    const avenueIz = view.z + Math.sin(QUEEN_GATE_ANGLE) * mainAvenueR

    for (let step = 0; step < subSteps; step += 1) {
      simTimeRef.current += dt
      const ringGreen = isRingTrafficGreen(simTimeRef.current)
      const radialGreen = isRadialTrafficGreen(simTimeRef.current)

      // Шаг 1: обновляем обработку текущей машины у Королевы (строго 1 машина за раз!).
      if (processingLotRef.current !== null) {
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

      // Если ворота Королевы свободны — запускаем ТОЛЬКО самую ближнюю к воротам машину у стоп-линии!
      if (processingLotRef.current === null) {
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

      // Шаг 2: двигаем каждую машину (с запретом проезда сквозь впереди идущую машину и OBB-коллизий).
      lots.forEach((lot, index) => {
        if (index >= mesh.count) return
        const car = cars[index]
        const homeAngle = houseAngles[index]
        const routes = precomputedRoutes[index]
        if (!car || homeAngle === undefined || !routes) return

        if (!lot.active) {
          deliveredMapRef.current.delete(lot.slot)
          car.lastVisible = false
          car.inIntersection = false
          return
        }

        let x = view.x
        let z = view.z
        let dirX = 0
        let dirZ = 1
        let visible = true

      if (car.phase === 'delivering') {
        const queenOutPath = routes.queenOutPath
        if (car.cooldown > 0) {
          car.cooldown -= dt
          visible = false
        } else {
          const curPose = evalSegmentedPose(queenOutPath.segments, Math.min(1, Math.max(0, car.progress)))
          if (
            !car.lastVisible &&
            isPoseCollidingWithPeers(curPose.x, curPose.z, curPose.dx, curPose.dz, index, cars)
          ) {
            visible = false
          } else {
            const aheadBlocked =
              car.lastVisible === true &&
              isVehicleAheadBlocked(
                curPose.x,
                curPose.z,
                curPose.dx,
                curPose.dz,
                index,
                cars,
                0.56,
                car.progress > 0.9 ? 'driveway' : 'main'
              )
            if (!aheadBlocked) {
              const nextP = car.progress + dt * (HIVE_CAR_SPEED / queenOutPath.totalLength)
              const canEnterInner =
                radialGreen &&
                isIntersectionClearForEntry(
                  innerIx,
                  innerIz,
                  curPose.x,
                  curPose.z,
                  curPose.dx,
                  curPose.dz,
                  index,
                  cars
                )
              const candidateP =
                !canEnterInner && car.progress <= queenOutPath.stopProgress && nextP >= queenOutPath.stopProgress
                  ? queenOutPath.stopProgress
                  : nextP
              if (
                !isPathStepBlocked(
                  queenOutPath.segments,
                  queenOutPath.totalLength,
                  candidateP,
                  index,
                  cars,
                  curPose.x,
                  curPose.z
                )
              ) {
                car.progress = candidateP
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
          car.phase = routes.moduleSteps.length > 0 ? 'to_module' : 'to_ring'
          car.progress = 0
        }
      } else if (car.phase === 'to_module') {
        const modStep = routes.moduleSteps[car.moduleStep % Math.max(1, routes.moduleSteps.length)]
        if (!modStep) {
          car.phase = 'to_ring'
          car.progress = 0
        } else {
          const modOutPath = modStep.outPath
          const curPose = evalSegmentedPose(modOutPath.segments, Math.min(1, Math.max(0, car.progress)))
          if (
            !car.lastVisible &&
            isPoseCollidingWithPeers(curPose.x, curPose.z, curPose.dx, curPose.dz, index, cars)
          ) {
            visible = false
            x = curPose.x
            z = curPose.z
            dirX = curPose.dx
            dirZ = curPose.dz
          } else {
            const aheadBlocked =
              car.lastVisible === true &&
              isVehicleAheadBlocked(
                curPose.x,
                curPose.z,
                curPose.dx,
                curPose.dz,
                index,
                cars,
                0.56,
                car.progress > 0.9 ? 'driveway' : 'main'
              )
            if (!aheadBlocked) {
              const nextP = car.progress + dt * (HIVE_CAR_SPEED / modOutPath.totalLength)
              const canEnterInner =
                ringGreen &&
                isIntersectionClearForEntry(
                  innerIx,
                  innerIz,
                  curPose.x,
                  curPose.z,
                  curPose.dx,
                  curPose.dz,
                  index,
                  cars
                )
              const canEnterAvenue =
                radialGreen &&
                isIntersectionClearForEntry(
                  avenueIx,
                  avenueIz,
                  curPose.x,
                  curPose.z,
                  curPose.dx,
                  curPose.dz,
                  index,
                  cars
                )
              let candidateP = nextP
              if (
                !canEnterInner &&
                car.progress <= modOutPath.ringStopProgress &&
                nextP >= modOutPath.ringStopProgress
              ) {
                candidateP = modOutPath.ringStopProgress
              } else if (
                !canEnterAvenue &&
                car.progress <= modOutPath.avenueStopProgress &&
                nextP >= modOutPath.avenueStopProgress
              ) {
                candidateP = modOutPath.avenueStopProgress
              }
              if (
                !isPathStepBlocked(
                  modOutPath.segments,
                  modOutPath.totalLength,
                  candidateP,
                  index,
                  cars,
                  curPose.x,
                  curPose.z
                )
              ) {
                car.progress = candidateP
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
        }
      } else if (car.phase === 'at_module') {
        const modStep = routes.moduleSteps[car.moduleStep % Math.max(1, routes.moduleSteps.length)]
        visible = false
        if (modStep) {
          const pose = evalSegmentedPose(modStep.outPath.segments, 1)
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
        const modStep = routes.moduleSteps[car.moduleStep % Math.max(1, routes.moduleSteps.length)]
        if (!modStep) {
          car.phase = 'to_ring'
          car.progress = 0
        } else {
          const modInPath = modStep.inPath
          const curPose = evalSegmentedPose(modInPath.segments, Math.min(1, Math.max(0, car.progress)))
          if (
            !car.lastVisible &&
            isPoseCollidingWithPeers(curPose.x, curPose.z, curPose.dx, curPose.dz, index, cars)
          ) {
            visible = false
            x = curPose.x
            z = curPose.z
            dirX = curPose.dx
            dirZ = curPose.dz
          } else {
            const aheadBlocked =
              car.lastVisible === true &&
              isVehicleAheadBlocked(
                curPose.x,
                curPose.z,
                curPose.dx,
                curPose.dz,
                index,
                cars,
                0.56,
                car.progress > 0.9 ? 'driveway' : 'main'
              )
            if (!aheadBlocked) {
              const nextP = car.progress + dt * (HIVE_CAR_SPEED / modInPath.totalLength)
              const canEnterAvenue =
                ringGreen &&
                isIntersectionClearForEntry(
                  avenueIx,
                  avenueIz,
                  curPose.x,
                  curPose.z,
                  curPose.dx,
                  curPose.dz,
                  index,
                  cars
                )
              const canEnterInner =
                radialGreen &&
                isIntersectionClearForEntry(
                  innerIx,
                  innerIz,
                  curPose.x,
                  curPose.z,
                  curPose.dx,
                  curPose.dz,
                  index,
                  cars
                )
              let candidateP = nextP
              if (
                !canEnterAvenue &&
                car.progress <= modInPath.avenueStopProgress &&
                nextP >= modInPath.avenueStopProgress
              ) {
                candidateP = modInPath.avenueStopProgress
              } else if (
                !canEnterInner &&
                car.progress <= modInPath.ringStopProgress &&
                nextP >= modInPath.ringStopProgress
              ) {
                candidateP = modInPath.ringStopProgress
              }
              if (
                !isPathStepBlocked(
                  modInPath.segments,
                  modInPath.totalLength,
                  candidateP,
                  index,
                  cars,
                  curPose.x,
                  curPose.z
                )
              ) {
                car.progress = candidateP
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
          if (car.moduleStep < car.moduleTotalSteps && routes.moduleSteps.length > 0) {
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
        const targetEntryAngle = homeAngle - 0.06
        const targetLaneOffset = 0.3
        const targetLaneR = innerBoulevardR + targetLaneOffset
        const evalToRingPose = (p: number) => {
          const clampedP = Math.min(1, Math.max(0, p))
          if (clampedP <= RING_ENTRY_STOP_PROGRESS) {
            const r = homeGarageR - clampedP * (homeGarageR - innerBoulevardR)
            return {
              x: view.x + Math.cos(homeAngle) * r - hdz * laneOffset,
              z: view.z + Math.sin(homeAngle) * r + hdx * laneOffset,
              dx: hdx,
              dz: hdz
            }
          }
          const turnU = (clampedP - RING_ENTRY_STOP_PROGRESS) / Math.max(0.05, 1 - RING_ENTRY_STOP_PROGRESS)
          const stopR = homeGarageR - RING_ENTRY_STOP_PROGRESS * (homeGarageR - innerBoulevardR)
          const ax = view.x + Math.cos(homeAngle) * stopR - hdz * laneOffset
          const az = view.z + Math.sin(homeAngle) * stopR + hdx * laneOffset
          const cx = view.x + Math.cos(homeAngle) * targetLaneR - hdz * laneOffset
          const cz = view.z + Math.sin(homeAngle) * targetLaneR + hdx * laneOffset
          const bx = view.x + Math.cos(targetEntryAngle) * targetLaneR
          const bz = view.z + Math.sin(targetEntryAngle) * targetLaneR
          return evalTurnBezier(ax, az, cx, cz, bx, bz, turnU)
        }

        if (car.cooldown > 0) {
          car.cooldown -= dt
          visible = false
          dirX = hdx
          dirZ = hdz
          x = view.x + Math.cos(homeAngle) * homeGarageR - dirZ * laneOffset
          z = view.z + Math.sin(homeAngle) * homeGarageR + dirX * laneOffset
        } else {
          const curPose = evalToRingPose(car.progress)
          if (
            !car.lastVisible &&
            isPoseCollidingWithPeers(curPose.x, curPose.z, curPose.dx, curPose.dz, index, cars)
          ) {
            visible = false
            x = curPose.x
            z = curPose.z
            dirX = curPose.dx
            dirZ = curPose.dz
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
            let candidateP = car.progress
            if (car.progress > RING_ENTRY_STOP_PROGRESS) {
              candidateP = Math.min(1, nextProgress)
            } else if (car.progress < RING_ENTRY_STOP_PROGRESS) {
              candidateP = ringBlocked ? Math.min(RING_ENTRY_STOP_PROGRESS, nextProgress) : Math.min(1, nextProgress)
            } else if (!ringBlocked) {
              candidateP = Math.min(1, nextProgress)
            }
            const nextPose = evalToRingPose(candidateP)
            const lookPose = evalToRingPose(Math.min(1, candidateP + 0.16 / toRingLen))
            const obbBlocked =
              isPoseCollidingWithPeers(
                nextPose.x,
                nextPose.z,
                nextPose.dx,
                nextPose.dz,
                index,
                cars,
                car.lastVisible ? curPose.x : undefined,
                car.lastVisible ? curPose.z : undefined
              ) ||
              (car.lastVisible &&
                isPoseCollidingWithPeers(
                  lookPose.x,
                  lookPose.z,
                  lookPose.dx,
                  lookPose.dz,
                  index,
                  cars,
                  curPose.x,
                  curPose.z
                ))
            if (!obbBlocked) {
              car.progress = candidateP
            }
            const finalPose = evalToRingPose(car.progress)
            x = finalPose.x
            z = finalPose.z
            dirX = finalPose.dx
            dirZ = finalPose.dz
            if (car.progress >= 1) {
              car.phase = 'outer_to_uturn'
              car.progress = 0
              car.curAngle = targetEntryAngle
              car.curLaneOffset = targetLaneOffset
            }
          }
        }
      } else if (car.phase === 'outer_to_uturn') {
        const arcToIntersection = posArc(car.curAngle - QUEEN_GATE_ANGLE)
        const intersectionEntryArc = INTERSECTION_STOP_HALF_EXTENT / innerBoulevardR
        const intersectionStopArc =
          (INTERSECTION_STOP_HALF_EXTENT + CAR_FRONT_BUMPER_OFFSET) / innerBoulevardR
        const curRingX = view.x + Math.cos(car.curAngle) * (innerBoulevardR + car.curLaneOffset)
        const curRingZ = view.z + Math.sin(car.curAngle) * (innerBoulevardR + car.curLaneOffset)
        const curRingDx = Math.sin(car.curAngle)
        const curRingDz = -Math.cos(car.curAngle)
        const canEnterInner =
          ringGreen &&
          isIntersectionClearForEntry(
            innerIx,
            innerIz,
            curRingX,
            curRingZ,
            curRingDx,
            curRingDz,
            index,
            cars
          )
        let maxTravel = arcToIntersection
        if (!canEnterInner && arcToIntersection >= intersectionStopArc - 0.015) {
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
        const uTurnBlocked =
          arcToIntersection <= intersectionStopArc + 0.02 &&
          cars.some((other, oIdx) => oIdx !== index && other.phase === 'queued')
        if (
          car.lastVisible &&
          isVehicleAheadBlocked(curRingX, curRingZ, curRingDx, curRingDz, index, cars)
        ) {
          maxTravel = 0
        }
        const outerAngularSpeed = HIVE_CAR_SPEED / Math.max(1.5, innerBoulevardR + car.curLaneOffset)
        let allowedStep = uTurnBlocked ? 0 : Math.min(Math.max(0, maxTravel), dt * outerAngularSpeed)
        const desiredOuterOffset = 0.13
        if (allowedStep > 0) {
          const candAngle = car.curAngle - allowedStep
          const candArcToIntersection = Math.max(0, arcToIntersection - allowedStep)
          const candUTurnZone =
            candArcToIntersection < intersectionEntryArc
              ? 1 - candArcToIntersection / intersectionEntryArc
              : 0
          let candX = curRingX
          let candZ = curRingZ
          let candDx = curRingDx
          let candDz = curRingDz
          if (candUTurnZone > 0) {
            const gdx = -Math.cos(QUEEN_GATE_ANGLE)
            const gdz = -Math.sin(QUEEN_GATE_ANGLE)
            const enterR = innerBoulevardR - 0.52
            const ax = view.x + Math.cos(QUEEN_GATE_ANGLE + intersectionEntryArc) * (innerBoulevardR + 0.13)
            const az = view.z + Math.sin(QUEEN_GATE_ANGLE + intersectionEntryArc) * (innerBoulevardR + 0.13)
            const cx = view.x + Math.cos(QUEEN_GATE_ANGLE) * innerBoulevardR
            const cz = view.z + Math.sin(QUEEN_GATE_ANGLE) * innerBoulevardR
            const bx = view.x + Math.cos(QUEEN_GATE_ANGLE) * enterR - gdz * laneOffset
            const bz = view.z + Math.sin(QUEEN_GATE_ANGLE) * enterR + gdx * laneOffset
            const candPose = evalTurnBezier(ax, az, cx, cz, bx, bz, candUTurnZone)
            candX = candPose.x
            candZ = candPose.z
            candDx = candPose.dx
            candDz = candPose.dz
          } else {
            const laneDiff = desiredOuterOffset - car.curLaneOffset
            const arcMove = allowedStep * (innerBoulevardR + car.curLaneOffset)
            const maxStep = arcMove * 0.36
            const laneStep = Math.abs(laneDiff) <= maxStep ? laneDiff : Math.sign(laneDiff) * maxStep
            const candLaneR = innerBoulevardR + car.curLaneOffset + laneStep
            candX = view.x + Math.cos(candAngle) * candLaneR
            candZ = view.z + Math.sin(candAngle) * candLaneR
            candDx = Math.sin(candAngle)
            candDz = -Math.cos(candAngle)
          }
          if (
            isPoseCollidingWithPeers(
              candX,
              candZ,
              candDx,
              candDz,
              index,
              cars,
              car.lastVisible ? (car.lastX ?? curRingX) : undefined,
              car.lastVisible ? (car.lastZ ?? curRingZ) : undefined
            )
          ) {
            allowedStep = 0
          }
        }
        car.curAngle -= allowedStep
        const newArcToIntersection = Math.max(0, arcToIntersection - allowedStep)
        if (newArcToIntersection <= 0.01 && !uTurnBlocked && allowedStep > 0) {
          car.phase = 'queued'
          car.progress = 0
          car.curAngle = QUEEN_GATE_ANGLE
          car.curLaneOffset = 0.11
          if (!queue.includes(index)) queue.push(index)
        }
        const uTurnZone =
          newArcToIntersection < intersectionEntryArc ? 1 - newArcToIntersection / intersectionEntryArc : 0
        if (uTurnZone > 0) {
          const gdx = -Math.cos(QUEEN_GATE_ANGLE)
          const gdz = -Math.sin(QUEEN_GATE_ANGLE)
          const enterR = innerBoulevardR - 0.52
          const ax = view.x + Math.cos(QUEEN_GATE_ANGLE + intersectionEntryArc) * (innerBoulevardR + 0.13)
          const az = view.z + Math.sin(QUEEN_GATE_ANGLE + intersectionEntryArc) * (innerBoulevardR + 0.13)
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
        const nextProgress = Math.min(1, car.progress + dt * (HIVE_CAR_SPEED / procLen))
        const gdx = -Math.cos(QUEEN_GATE_ANGLE)
        const gdz = -Math.sin(QUEEN_GATE_ANGLE)
        const curR = enterR - car.progress * (enterR - gateEntryR)
        const curProcX = view.x + Math.cos(QUEEN_GATE_ANGLE) * curR - gdz * laneOffset
        const curProcZ = view.z + Math.sin(QUEEN_GATE_ANGLE) * curR + gdx * laneOffset
        const candR = enterR - nextProgress * (enterR - gateEntryR)
        const candProcX = view.x + Math.cos(QUEEN_GATE_ANGLE) * candR - gdz * laneOffset
        const candProcZ = view.z + Math.sin(QUEEN_GATE_ANGLE) * candR + gdx * laneOffset
        if (
          !isPoseCollidingWithPeers(
            candProcX,
            candProcZ,
            gdx,
            gdz,
            index,
            cars,
            car.lastVisible ? curProcX : undefined,
            car.lastVisible ? curProcZ : undefined
          )
        ) {
          car.progress = nextProgress
        }
        const r = enterR - car.progress * (enterR - gateEntryR)
        dirX = gdx
        dirZ = gdz
        x = view.x + Math.cos(QUEEN_GATE_ANGLE) * r - dirZ * laneOffset
        z = view.z + Math.sin(QUEEN_GATE_ANGLE) * r + dirX * laneOffset
      } else if (car.phase === 'returning_ring') {
        const queenOutPath = routes.queenOutPath
        const curPose = evalSegmentedPose(queenOutPath.segments, Math.min(1, Math.max(0, car.progress)))
        if (
          !car.lastVisible &&
          isPoseCollidingWithPeers(curPose.x, curPose.z, curPose.dx, curPose.dz, index, cars)
        ) {
          visible = false
        } else {
          const aheadBlocked =
            car.lastVisible === true &&
            isVehicleAheadBlocked(curPose.x, curPose.z, curPose.dx, curPose.dz, index, cars)
          if (!aheadBlocked) {
            const nextP = car.progress + dt * (HIVE_CAR_SPEED / queenOutPath.totalLength)
            const canEnterInner =
              radialGreen &&
              isIntersectionClearForEntry(
                innerIx,
                innerIz,
                curPose.x,
                curPose.z,
                curPose.dx,
                curPose.dz,
                index,
                cars
              )
            const candidateP =
              !canEnterInner && car.progress <= queenOutPath.stopProgress && nextP >= queenOutPath.stopProgress
                ? queenOutPath.stopProgress
                : nextP
            if (
              !isPathStepBlocked(
                queenOutPath.segments,
                queenOutPath.totalLength,
                candidateP,
                index,
                cars,
                curPose.x,
                curPose.z
              )
            ) {
              car.progress = candidateP
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
        }
        const t = Math.min(1, car.progress)
        const pose = evalSegmentedPose(queenOutPath.segments, t)
        x = pose.x
        z = pose.z
        dirX = pose.dx
        dirZ = pose.dz
      } else {
        visible = false
        dirX = -Math.cos(homeAngle)
        dirZ = -Math.sin(homeAngle)
        x = view.x + Math.cos(homeAngle) * homeGarageR - dirZ * laneOffset
        z = view.z + Math.sin(homeAngle) * homeGarageR + dirX * laneOffset
        car.cooldown -= dt
        if (car.cooldown <= 0) {
          car.phase = routes.moduleSteps.length > 0 ? 'to_module' : 'to_ring'
          car.progress = 0
        }
      }

      const moved =
        car.lastX !== undefined &&
        car.lastZ !== undefined &&
        Math.hypot(x - car.lastX, z - car.lastZ) > 1e-4
      if (!visible || moved) {
        car.blockedSec = 0
        car.yieldingTo = undefined
      } else {
        car.blockedSec = Math.min(10, (car.blockedSec ?? 0) + dt)
      }

      const inInner = Math.abs(x - innerIx) < 0.56 && Math.abs(z - innerIz) < 0.56
      const inAvenue = Math.abs(x - avenueIx) < 0.56 && Math.abs(z - avenueIz) < 0.56
      car.inIntersection = visible && (inInner || inAvenue)

      car.lastVisible = visible
      car.lastX = x
      car.lastZ = z
      car.lastDirX = dirX
      car.lastDirZ = dirZ
      car.roadPriority =
        (car.phase === 'to_ring' && car.progress <= RING_ENTRY_STOP_PROGRESS) ||
        (car.phase === 'delivering' && car.progress > 0.9) ||
        ((car.phase === 'to_module' || car.phase === 'from_module') &&
          (car.progress < 0.08 || car.progress > 0.92))
          ? 'driveway'
          : 'main'
      })
    }

    if (gateLampRef.current) {
      gateLampRef.current.material = processingLotRef.current !== null ? gateSignalAmber : gateSignalGreen
    }
    if (simClockRef) {
      simClockRef.current = simTimeRef.current
    }

    lots.forEach((lot, index) => {
      if (index >= mesh.count) return
      const car = cars[index]
      if (
        !lot.active ||
        !car ||
        !car.lastVisible ||
        car.lastX === undefined ||
        car.lastZ === undefined ||
        car.lastDirX === undefined ||
        car.lastDirZ === undefined
      ) {
        dummy.scale.setScalar(0)
        dummy.updateMatrix()
        mesh.setMatrixAt(index, dummy.matrix)
        return
      }

      const carY = 0.13 + (index % 7) * 0.0008
      dummy.position.set(car.lastX, carY, car.lastZ)
      dummy.lookAt(car.lastX + car.lastDirX, carY, car.lastZ + car.lastDirZ)
      dummy.scale.set(0.16, 0.11, 0.36)
      dummy.updateMatrix()
      mesh.setMatrixAt(index, dummy.matrix)
    })
    mesh.instanceMatrix.needsUpdate = true
  })

  if (lots.length === 0) return <></>
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
