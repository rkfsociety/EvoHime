import type {
  HoneycombCity,
  HoneycombHexCoord,
  HoneycombWorldState
} from '@shared/hive-city-map'

export const CITADEL_HEX_RADIUS = 46
export const CELL_HEX_RADIUS = 30
export const RING_BASE_RADIUS = 74
export const RING_STEP = 98

/** Углы нормалей к 6 граням pointy-top гексагона (строго перпендикулярно граням). */
export const HEX_FACE_NORMAL_ANGLES: readonly number[] = [
  0,
  Math.PI / 3,
  (2 * Math.PI) / 3,
  Math.PI,
  (4 * Math.PI) / 3,
  (5 * Math.PI) / 3
]

export interface Point2D {
  readonly x: number
  readonly y: number
}

export interface RoadNode {
  readonly id: string
  readonly cityId: string
  readonly x: number
  readonly y: number
  readonly kind: 'citadel_hub' | 'exit_gate' | 'intersection' | 'cell_entry' | 'intercity_gate'
  readonly ring: number
  readonly rayIndex?: number | undefined
}

export interface RoadEdge {
  readonly id: string
  readonly fromId: string
  readonly toId: string
  readonly kind: 'radial' | 'ring' | 'driveway' | 'intercity'
  readonly length: number
  readonly lanes: 2 | 4
}

export interface RoadGraph {
  readonly nodes: ReadonlyMap<string, RoadNode>
  readonly edges: readonly RoadEdge[]
  readonly adjacency: ReadonlyMap<string, readonly { readonly toId: string; readonly edge: RoadEdge }[]>
}

export interface HexCellPlacement {
  readonly id: string
  readonly cityId: string
  readonly kind: 'citadel' | 'project_hub' | 'sector' | 'module'
  readonly ring: number
  readonly x: number
  readonly y: number
  readonly radius: number
  readonly exitAngles: readonly number[]
}

/** Вычисляет радиус кольцевой магистрали для заданного кольца `ring >= 1`. */
export function getRingRoadRadius(ring: number): number {
  return RING_BASE_RADIUS + ring * RING_STEP - RING_STEP * 0.42
}

/** Вычисляет радиус центров сот на заданном кольце `ring >= 1`. */
export function getRingCellRadius(ring: number): number {
  return RING_BASE_RADIUS + ring * RING_STEP
}

/**
 * Выбирает индексы граней (0..5) для `exitsCount` выходов Цитадели так,
 * чтобы при любом количестве 1..6 выходы распределялись симметрично.
 */
export function selectCitadelExitFaceIndices(exitsCount: number): readonly number[] {
  const clamped = Math.max(1, Math.min(6, Math.round(exitsCount)))
  switch (clamped) {
    case 1:
      return [0]
    case 2:
      return [0, 3]
    case 3:
      return [0, 2, 4]
    case 4:
      return [0, 1, 3, 4]
    case 5:
      return [0, 1, 2, 3, 4]
    default:
      return [0, 1, 2, 3, 4, 5]
  }
}

/** Возвращает 6 вершин pointy-top гексагона с центром `(cx, cy)` и радиусом `radius`. */
export function getPointyHexVertices(cx: number, cy: number, radius: number): readonly Point2D[] {
  const vertices: Point2D[] = []
  for (let i = 0; i < 6; i += 1) {
    const angle = (Math.PI / 6) + (i * Math.PI) / 3
    vertices.push({
      x: cx + Math.cos(angle) * radius,
      y: cy + Math.sin(angle) * radius
    })
  }
  return vertices
}

/**
 * Переводит гекс-координаты `(q, r)` и индекс на кольце в плавную 2D-позицию
 * вокруг центра города.
 */
