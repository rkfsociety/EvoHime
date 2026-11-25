import type {
  HoneycombAgentRole,
  HoneycombCargoType,
  HoneycombVehicleDispatch,
  HoneycombWorldState
} from '@shared/hive-city-map'
import { findRoadPath, type RoadGraph } from './honeycomb-geometry'

const TRAFFIC_LIGHT_CYCLE_SEC = 10.0
export const VEHICLE_SPEED_PX_PER_SEC = 105
export const VEHICLE_SAFE_HEADWAY_PX = 24
export const VEHICLE_STOP_LINE_PX = 18

export type TrafficLightPhase =
  | 'ring_green'
  | 'ring_green_blink'
  | 'amber_to_radial'
  | 'radial_green'
  | 'radial_green_blink'
  | 'amber_to_ring'

export function getTrafficLightPhase(timeSec: number): TrafficLightPhase {
  const phase = ((timeSec % TRAFFIC_LIGHT_CYCLE_SEC) + TRAFFIC_LIGHT_CYCLE_SEC) % TRAFFIC_LIGHT_CYCLE_SEC
  if (phase < 3.2) return 'ring_green'
  if (phase < 4.2) return 'ring_green_blink'
  if (phase < 5.0) return 'amber_to_radial'
  if (phase < 8.2) return 'radial_green'
  if (phase < 9.2) return 'radial_green_blink'
  return 'amber_to_ring'
}

export function isTrafficGreenForEdgeKind(
  timeSec: number,
  edgeKind: 'radial' | 'ring' | 'driveway' | 'intercity'
): boolean {
  if (edgeKind === 'driveway' || edgeKind === 'intercity') return true
  const phase = getTrafficLightPhase(timeSec)
  if (edgeKind === 'ring') {
    return phase === 'ring_green' || phase === 'ring_green_blink'
  }
  return phase === 'radial_green' || phase === 'radial_green_blink'
}

/** Явно различимые цвета ролей и грузов для 2D-вида сверху. */
export const ROLE_COLORS: Record<HoneycombAgentRole, string> = {
  executor: '#38bdf8',
  analyst: '#a78bfa',
  strategist: '#f59e0b',
  decision_coordinator: '#34d399'
}

export const CARGO_COLORS: Record<HoneycombCargoType, string> = {
  task: '#38bdf8',
  report: '#818cf8',
  analysis: '#f59e0b',
  decision: '#10b981',
  comms_request: '#f43f5e',
  comms_approved: '#ec4899'
}

export interface ActiveVehicle {
  readonly id: string
  readonly dispatchId: string
  readonly agentId: string
  readonly role: HoneycombAgentRole
  readonly cargo: HoneycombCargoType
  readonly waypoints: readonly string[]
  segmentIndex: number
  segmentProgressPx: number
  x: number
  y: number
  angleRad: number
  waitingAtSignal: boolean
  waitingForCarAhead: boolean
}

export interface TrafficSimulationState {
  readonly vehicles: Map<string, ActiveVehicle>
}

export function createTrafficSimulationState(): TrafficSimulationState {
  return { vehicles: new Map() }
}

function resolveDispatchWaypoints(
  world: HoneycombWorldState,
  graph: RoadGraph,
  dispatch: HoneycombVehicleDispatch
): readonly string[] {
  const primaryCity = world.cities.find((c) => c.id === world.primaryCityId) ?? world.cities[0]
  const queenHubId = primaryCity
    ? primaryCity.hasQueenCitadel
      ? `${primaryCity.id}:citadel`
      : `${primaryCity.id}:hub`
    : dispatch.toNodeId

  if (dispatch.finalTargetNodeId && dispatch.cargo === 'comms_request') {
    // Обязательный заезд к Королеве за разрешением, затем к целевому агенту.
    const leg1 = findRoadPath(graph, dispatch.fromNodeId, queenHubId)
    const leg2 = findRoadPath(graph, queenHubId, dispatch.finalTargetNodeId)
    if (leg1.length > 0 && leg2.length > 1) {
      return [...leg1, ...leg2.slice(1)]
    }
  }
  return findRoadPath(graph, dispatch.fromNodeId, dispatch.toNodeId)
}

/**
 * Шаг ПДД-симуляции машин:
 * - При паузе Улья на дорогах 0 машин (все припаркованы в домах/секторах).
 * - Движение строго по полосам (правостороннее смещение), светофоры и дистанция.
 */
