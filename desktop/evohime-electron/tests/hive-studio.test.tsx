// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it } from 'vitest'

import { parseHiveCityMapSnapshot } from '../src/shared/hive-city-map'
import { StudioApp } from '../src/renderer/src/hive-studio-app'
import {
  buildHiveStudioSimulation,
  HIVE_STUDIO_DEPARTMENTS
} from '../src/renderer/src/hive-studio-simulation'

function activeHouseCount(): number {
  return Array.from(
    document.querySelectorAll('[data-testid^="hive-archipelago-house-"]')
  ).filter((house) => house.getAttribute('data-active') === 'true').length
}

describe('buildHiveStudioSimulation', () => {
  it('round-trips through the strict daemon snapshot parser without loss', () => {
    for (const satelliteCount of [0, 1, 2, 3]) {
      const snapshot = buildHiveStudioSimulation({
        generatedAtMs: 1_730_000_000_000,
        satelliteCount
      })
      const parsed = parseHiveCityMapSnapshot(snapshot)
      expect(parsed).not.toBeNull()
      expect(parsed).toEqual(snapshot)
      expect(parsed?.capital.role).toBe('capital')
      expect(parsed?.satellites).toHaveLength(satelliteCount)
    }
  })

  it('is deterministic for the same seed and options', () => {
    const options = { generatedAtMs: 1_730_000_000_000, satelliteCount: 2, seed: 7 }
    expect(buildHiveStudioSimulation(options)).toEqual(buildHiveStudioSimulation(options))
  })

  it('keeps building and edge identifiers unique and resolvable', () => {
    const snapshot = buildHiveStudioSimulation({ satelliteCount: 2 })
    for (const island of [snapshot.capital, ...snapshot.satellites]) {
      const ids = new Set(island.buildings.map((building) => building.id))
      expect(ids.size).toBe(island.buildings.length)
      const districtIds = new Set(island.districts.map((district) => district.id))
      for (const building of island.buildings) {
        expect(districtIds.has(building.district_id)).toBe(true)
        expect(building.height_weight_bp).toBeGreaterThanOrEqual(150)
        expect(building.height_weight_bp).toBeLessThanOrEqual(1000)
      }
      for (const edge of island.edges) {
        expect(ids.has(edge.from_id)).toBe(true)
        expect(ids.has(edge.to_id)).toBe(true)
      }
    }
  })

  it('simulates every dashboard department with positive capacity', () => {
    expect(HIVE_STUDIO_DEPARTMENTS.length).toBeGreaterThanOrEqual(8)
    for (const department of HIVE_STUDIO_DEPARTMENTS) {
      expect(department.maxCapacity).toBeGreaterThan(0)
      expect(department.activeCount).toBeGreaterThanOrEqual(0)
      expect(department.activeCount).toBeLessThanOrEqual(department.maxCapacity)
    }
  })
})

describe('Hive Studio app', () => {
  afterEach(() => {
    cleanup()
  })

  it('renders the live scene in simulation mode with archipelago data', () => {
    render(<StudioApp />)
    const scene = screen.getByTestId('hive-city-scene')
    // jsdom без WebGL → живой архипелаг рисуется SVG-рендерером (2D).
    expect(scene.getAttribute('data-mode')).toBe('2d')
    const houses = document.querySelectorAll('[data-testid^="hive-archipelago-house-"]')
    expect(houses.length).toBeGreaterThan(0)
  })

  it('steps department workload and lights the corresponding houses', async () => {
    const user = userEvent.setup()
    render(<StudioApp />)
    const planningBefore = Array.from(
      document.querySelectorAll('[data-testid^="hive-archipelago-house-planning-"]')
    ).filter((house) => house.getAttribute('data-active') === 'true').length
    const totalBefore = activeHouseCount()

    await user.click(screen.getByRole('button', { name: 'Открыть пульт' }))
    await user.click(screen.getByRole('button', { name: 'Меньше: Планирование' }))

    const planningAfterDown = Array.from(
      document.querySelectorAll('[data-testid^="hive-archipelago-house-planning-"]')
    ).filter((house) => house.getAttribute('data-active') === 'true').length
    expect(planningAfterDown).toBe(planningBefore - 1)
    expect(activeHouseCount()).toBe(totalBefore - 1)

    await user.click(screen.getByRole('button', { name: 'Больше: Планирование' }))

    const planningAfterUp = Array.from(
      document.querySelectorAll('[data-testid^="hive-archipelago-house-planning-"]')
    ).filter((house) => house.getAttribute('data-active') === 'true').length
    expect(planningAfterUp).toBe(planningBefore)
  })
})
