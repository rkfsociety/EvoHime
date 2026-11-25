import type {
  HoneycombCity,
  HoneycombHexCoord,
  HoneycombWorldState
} from '@shared/hive-city-map'

export const CITADEL_HEX_RADIUS = 48
export const CELL_HEX_RADIUS = 30
export const RING_BASE_RADIUS = 84
export const RING_STEP = 112

/** Углы нормалей к 6 граням flat-top гексагона (строго перпендикулярно граням, верхняя/нижняя грани горизонтальны). */
export const HEX_FACE_NORMAL_ANGLES: readonly number[] = [
  Math.PI / 6,
  Math.PI / 2,
  (5 * Math.PI) / 6,
  (7 * Math.PI) / 6,
  (3 * Math.PI) / 2,
  (11 * Math.PI) / 6
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
  readonly kind: 'citadel' | 'project_hub' | 'sector' | 'agent_home' | 'module'
  readonly ring: number
  readonly x: number
  readonly y: number
  readonly radius: number
  readonly exitAngles: readonly number[]
  readonly sectorId?: string | undefined
  readonly agentSlot?: number | undefined
}

export interface SectorArcPlacement {
  readonly id: string
  readonly cityId: string
  readonly role: import('@shared/hive-city-map').HoneycombAgentRole
  readonly label: string
  readonly centerX: number
  readonly centerY: number
  readonly innerRadius: number
  readonly outerRadius: number
  readonly startAngle: number
  readonly endAngle: number
}

/** Вычисляет радиус кольцевой магистрали для заданного кольца `ring >= 1`. */
export function getRingRoadRadius(ring: number): number {
  return RING_BASE_RADIUS + ring * RING_STEP - RING_STEP * 0.42
}

/** Вычисляет радиус центров сот на заданном кольце `ring >= 1`. */
export function getRingCellRadius(ring: number): number {
  return RING_BASE_RADIUS + ring * RING_STEP
}

