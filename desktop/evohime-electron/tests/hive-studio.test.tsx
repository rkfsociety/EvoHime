// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it } from 'vitest'

import { HONEYCOMB_SECTOR_CAPACITY } from '../src/shared/hive-city-map'
import { StudioApp } from '../src/renderer/src/hive-studio-app'
import { buildHoneycombStudioWorld } from '../src/renderer/src/hive-studio-simulation'
import {
  buildWorldRoadGraphAndPlacements,
  findRoadPath,
  HEX_FACE_NORMAL_ANGLES,
  selectCitadelExitFaceIndices
} from '../src/renderer/src/components/hive/honeycomb-geometry'
import {
  createTrafficSimulationState,
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

  it('enforces 3 agents per sector on ring 1 and connects multiple cities via outer rings', () => {
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
    expect(primary.sectors).toHaveLength(3)
    expect(primary.sectors[0]?.agents).toHaveLength(HONEYCOMB_SECTOR_CAPACITY)
    expect(primary.sectors[1]?.agents).toHaveLength(HONEYCOMB_SECTOR_CAPACITY)
    expect(primary.sectors[2]?.agents).toHaveLength(1)

    expect(world.cities).toHaveLength(3)
    expect(world.cities.filter((c) => c.hasQueenCitadel)).toHaveLength(1)
    expect(world.intercityLinks).toHaveLength(2)
    for (const link of world.intercityLinks) {
      expect(link.fromRing).toBeGreaterThanOrEqual(1)
      expect(link.toRing).toBeGreaterThanOrEqual(1)
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
    const { roadGraph } = buildWorldRoadGraphAndPlacements(world)
    const path = findRoadPath(roadGraph, 'city-primary:citadel', 'city-primary:sector:0')
    expect(path.length).toBeGreaterThanOrEqual(3)

    expect(isTrafficGreenForEdgeKind(1.0, 'ring')).toBe(true)
    expect(isTrafficGreenForEdgeKind(1.0, 'radial')).toBe(false)
    expect(isTrafficGreenForEdgeKind(6.0, 'radial')).toBe(true)
    expect(isTrafficGreenForEdgeKind(6.0, 'ring')).toBe(false)

    const traffic = createTrafficSimulationState()
    stepTrafficSimulation(traffic, world, roadGraph, 0.016, 1.0, 1)
    expect(traffic.vehicles.size).toBe(world.activeDispatches.length)
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
    expect(screen.getByTestId('studio-agent-summary').textContent).toContain('агентов: 5 · секторов: 2')

    await user.click(screen.getByRole('button', { name: 'Добавить агента' }))
    await user.click(screen.getByRole('button', { name: 'Добавить агента' }))
    expect(screen.getByTestId('studio-agent-summary').textContent).toContain('агентов: 7 · секторов: 3')

    await user.click(screen.getByRole('button', { name: 'Убрать агента' }))
    expect(screen.getByTestId('studio-agent-summary').textContent).toContain('агентов: 6 · секторов: 2')
  })
})
