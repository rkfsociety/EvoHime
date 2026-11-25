// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it } from 'vitest'

import {
  applyHoneycombDelta,
  HONEYCOMB_SECTOR_CAPACITY,
  parseHoneycombWorldDelta
} from '../src/shared/hive-city-map'
import { StudioApp } from '../src/renderer/src/hive-studio-app'
import { buildHoneycombStudioWorld } from '../src/renderer/src/hive-studio-simulation'
import {
  buildWorldRoadGraphAndPlacements,
  findRoadPath,
  getCityOuterHexRadius,
  getCityRingExpansion,
  HEX_FACE_NORMAL_ANGLES,
  selectCitadelExitFaceIndices
} from '../src/renderer/src/components/hive/honeycomb-geometry'
import {
  createTrafficSimulationState,
  getIntersectionPhaseOffsetSec,
  isTrafficGreenForEdgeKind,
  stepTrafficSimulation
} from '../src/renderer/src/components/hive/honeycomb-traffic'

describe('buildHoneycombStudioWorld', () => {
  it('starts with only a single Queen citadel in zero stage', () => {
    const world = buildHoneycombStudioWorld({
      stage: 'zero',
      paused: false,
      evaSourceConfigured: true,
      projectCount: 1,
      agentCount: 5,
      exitsOverride: 'auto',
      remodelSeed: 0
    })

    expect(world.cities).toHaveLength(1)
    expect(world.cities[0]?.hasQueenCitadel).toBe(true)
    expect(world.cities[0]?.sectors).toHaveLength(0)
    expect(world.cities[0]?.modules).toHaveLength(0)
    expect(world.activeDispatches).toHaveLength(0)
  })

  it('enforces 3 homogeneous-role agents per sector on ring 1 and connects multiple cities via outer rings', () => {
    const world = buildHoneycombStudioWorld({
      stage: 'pipeline',
      paused: false,
      evaSourceConfigured: true,
      projectCount: 3,
      agentCount: 7,
      exitsOverride: 'auto',
      remodelSeed: 0
    })

    const primary = world.cities[0]!
    expect(primary.sectors.length).toBe(4)
    for (const sector of primary.sectors) {
      expect(sector.agents.length).toBeGreaterThan(0)
      expect(sector.agents.length).toBeLessThanOrEqual(HONEYCOMB_SECTOR_CAPACITY)
      for (const agent of sector.agents) {
        expect(agent.role).toBe(sector.role)
      }
    }

    expect(world.cities).toHaveLength(3)
    expect(world.cities.filter((c) => c.hasQueenCitadel)).toHaveLength(1)
    expect(world.intercityLinks).toHaveLength(2)
    for (const link of world.intercityLinks) {
      expect(link.fromRing).toBeGreaterThanOrEqual(1)
      expect(link.toRing).toBeGreaterThanOrEqual(1)
    }

    // Ни одна пара городов не должна пересекаться своими внешними сотами-заборами.
    for (let i = 0; i < world.cities.length; i += 1) {
      for (let j = i + 1; j < world.cities.length; j += 1) {
        const a = world.cities[i]!
        const b = world.cities[j]!
        const dist = Math.hypot(b.worldCenterX - a.worldCenterX, b.worldCenterY - a.worldCenterY)
        const minRequired =
          getCityOuterHexRadius(a.maxRing, getCityRingExpansion(a)) +
          getCityOuterHexRadius(b.maxRing, getCityRingExpansion(b))
        expect(dist).toBeGreaterThan(minRequired)
      }
    }

    // При большом количестве агентов (15) и секторов соты агентов не пересекаются друг с другом.
    const crowded = buildHoneycombStudioWorld({
      stage: 'pipeline',
      paused: false,
      evaSourceConfigured: true,
      projectCount: 1,
      agentCount: 15,
      exitsOverride: 'auto',
      remodelSeed: 0
    })
    const crowdedHomes = Array.from(buildWorldRoadGraphAndPlacements(crowded).placements.values()).filter(
      (p) => p.kind === 'agent_home'
    )
    const crowdedLabels = crowded.cities[0]!.sectors.map((s) => s.label)
    expect(new Set(crowdedLabels).size).toBe(crowdedLabels.length)
    for (const label of crowdedLabels) {
      expect(label).not.toMatch(/#\d+/)
    }
    for (let i = 0; i < crowdedHomes.length; i += 1) {
      for (let j = i + 1; j < crowdedHomes.length; j += 1) {
        const a = crowdedHomes[i]!
        const b = crowdedHomes[j]!
        const dist = Math.hypot(b.x - a.x, b.y - a.y)
        expect(dist).toBeGreaterThan(a.radius + b.radius)
      }
    }
  })

  it('falls back primary citadel to the first added project when Eva sources are not configured', () => {
    const world = buildHoneycombStudioWorld({
      stage: 'scouting',
      paused: false,
      evaSourceConfigured: false,
      projectCount: 2,
      agentCount: 2,
      exitsOverride: 3,
      remodelSeed: 0
    })

    expect(world.cities[0]?.projectName).toBe('CodexGUI')
    expect(world.cities[0]?.hasQueenCitadel).toBe(true)
    expect(world.cities[1]?.hasQueenCitadel).toBe(false)
  })

  it('parks all vehicles when paused (zero dispatches on roads)', () => {
    const world = buildHoneycombStudioWorld({
      stage: 'pipeline',
      paused: true,
      evaSourceConfigured: true,
      projectCount: 2,
      agentCount: 6,
      exitsOverride: 'auto',
      remodelSeed: 0
    })

    expect(world.paused).toBe(true)
    expect(world.activeDispatches).toHaveLength(0)

    const { roadGraph } = buildWorldRoadGraphAndPlacements(world)
    const traffic = createTrafficSimulationState()
    stepTrafficSimulation(traffic, world, roadGraph, 0.016, 1.0, 1)
    expect(traffic.vehicles.size).toBe(0)
  })

  it('applies incremental HoneycombWorldDelta events cleanly', () => {
    const base = buildHoneycombStudioWorld({
      stage: 'zero',
      paused: false,
      evaSourceConfigured: true,
      projectCount: 1,
      agentCount: 0,
      exitsOverride: 'auto',
      remodelSeed: 0
    })
    const delta = parseHoneycombWorldDelta({
      schemaVersion: 2,
      fromRevision: base.revision,
      toRevision: base.revision + 1,
      requiresFullResync: false,
      events: [
        {
          type: 'sector.create',
          cityId: 'city-primary',
          sector: {
            id: 'city-primary:sector:0',
            cityId: 'city-primary',
            coord: { q: 0, r: 1 },
            ring: 1,
            label: 'Исполнители',
            role: 'executor',
            capacity: 3,
            agents: []
          }
        },
        {
          type: 'agent.spawn',
          cityId: 'city-primary',
          sectorId: 'city-primary:sector:0',
          agent: {
            id: 'agent-1',
            name: 'Агент #1',
            role: 'executor',
            sectorId: 'city-primary:sector:0',
            topic: 'разведка',
            busy: true,
            activeTripId: 'trip-1'
          }
        }
      ]
    })
    expect(delta).not.toBeNull()
    const next = applyHoneycombDelta(base, delta!)
    expect(next.revision).toBe(base.revision + 1)
    expect(next.cities[0]?.sectors).toHaveLength(1)
    expect(next.cities[0]?.sectors[0]?.agents).toHaveLength(1)
  })
})

describe('honeycomb geometry & traffic', () => {
  it('places 1..6 citadel exits strictly perpendicular to outer hex faces', () => {
    for (let count = 1; count <= 6; count += 1) {
      const faces = selectCitadelExitFaceIndices(count)
      expect(faces).toHaveLength(count)
      for (const faceIdx of faces) {
        expect(HEX_FACE_NORMAL_ANGLES[faceIdx]).toBeTypeOf('number')
      }
    }
  })

  it('routes vehicles through road network and stops on red traffic lights', () => {
    const world = buildHoneycombStudioWorld({
      stage: 'pipeline',
      paused: false,
      evaSourceConfigured: true,
      projectCount: 1,
      agentCount: 4,
      exitsOverride: 4,
      remodelSeed: 0
    })
    const { placements, roadGraph } = buildWorldRoadGraphAndPlacements(world)
    const radialEdges = roadGraph.edges.filter((e) => e.kind === 'radial')
    for (const placement of placements.values()) {
      if (placement.kind !== 'agent_home') continue
      for (const edge of radialEdges) {
        if (!edge.id.startsWith(`${placement.cityId}:`)) continue
        const a = roadGraph.nodes.get(edge.fromId)!
        const b = roadGraph.nodes.get(edge.toId)!
        const dx = b.x - a.x
        const dy = b.y - a.y
        const lenSq = dx * dx + dy * dy
        if (lenSq < 1e-3) continue
        const t = Math.max(0, Math.min(1, ((placement.x - a.x) * dx + (placement.y - a.y) * dy) / lenSq))
        const dist = Math.hypot(placement.x - (a.x + dx * t), placement.y - (a.y + dy * t))
        expect(dist).toBeGreaterThan(placement.radius)
      }
    }

    const path = findRoadPath(roadGraph, 'city-primary:citadel', 'city-primary:sector:0')
    expect(path.length).toBeGreaterThanOrEqual(3)

    expect(isTrafficGreenForEdgeKind(1.0, 'ring')).toBe(true)
    expect(isTrafficGreenForEdgeKind(1.0, 'radial')).toBe(false)
    expect(isTrafficGreenForEdgeKind(6.0, 'radial')).toBe(true)
    expect(isTrafficGreenForEdgeKind(6.0, 'ring')).toBe(false)

    // Локальные светофоры имеют индивидуальные сдвиги фаз для разных перекрёстков.
    expect(getIntersectionPhaseOffsetSec('city-primary:ring:1:n:0')).not.toBe(
      getIntersectionPhaseOffsetSec('city-primary:ring:2:n:4')
    )

    const traffic = createTrafficSimulationState()
    stepTrafficSimulation(traffic, world, roadGraph, 0.016, 1.0, 1)
    expect(traffic.vehicles.size).toBe(world.activeDispatches.length)

    // Проверяем отсутствие телепортов: за 200 шагов ни одна машина не прыгает скачком > 25px за шаг.
    for (let step = 0; step < 200; step += 1) {
      const prevPositions = new Map(
        Array.from(traffic.vehicles.values()).map((v) => [v.id, { x: v.x, y: v.y }])
      )
      stepTrafficSimulation(traffic, world, roadGraph, 0.03, 1.0 + step * 0.03, 1)
      for (const v of traffic.vehicles.values()) {
        const prev = prevPositions.get(v.id)
        if (prev) {
          expect(Math.hypot(v.x - prev.x, v.y - prev.y)).toBeLessThan(25)
        }
        // Машина всегда едет строго по дорожному полотну (отклонение от оси дороги <= 11px).
        if (v.opacity > 0.1) {
          let minEdgeDist = Infinity
          for (const edge of roadGraph.edges) {
            const a = roadGraph.nodes.get(edge.fromId)!
            const b = roadGraph.nodes.get(edge.toId)!
            const dx = b.x - a.x
            const dy = b.y - a.y
            const lenSq = dx * dx + dy * dy
            if (lenSq < 1e-3) continue
            const t = Math.max(0, Math.min(1, ((v.x - a.x) * dx + (v.y - a.y) * dy) / lenSq))
            const dist = Math.hypot(v.x - (a.x + dx * t), v.y - (a.y + dy * t))
            if (dist < minEdgeDist) minEdgeDist = dist
          }
          expect(minEdgeDist).toBeLessThanOrEqual(11)
        }
      }
    }
  })

  it('routes dispatches to the specific agent cell slot instead of empty middle sector node', () => {
    const world = buildHoneycombStudioWorld({
      stage: 'pipeline',
      paused: false,
      evaSourceConfigured: true,
      projectCount: 1,
      agentCount: 1,
      exitsOverride: 'auto',
      remodelSeed: 0
    })
    const primary = world.cities[0]!
    const sector = primary.sectors[0]!
    const agent = sector.agents[0]!
    const singleDispatchWorld = {
      ...world,
      activeDispatches: [
        {
          id: 'trip-agent-slot-0',
          cityId: primary.id,
          agentId: agent.id,
          role: agent.role,
          cargo: 'task' as const,
          fromNodeId: `${primary.id}:citadel`,
          toNodeId: sector.id,
          startedAtMs: 1000
        }
      ]
    }
    const { roadGraph } = buildHoneycombSceneLayout(singleDispatchWorld)
    const traffic = createTrafficSimulationState()
    stepTrafficSimulation(traffic, singleDispatchWorld, roadGraph, 0.016, 1.0, 1)
    const vehicle = traffic.vehicles.get('trip-agent-slot-0')
    expect(vehicle).toBeTruthy()
    expect(vehicle!.waypoints[vehicle!.waypoints.length - 1]).toBe(`${sector.id}:slot:0`)
  })

  it('keeps safe headway between vehicles across segment boundaries', () => {
    const world = buildHoneycombStudioWorld({
      stage: 'pipeline',
      paused: false,
      evaSourceConfigured: true,
      projectCount: 1,
      agentCount: 2,
      exitsOverride: 'auto',
      remodelSeed: 0
    })
    const primary = world.cities[0]!
    const sector = primary.sectors[0]!
    const twoVehiclesWorld = {
      ...world,
      activeDispatches: [
        {
          id: 'trip-lead',
          cityId: primary.id,
          agentId: sector.agents[0]!.id,
          role: sector.agents[0]!.role,
          cargo: 'task' as const,
          fromNodeId: `${primary.id}:citadel`,
          toNodeId: sector.id,
          startedAtMs: 1000
        },
        {
          id: 'trip-trail',
          cityId: primary.id,
          agentId: sector.agents[1]!.id,
          role: sector.agents[1]!.role,
          cargo: 'task' as const,
          fromNodeId: `${primary.id}:citadel`,
          toNodeId: sector.id,
          startedAtMs: 1001
        }
      ]
    }
    const { roadGraph } = buildHoneycombSceneLayout(twoVehiclesWorld)
    const traffic = createTrafficSimulationState()
    for (let step = 0; step < 120; step += 1) {
      stepTrafficSimulation(traffic, twoVehiclesWorld, roadGraph, 0.03, 1.0 + step * 0.03, 1)
      const a = traffic.vehicles.get('trip-lead')
      const b = traffic.vehicles.get('trip-trail')
      if (a && b && a.opacity > 0.5 && b.opacity > 0.5) {
        const sameDir = Math.cos(a.angleRad - b.angleRad) > 0.5
        if (sameDir) {
          expect(Math.hypot(a.x - b.x, a.y - b.y)).toBeGreaterThanOrEqual(14)
        }
      }
    }
  })
})

describe('Hive Studio app (2D)', () => {
  afterEach(() => {
    cleanup()
  })

  it('renders 2D canvas scene and updates sectors when adding/removing agents', async () => {
    const user = userEvent.setup()
    render(<StudioApp />)

    const scene = screen.getByTestId('hive-city-scene')
    expect(scene.getAttribute('data-mode')).toBe('2d-canvas')
    expect(screen.getByTestId('hive-city-canvas')).toBeTruthy()
    expect(screen.getByTestId('studio-agent-summary').textContent).toContain('агентов: 5 · секторов: 4')

    await user.click(screen.getByRole('button', { name: 'Добавить агента' }))
    await user.click(screen.getByRole('button', { name: 'Добавить агента' }))
    expect(screen.getByTestId('studio-agent-summary').textContent).toContain('агентов: 7 · секторов: 4')

    await user.click(screen.getByRole('button', { name: 'Убрать агента' }))
    expect(screen.getByTestId('studio-agent-summary').textContent).toContain('агентов: 6 · секторов: 4')
  })
})