export function resolveCellPositionOnRing(
  city: HoneycombCity,
  ring: number,
  coord: HoneycombHexCoord,
  indexInRing: number,
  totalInRing: number
): Point2D {
  const radius = getRingCellRadius(ring)
  const count = Math.max(1, totalInRing)
  // Детерминированный угол с небольшим сдвигом от axial-координат, чтобы при
  // перестройке сохранялась логика раскладки без наложений.
  const baseAngle = ((indexInRing + 0.5) / count) * Math.PI * 2
  const axialBias = ((coord.q * 7 + coord.r * 13) % 11) * 0.004
  const angle = baseAngle + axialBias
  return {
    x: city.worldCenterX + Math.cos(angle) * radius,
    y: city.worldCenterY + Math.sin(angle) * radius
  }
}

/** Строит 2D-размещение всех сот города и граф дорожной сети с ПДД-узлами. */
export function buildWorldRoadGraphAndPlacements(world: HoneycombWorldState): {
  readonly placements: ReadonlyMap<string, HexCellPlacement>
  readonly roadGraph: RoadGraph
} {
  const placements = new Map<string, HexCellPlacement>()
  const nodes = new Map<string, RoadNode>()
  const edges: RoadEdge[] = []
  const adjacency = new Map<string, { toId: string; edge: RoadEdge }[]>()

  const addNode = (node: RoadNode): void => {
    nodes.set(node.id, node)
    if (!adjacency.has(node.id)) {
      adjacency.set(node.id, [])
    }
  }

  const addBidirectionalEdge = (
    idPrefix: string,
    aId: string,
    bId: string,
    kind: RoadEdge['kind'],
    lanes: 2 | 4
  ): void => {
    const a = nodes.get(aId)
    const b = nodes.get(bId)
    if (!a || !b) return
    const length = Math.hypot(b.x - a.x, b.y - a.y)
    if (length < 1e-3) return
    const edgeForward: RoadEdge = { id: `${idPrefix}:f`, fromId: aId, toId: bId, kind, length, lanes }
    const edgeBackward: RoadEdge = { id: `${idPrefix}:b`, fromId: bId, toId: aId, kind, length, lanes }
    edges.push(edgeForward)
    adjacency.get(aId)?.push({ toId: bId, edge: edgeForward })
    adjacency.get(bId)?.push({ toId: aId, edge: edgeBackward })
  }

  for (const city of world.cities) {
    const hubId = city.hasQueenCitadel ? `${city.id}:citadel` : `${city.id}:hub`
    const exitFaces = selectCitadelExitFaceIndices(city.hasQueenCitadel ? city.citadelExitsCount : 2)
    const exitAngles = exitFaces.map((idx) => HEX_FACE_NORMAL_ANGLES[idx] ?? 0)

    placements.set(hubId, {
      id: hubId,
      cityId: city.id,
      kind: city.hasQueenCitadel ? 'citadel' : 'project_hub',
      ring: 0,
      x: city.worldCenterX,
      y: city.worldCenterY,
      radius: city.hasQueenCitadel ? CITADEL_HEX_RADIUS : CITADEL_HEX_RADIUS * 0.78,
      exitAngles
    })

    addNode({
      id: hubId,
      cityId: city.id,
      x: city.worldCenterX,
      y: city.worldCenterY,
      kind: 'citadel_hub',
      ring: 0
    })

    const maxRing = Math.max(
      city.maxRing,
      city.sectors.length > 0 ? 1 : 0,
      ...city.modules.map((m) => m.ring)
    )

    // Гексагональный апофемный радиус точки выхода на грани Цитадели.
    const hubRadius = city.hasQueenCitadel ? CITADEL_HEX_RADIUS : CITADEL_HEX_RADIUS * 0.78
    const apothem = hubRadius * Math.cos(Math.PI / 6)

    for (const faceIdx of exitFaces) {
      const angle = HEX_FACE_NORMAL_ANGLES[faceIdx] ?? 0
      const gateId = `${city.id}:gate:${faceIdx}`
      addNode({
        id: gateId,
        cityId: city.id,
        x: city.worldCenterX + Math.cos(angle) * apothem,
        y: city.worldCenterY + Math.sin(angle) * apothem,
        kind: 'exit_gate',
        ring: 0,
        rayIndex: faceIdx
      })
      addBidirectionalEdge(`${city.id}:hub-gate:${faceIdx}`, hubId, gateId, 'radial', 4)
    }

    // Кольцевые магистрали и пересечения с активными радиальными лучами.
    const ringAnchorSegments = 12
    for (let ring = 1; ring <= maxRing; ring += 1) {
      const roadRadius = getRingRoadRadius(ring)
      const ringAngles = new Set<number>(exitFaces.map((idx) => HEX_FACE_NORMAL_ANGLES[idx] ?? 0))
      for (let s = 0; s < ringAnchorSegments; s += 1) {
        ringAngles.add((s / ringAnchorSegments) * Math.PI * 2)
      }
      const sortedAngles = Array.from(ringAngles).sort((a, b) => a - b)
      const ringNodeIds: string[] = []

      sortedAngles.forEach((angle, idx) => {
        const rayIdx = exitFaces.find((face) => Math.abs((HEX_FACE_NORMAL_ANGLES[face] ?? 0) - angle) < 0.01)
        const nodeId = `${city.id}:ring:${ring}:n:${idx}`
        addNode({
          id: nodeId,
          cityId: city.id,
          x: city.worldCenterX + Math.cos(angle) * roadRadius,
          y: city.worldCenterY + Math.sin(angle) * roadRadius,
          kind: 'intersection',
          ring,
          rayIndex: rayIdx
        })
        ringNodeIds.push(nodeId)

        // Соединяем радиальный луч с предыдущим кольцом (или с выходом Цитадели).
        if (rayIdx !== undefined) {
          if (ring === 1) {
            addBidirectionalEdge(`${city.id}:ray:${rayIdx}:r0-1`, `${city.id}:gate:${rayIdx}`, nodeId, 'radial', 4)
          } else {
            const prevRingRayNode = Array.from(nodes.values()).find(
              (n) => n.cityId === city.id && n.ring === ring - 1 && n.rayIndex === rayIdx
            )
            if (prevRingRayNode) {
              addBidirectionalEdge(
                `${city.id}:ray:${rayIdx}:r${ring - 1}-${ring}`,
                prevRingRayNode.id,
                nodeId,
                'radial',
                4
              )
            }
          }
        }
      })

      for (let idx = 0; idx < ringNodeIds.length; idx += 1) {
        const curr = ringNodeIds[idx]
        const next = ringNodeIds[(idx + 1) % ringNodeIds.length]
        if (curr && next) {
          addBidirectionalEdge(`${city.id}:ring:${ring}:seg:${idx}`, curr, next, 'ring', 4)
        }
      }
    }

    // Размещаем сектора (кольцо 1) и модули (кольца 2+) и подключаем подъездные дороги.
    const connectCellToNearestRingNode = (cellId: string, cellX: number, cellY: number, ring: number): void => {
      addNode({
        id: cellId,
        cityId: city.id,
        x: cellX,
        y: cellY,
        kind: 'cell_entry',
        ring
      })
      let bestNode: RoadNode | null = null
      let bestDist = Infinity
      for (const candidate of nodes.values()) {
        if (candidate.cityId !== city.id || candidate.kind !== 'intersection' || candidate.ring !== ring) continue
        const dist = Math.hypot(candidate.x - cellX, candidate.y - cellY)
        if (dist < bestDist) {
          bestDist = dist
          bestNode = candidate
        }
      }
      if (bestNode) {
        addBidirectionalEdge(`${city.id}:drive:${cellId}`, bestNode.id, cellId, 'driveway', 2)
      }
    }

    city.sectors.forEach((sector, index) => {
      const pos = resolveCellPositionOnRing(city, 1, sector.coord, index, city.sectors.length)
      placements.set(sector.id, {
        id: sector.id,
        cityId: city.id,
        kind: 'sector',
        ring: 1,
        x: pos.x,
        y: pos.y,
        radius: CELL_HEX_RADIUS,
        exitAngles: []
      })
      connectCellToNearestRingNode(sector.id, pos.x, pos.y, 1)
    })

    const modulesByRing = new Map<number, typeof city.modules[number][]>()
    for (const mod of city.modules) {
      const list = modulesByRing.get(mod.ring) ?? []
      list.push(mod)
      modulesByRing.set(mod.ring, list)
    }
    for (const [ring, list] of modulesByRing.entries()) {
      list.forEach((mod, index) => {
        const pos = resolveCellPositionOnRing(city, ring, mod.coord, index, list.length)
        placements.set(mod.id, {
          id: mod.id,
          cityId: city.id,
          kind: 'module',
          ring,
          x: pos.x,
          y: pos.y,
          radius: CELL_HEX_RADIUS * 0.92,
          exitAngles: []
        })
        connectCellToNearestRingNode(mod.id, pos.x, pos.y, ring)
      })
    }
  }

  // Межгородские магистрали через самые внешние (последние) кольца городов.
  for (const link of world.intercityLinks) {
    const fromCity = world.cities.find((c) => c.id === link.fromCityId)
    const toCity = world.cities.find((c) => c.id === link.toCityId)
    if (!fromCity || !toCity) continue

    const fromRing = Math.max(1, link.fromRing)
    const toRing = Math.max(1, link.toRing)
    const fromCandidates = Array.from(nodes.values()).filter(
      (n) => n.cityId === fromCity.id && n.kind === 'intersection' && n.ring === fromRing
    )
    const toCandidates = Array.from(nodes.values()).filter(
      (n) => n.cityId === toCity.id && n.kind === 'intersection' && n.ring === toRing
    )
    let bestPair: { a: RoadNode; b: RoadNode; dist: number } | null = null
    for (const a of fromCandidates) {
      for (const b of toCandidates) {
        const dist = Math.hypot(b.x - a.x, b.y - a.y)
        if (!bestPair || dist < bestPair.dist) {
          bestPair = { a, b, dist }
        }
      }
    }
    if (bestPair) {
      addBidirectionalEdge(`intercity:${link.id}`, bestPair.a.id, bestPair.b.id, 'intercity', 4)
    }
  }

  return {
    placements,
    roadGraph: { nodes, edges, adjacency }
  }
}