/** Вычисляет радиус внешней гексагональной соты-забора города (единый источник истины). */
export function getCityOuterHexRadius(maxRing: number): number {
  const contentRadius =
    maxRing <= 0 ? CITADEL_HEX_RADIUS + 28 : getRingCellRadius(maxRing) + CELL_HEX_RADIUS + 20
  return contentRadius / Math.cos(Math.PI / 6)
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

/** Возвращает 6 вершин flat-top гексагона (верхняя и нижняя грани горизонтальны). */
export function getFlatHexVertices(cx: number, cy: number, radius: number): readonly Point2D[] {
  const vertices: Point2D[] = []
  for (let i = 0; i < 6; i += 1) {
    const angle = (i * Math.PI) / 3
    vertices.push({
      x: cx + Math.cos(angle) * radius,
      y: cy + Math.sin(angle) * radius
    })
  }
  return vertices
}

/** @deprecated Используй `getFlatHexVertices`. */
export const getPointyHexVertices = getFlatHexVertices

function normalizeAngle(angle: number): number {
  const twoPi = Math.PI * 2
  return ((angle % twoPi) + twoPi) % twoPi
}

function shortestAngleDiff(a: number, b: number): number {
  const diff = Math.abs(normalizeAngle(a) - normalizeAngle(b))
  return diff > Math.PI ? Math.PI * 2 - diff : diff
}

/**
 * Отводит угол соты от заблокированных осей (радиальных лучей и межгородских трасс),
 * чтобы дороги никогда не проходили сквозь соты агентов и модули.
 */
function nudgeAngleAwayFromRays(
  angle: number,
  blockedAngles: readonly number[],
  minGapRad: number
): number {
  let candidate = normalizeAngle(angle)
  for (let pass = 0; pass < 3; pass += 1) {
    for (const ray of blockedAngles) {
      const diff = normalizeAngle(candidate - ray)
      const signed = diff > Math.PI ? diff - Math.PI * 2 : diff
      if (Math.abs(signed) < minGapRad) {
        candidate = normalizeAngle(ray + (signed >= 0 ? minGapRad : -minGapRad))
      }
    }
  }
  return candidate
}

/**
 * Переводит гекс-координаты `(q, r)` и индекс на кольце в плавную 2D-позицию
 * вокруг центра города, обходя углы радиальных дорог.
 */
export function resolveCellPositionOnRing(
  city: HoneycombCity,
  ring: number,
  coord: HoneycombHexCoord,
  indexInRing: number,
  totalInRing: number,
  blockedAngles: readonly number[] = []
): Point2D {
  const radius = getRingCellRadius(ring)
  const count = Math.max(1, totalInRing)
  const baseAngle = ((indexInRing + 0.5) / count) * Math.PI * 2
  const axialBias = ((coord.q * 7 + coord.r * 13) % 11) * 0.004
  const minGapRad = Math.min(0.28, (CELL_HEX_RADIUS + 16) / Math.max(1, radius))
  const angle =
    blockedAngles.length > 0
      ? nudgeAngleAwayFromRays(baseAngle + axialBias, blockedAngles, minGapRad)
      : baseAngle + axialBias
  return {
    x: city.worldCenterX + Math.cos(angle) * radius,
    y: city.worldCenterY + Math.sin(angle) * radius
  }
}

/** Строит 2D-размещение всех сот города, кольцевых секторов и граф дорожной сети с ПДД-узлами. */
export function buildWorldRoadGraphAndPlacements(world: HoneycombWorldState): {
  readonly placements: ReadonlyMap<string, HexCellPlacement>
  readonly sectorArcs: readonly SectorArcPlacement[]
  readonly roadGraph: RoadGraph
} {
  const placements = new Map<string, HexCellPlacement>()
  const sectorArcs: SectorArcPlacement[] = []
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

    // Круглые кольцевые магистрали внутри города (24 сегмента по окружности для плавного круга).
    const ringSegments: { ring: number; aId: string; bId: string }[] = []
    const RING_STEPS = 24
    for (let ring = 1; ring <= maxRing; ring += 1) {
      const roadRadius = getRingRoadRadius(ring)
      const ringAngles = new Map<number, number | undefined>()
      for (let step = 0; step < RING_STEPS; step += 1) {
        const angle = (step / RING_STEPS) * Math.PI * 2
        ringAngles.set(angle, undefined)
      }
      // Гарантируем точные узлы на нормалях активных радиальных лучей Цитадели.
      for (const faceIdx of exitFaces) {
        const rayAngle = HEX_FACE_NORMAL_ANGLES[faceIdx] ?? 0
        // Удаляем ближайший шаг сетки, если он слишком близко (< 0.08 рад), чтобы не плодить микросегменты.
        for (const existingAngle of Array.from(ringAngles.keys())) {
          if (Math.abs(existingAngle - rayAngle) < 0.08) {
            ringAngles.delete(existingAngle)
          }
        }
        ringAngles.set(rayAngle, faceIdx)
      }

      const sortedEntries = Array.from(ringAngles.entries()).sort((a, b) => a[0] - b[0])
      const ringNodeIds: string[] = []

      sortedEntries.forEach(([angle, rayIdx], step) => {
        const nodeId = `${city.id}:ring:${ring}:n:${step}`
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
          ringSegments.push({ ring, aId: curr, bId: next })
        }
      }
    }

    // Размещаем сектора и модули и врезаем ровный перпендикулярный подъезд (второстепенная дорога)
    // от ближайшего сегмента кольца прямо к соте.
    const segmentSplits = new Map<number, { t: number; nodeId: string }[]>()

    const connectCellToRingSegment = (cellId: string, cellX: number, cellY: number, ring: number): void => {
      addNode({
        id: cellId,
        cityId: city.id,
        x: cellX,
        y: cellY,
        kind: 'cell_entry',
        ring
      })

      let bestSegIdx = -1
      let bestT = 0.5
      let bestProjX = cellX
      let bestProjY = cellY
      let bestDist = Infinity

      ringSegments.forEach((seg, segIdx) => {
        if (seg.ring !== ring) return
        const a = nodes.get(seg.aId)
        const b = nodes.get(seg.bId)
        if (!a || !b) return
        const dx = b.x - a.x
        const dy = b.y - a.y
        const lenSq = dx * dx + dy * dy
        if (lenSq < 1e-3) return
        const t = Math.max(0.08, Math.min(0.92, ((cellX - a.x) * dx + (cellY - a.y) * dy) / lenSq))
        const px = a.x + dx * t
        const py = a.y + dy * t
        const dist = Math.hypot(cellX - px, cellY - py)
        if (dist < bestDist) {
          bestDist = dist
          bestSegIdx = segIdx
          bestT = t
          bestProjX = px
          bestProjY = py
        }
      })

      if (bestSegIdx >= 0) {
        const tapId = `${city.id}:tap:${cellId}`
        addNode({
          id: tapId,
          cityId: city.id,
          x: bestProjX,
          y: bestProjY,
          kind: 'intersection',
          ring
        })
        const list = segmentSplits.get(bestSegIdx) ?? []
        list.push({ t: bestT, nodeId: tapId })
        segmentSplits.set(bestSegIdx, list)
        addBidirectionalEdge(`${city.id}:drive:${cellId}`, tapId, cellId, 'driveway', 2)
      }
    }

    // Размещаем кольцевые сектора (1-е кольцо) строго в дугах между радиальными дорогами,
    // чтобы ни одна радиальная дорога никогда не пересекала сектор.
    const sectorCount = city.sectors.length
    if (sectorCount > 0) {
      const ring1CellR = getRingCellRadius(1)
      const ring1RoadR = getRingRoadRadius(1)
      const arcInnerR = ring1RoadR + 14
      const arcOuterR = ring1CellR + 40

      const sortedRays = [...exitAngles].map(normalizeAngle).sort((a, b) => a - b)
      const pockets: { start: number; span: number }[] = []
      if (sortedRays.length === 0) {
        pockets.push({ start: 0, span: Math.PI * 2 })
      } else {
        for (let i = 0; i < sortedRays.length; i += 1) {
          const a = sortedRays[i]!
          const b = sortedRays[(i + 1) % sortedRays.length]!
          const span = b > a ? b - a : Math.PI * 2 - a + b
          pockets.push({ start: a, span })
        }
      }

      // Распределяем `sectorCount` секторов пропорционально ширине дуг между дорогами.
      const countsPerPocket = new Array<number>(pockets.length).fill(0)
      for (let s = 0; s < sectorCount; s += 1) {
        let bestIdx = 0
        let bestScore = -1
        for (let p = 0; p < pockets.length; p += 1) {
          const score = pockets[p]!.span / (countsPerPocket[p]! + 1)
          if (score > bestScore) {
            bestScore = score
            bestIdx = p
          }
        }
        countsPerPocket[bestIdx] = countsPerPocket[bestIdx]! + 1
      }

      const rayClearanceRad = 22 / Math.max(1, ring1CellR)
      const betweenSectorsRad = 0.035
      let sectorIdx = 0

      pockets.forEach((pocket, pIdx) => {
        const count = countsPerPocket[pIdx]!
        if (count <= 0) return
        const usableStart = pocket.start + rayClearanceRad
        const usableSpan = Math.max(0.1, pocket.span - rayClearanceRad * 2)
        const sliceSpan = usableSpan / count

        for (let localIdx = 0; localIdx < count; localIdx += 1) {
          const sector = city.sectors[sectorIdx]
          if (!sector) break
          sectorIdx += 1

          const padLeft = localIdx === 0 ? 0 : betweenSectorsRad / 2
          const padRight = localIdx === count - 1 ? 0 : betweenSectorsRad / 2
          const startAngle = usableStart + localIdx * sliceSpan + padLeft
          const endAngle = usableStart + (localIdx + 1) * sliceSpan - padRight
          const midAngle = (startAngle + endAngle) / 2

          sectorArcs.push({
            id: sector.id,
            cityId: city.id,
            role: sector.role,
            label: sector.label,
            centerX: city.worldCenterX,
            centerY: city.worldCenterY,
            innerRadius: arcInnerR,
            outerRadius: arcOuterR,
            startAngle,
            endAngle
          })

          const sectorEntryX = city.worldCenterX + Math.cos(midAngle) * ring1CellR
          const sectorEntryY = city.worldCenterY + Math.sin(midAngle) * ring1CellR
          placements.set(sector.id, {
            id: sector.id,
            cityId: city.id,
            kind: 'sector',
            ring: 1,
            x: sectorEntryX,
            y: sectorEntryY,
            radius: 18,
            exitAngles: []
          })

          // 3 соты-дома агентов внутри дуги сектора (симметрично, без пересечения дорог).
          const subSpan = (endAngle - startAngle) / 3
          for (let slot = 0; slot < 3; slot += 1) {
            const slotAngle = startAngle + (slot + 0.5) * subSpan
            const ax = city.worldCenterX + Math.cos(slotAngle) * ring1CellR
            const ay = city.worldCenterY + Math.sin(slotAngle) * ring1CellR
            const agent = sector.agents[slot]
            const cellId = agent ? `${sector.id}:slot:${slot}` : `${sector.id}:empty:${slot}`
            placements.set(cellId, {
              id: cellId,
              cityId: city.id,
              kind: 'agent_home',
              ring: 1,
              x: ax,
              y: ay,
              radius: 19,
              exitAngles: [],
              sectorId: sector.id,
              agentSlot: slot
            })
            if (slot === 1) {
              connectCellToRingSegment(sector.id, ax, ay, 1)
            } else {
              connectCellToRingSegment(cellId, ax, ay, 1)
            }
          }
        }
      })
    }

    const intercityDirections = world.intercityLinks
      .filter((l) => l.fromCityId === city.id || l.toCityId === city.id)
      .map((l) => {
        const otherId = l.fromCityId === city.id ? l.toCityId : l.fromCityId
        const other = world.cities.find((c) => c.id === otherId)
        return other
          ? Math.atan2(other.worldCenterY - city.worldCenterY, other.worldCenterX - city.worldCenterX)
          : null
      })
      .filter((a): a is number => a !== null)
    const moduleBlockedAngles = [...exitAngles, ...intercityDirections]

    const modulesByRing = new Map<number, typeof city.modules[number][]>()
    for (const mod of city.modules) {
      const list = modulesByRing.get(mod.ring) ?? []
      list.push(mod)
      modulesByRing.set(mod.ring, list)
    }
    for (const [ring, list] of modulesByRing.entries()) {
      list.forEach((mod, index) => {
        const pos = resolveCellPositionOnRing(city, ring, mod.coord, index, list.length, moduleBlockedAngles)
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
        connectCellToRingSegment(mod.id, pos.x, pos.y, ring)
      })
    }

    // Строим кольцевые рёбра с учётом врезанных Т-образных узлов подъездов.
    ringSegments.forEach((seg, segIdx) => {
      const splits = segmentSplits.get(segIdx)
      if (!splits || splits.length === 0) {
        addBidirectionalEdge(`${city.id}:ring:${seg.ring}:seg:${segIdx}`, seg.aId, seg.bId, 'ring', 4)
        return
      }
      const sorted = [...splits].sort((u, v) => u.t - v.t)
      let prevId = seg.aId
      sorted.forEach((split, sIdx) => {
        addBidirectionalEdge(`${city.id}:ring:${seg.ring}:seg:${segIdx}:${sIdx}`, prevId, split.nodeId, 'ring', 4)
        prevId = split.nodeId
      })
      addBidirectionalEdge(`${city.id}:ring:${seg.ring}:seg:${segIdx}:end`, prevId, seg.bId, 'ring', 4)
    })
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
    sectorArcs,
    roadGraph: { nodes, edges, adjacency }
  }
}

export { shortestAngleDiff }

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