export function stepTrafficSimulation(
  state: TrafficSimulationState,
  world: HoneycombWorldState,
  graph: RoadGraph,
  dtSec: number,
  timeSec: number,
  timeScale: number
): void {
  if (world.paused || world.activeDispatches.length === 0) {
    state.vehicles.clear()
    return
  }

  const activeIds = new Set(world.activeDispatches.map((d) => d.id))
  for (const existingId of state.vehicles.keys()) {
    if (!activeIds.has(existingId)) {
      state.vehicles.delete(existingId)
    }
  }

  let spawnIndex = 0
  for (const dispatch of world.activeDispatches) {
    if (state.vehicles.has(dispatch.id)) {
      spawnIndex += 1
      continue
    }
    const waypoints = resolveDispatchWaypoints(world, graph, dispatch)
    if (waypoints.length < 2) {
      spawnIndex += 1
      continue
    }
    // Сдвигаем стартовый сегмент по маршруту, чтобы машины не скапливались в одной точке на выходе.
    const initialSegment = (spawnIndex * 2) % (waypoints.length - 1)
    const startNode = graph.nodes.get(waypoints[initialSegment] ?? '')
    const nextNode = graph.nodes.get(waypoints[initialSegment + 1] ?? '')
    if (!startNode || !nextNode) {
      spawnIndex += 1
      continue
    }
    const initialSegLen = Math.max(1, Math.hypot(nextNode.x - startNode.x, nextNode.y - startNode.y))
    const initialProgress = Math.min(initialSegLen * 0.45, ((spawnIndex % 3) + 1) * 18)
    state.vehicles.set(dispatch.id, {
      id: dispatch.id,
      dispatchId: dispatch.id,
      agentId: dispatch.agentId,
      role: dispatch.role,
      cargo: dispatch.cargo,
      waypoints,
      segmentIndex: initialSegment,
      segmentProgressPx: initialProgress,
      x: startNode.x,
      y: startNode.y,
      angleRad: Math.atan2(nextNode.y - startNode.y, nextNode.x - startNode.x),
      waitingAtSignal: false,
      waitingForCarAhead: false
    })
    spawnIndex += 1
  }

  const stepPx = VEHICLE_SPEED_PX_PER_SEC * Math.min(0.1, dtSec) * Math.min(20, Math.max(1, timeScale))

  for (const vehicle of state.vehicles.values()) {
    const fromId = vehicle.waypoints[vehicle.segmentIndex]
    const toId = vehicle.waypoints[vehicle.segmentIndex + 1]
    if (!fromId || !toId) continue

    const fromNode = graph.nodes.get(fromId)
    const toNode = graph.nodes.get(toId)
    if (!fromNode || !toNode) continue

    const edge = graph.adjacency.get(fromId)?.find((item) => item.toId === toId)?.edge
    const segLen = Math.max(1, edge?.length ?? Math.hypot(toNode.x - fromNode.x, toNode.y - fromNode.y))
    const remaining = segLen - vehicle.segmentProgressPx

    // Проверка светофора перед въездом на регулируемый перекрёсток.
    let blockedBySignal = false
    if (toNode.kind === 'intersection' && edge && remaining <= VEHICLE_STOP_LINE_PX + stepPx && remaining >= 4) {
      blockedBySignal = !isTrafficGreenForEdgeKind(timeSec, edge.kind)
    }

    // Проверка дистанции до впереди идущей машины на том же отрезке и направлении.
    let blockedByAhead = false
    for (const peer of state.vehicles.values()) {
      if (peer.id === vehicle.id) continue
      const peerFrom = peer.waypoints[peer.segmentIndex]
      const peerTo = peer.waypoints[peer.segmentIndex + 1]
      if (peerFrom === fromId && peerTo === toId && peer.segmentProgressPx > vehicle.segmentProgressPx) {
        if (peer.segmentProgressPx - vehicle.segmentProgressPx < VEHICLE_SAFE_HEADWAY_PX) {
          blockedByAhead = true
          break
        }
      }
    }

    vehicle.waitingAtSignal = blockedBySignal
    vehicle.waitingForCarAhead = blockedByAhead

    if (!blockedBySignal && !blockedByAhead) {
      vehicle.segmentProgressPx += stepPx
      while (vehicle.segmentProgressPx >= segLen) {
        vehicle.segmentProgressPx -= segLen
        vehicle.segmentIndex += 1
        if (vehicle.segmentIndex >= vehicle.waypoints.length - 1) {
          // Циклический повтор рейса на стенде, пока dispatch активен.
          vehicle.segmentIndex = 0
          vehicle.segmentProgressPx = 0
          break
        }
        const nFrom = graph.nodes.get(vehicle.waypoints[vehicle.segmentIndex] ?? '')
        const nTo = graph.nodes.get(vehicle.waypoints[vehicle.segmentIndex + 1] ?? '')
        if (!nFrom || !nTo) break
      }
    }

    const curFrom = graph.nodes.get(vehicle.waypoints[vehicle.segmentIndex] ?? '')
    const curTo = graph.nodes.get(vehicle.waypoints[vehicle.segmentIndex + 1] ?? '')
    if (curFrom && curTo) {
      const curLen = Math.max(1, Math.hypot(curTo.x - curFrom.x, curTo.y - curFrom.y))
      const t = Math.max(0, Math.min(1, vehicle.segmentProgressPx / curLen))
      const angle = Math.atan2(curTo.y - curFrom.y, curTo.x - curFrom.x)
      // Правостороннее смещение по полосе, чтобы встречные машины не ехали друг сквозь друга.
      const laneOffset = 6.5
      const nx = -Math.sin(angle) * laneOffset
      const ny = Math.cos(angle) * laneOffset
      vehicle.x = curFrom.x + (curTo.x - curFrom.x) * t + nx
      vehicle.y = curFrom.y + (curTo.y - curFrom.y) * t + ny
      vehicle.angleRad = angle
    }
  }
}