/** Кратчайший путь (Dijkstra) между двумя узлами дорожной сети. */
export function findRoadPath(graph: RoadGraph, fromId: string, toId: string): readonly string[] {
  if (fromId === toId) return [fromId]
  if (!graph.nodes.has(fromId) || !graph.nodes.has(toId)) return []

  const dist = new Map<string, number>([[fromId, 0]])
  const prev = new Map<string, string>()
  const unvisited = new Set<string>(graph.nodes.keys())

  while (unvisited.size > 0) {
    let current: string | null = null
    let bestDist = Infinity
    for (const candidate of unvisited) {
      const d = dist.get(candidate) ?? Infinity
      if (d < bestDist) {
        bestDist = d
        current = candidate
      }
    }
    if (current === null || !Number.isFinite(bestDist)) break
    if (current === toId) break
    unvisited.delete(current)

    for (const neighbor of graph.adjacency.get(current) ?? []) {
      if (!unvisited.has(neighbor.toId)) continue
      const alt = bestDist + neighbor.edge.length
      if (alt < (dist.get(neighbor.toId) ?? Infinity)) {
        dist.set(neighbor.toId, alt)
        prev.set(neighbor.toId, current)
      }
    }
  }

  if (!prev.has(toId)) return []
  const path: string[] = [toId]
  let cursor = toId
  while (cursor !== fromId) {
    const parent = prev.get(cursor)
    if (!parent) return []
    path.push(parent)
    cursor = parent
  }
  path.reverse()
  return path
}
