// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('../src/renderer/src/components/3d/HiveArchipelago3D', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../src/renderer/src/components/3d/HiveArchipelago3D')>()
  return {
    ...actual,
    HiveArchipelago3D: function MockHiveArchipelago3D({
      departmentHouses = []
    }: import('../src/renderer/src/components/3d/HiveArchipelago3D').HiveArchipelago3DProps) {
      return (
        <div data-testid="hive-archipelago-3d">
          {departmentHouses.map((lot) => (
            <div
              key={`${lot.departmentKey}-${lot.slot}`}
              data-testid={`hive-archipelago-house-${lot.departmentKey}-${lot.slot}`}
              data-dept={lot.departmentKey}
              data-active={lot.active ? 'true' : 'false'}
            />
          ))}
        </div>
      )
    }
  }
})

import {
  buildCityMapView,
  computeCityLayoutEfficiency,
  optimizeDepartmentOrder,
  parseHiveCityMapSnapshot
} from '../src/shared/hive-city-map'
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
  const originalGetContext = HTMLCanvasElement.prototype.getContext

  beforeEach(() => {
    HTMLCanvasElement.prototype.getContext = ((type: string) =>
      type === 'webgl2' || type === 'webgl' ? ({} as RenderingContext) : null) as typeof HTMLCanvasElement.prototype.getContext
  })

  afterEach(() => {
    HTMLCanvasElement.prototype.getContext = originalGetContext
    cleanup()
    window.localStorage.clear()
  })

  it('renders the live scene in simulation mode with archipelago data', () => {
    render(<StudioApp />)
    const scene = screen.getByTestId('hive-city-scene')
    expect(scene.getAttribute('data-mode')).toBe('3d')
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

  it('persists studio settings across remounts in localStorage', async () => {
    const user = userEvent.setup()
    const { unmount } = render(<StudioApp />)

    await user.click(screen.getByRole('button', { name: 'Открыть пульт' }))
    await user.click(screen.getByRole('button', { name: 'Добавить спутник' }))
    await user.selectOptions(screen.getByLabelText('Статус'), 'sleeping')
    unmount()

    render(<StudioApp />)
    expect(screen.getByLabelText('Пульт Hive Studio')).toBeTruthy()
    expect(screen.getByText('спутников: 4')).toBeTruthy()
    expect((screen.getByLabelText('Статус') as HTMLSelectElement).value).toBe('sleeping')
  })

  it('includes CodexGUI, MemPulse, and PCPulse as the first satellite cities in simulation', () => {
    const snapshot = buildHiveStudioSimulation({ satelliteCount: 3 })
    expect(snapshot.satellites.length).toBe(3)
    expect(snapshot.satellites[0]?.name).toBe('CodexGUI')
    expect(snapshot.satellites[0]?.ecosystem).toBe('dotnet')
    expect(snapshot.satellites[1]?.name).toBe('MemPulse')
    expect(snapshot.satellites[1]?.ecosystem).toBe('dotnet')
    expect(snapshot.satellites[2]?.name).toBe('PCPulse')
    expect(snapshot.satellites[2]?.ecosystem).toBe('dotnet')
  })

  it('expands capital city rings and island radius proportionally when agent count grows', () => {
    const snapshot = buildHiveStudioSimulation({ satelliteCount: 1 })
    const compact = buildCityMapView(snapshot, null, { optimizeByQueen: true, totalHouses: 18 })
    const expanded = buildCityMapView(snapshot, null, { optimizeByQueen: true, totalHouses: 36 })

    expect(expanded.capital.radius).toBeGreaterThan(compact.capital.radius)
    const compactModules = compact.capital.buildings.filter((b) => b.building.kind !== 'citadel')
    const expandedModules = expanded.capital.buildings.filter((b) => b.building.kind !== 'citadel')
    const minCompactDist = Math.min(...compactModules.map((b) => Math.hypot(b.x, b.z)))
    const minExpandedDist = Math.min(...expandedModules.map((b) => Math.hypot(b.x, b.z)))
    expect(minExpandedDist).toBeGreaterThan(minCompactDist + 3)
  })

  it('places satellite cities along hexagonal honeycomb face normals around capital', () => {
    const snapshot = buildHiveStudioSimulation({ satelliteCount: 3 })
    const view = buildCityMapView(snapshot, null, { optimizeByQueen: true, totalHouses: 18 })
    const expectedAngles = [Math.PI / 6, (5 * Math.PI) / 6, -Math.PI / 2]
    const distances = view.satellites.map((sat) => Math.hypot(sat.x, sat.z))
    expect(Math.abs((distances[0] ?? 0) - (distances[1] ?? 0))).toBeLessThan(1e-6)
    expect(Math.abs((distances[1] ?? 0) - (distances[2] ?? 0))).toBeLessThan(1e-6)
    view.satellites.forEach((sat, index) => {
      const actualAngle = Math.atan2(sat.z, sat.x)
      expect(actualAngle).toBeCloseTo(expectedAngles[index] ?? 0, 5)
    })
  })

  it('optimizes building and department placement when Queen optimization is enabled', async () => {
    const snapshot = buildHiveStudioSimulation({ satelliteCount: 0 })
    const standardView = buildCityMapView(snapshot, null, { optimizeByQueen: false }).capital
    const optimizedView = buildCityMapView(snapshot, null, { optimizeByQueen: true }).capital
    expect(optimizedView.buildings).toHaveLength(standardView.buildings.length)

    const standardEff = computeCityLayoutEfficiency(standardView)
    const optimizedEff = computeCityLayoutEfficiency(optimizedView)
    expect(optimizedEff.avgGateDistance).toBeLessThanOrEqual(standardEff.avgGateDistance)

    const ordered = optimizeDepartmentOrder(HIVE_STUDIO_DEPARTMENTS, true)
    expect(ordered).toHaveLength(HIVE_STUDIO_DEPARTMENTS.length)
    expect(ordered[0]!.activeCount).toBeGreaterThanOrEqual(ordered[Math.floor(ordered.length / 2)]!.activeCount)

    const user = userEvent.setup()
    const { unmount } = render(<StudioApp />)
    await user.click(screen.getByRole('button', { name: 'Открыть пульт' }))
    const toggle = screen.getByLabelText('Королева оптимизирует город') as HTMLInputElement
    expect(toggle.checked).toBe(true)
    expect(screen.getByTestId('hive-studio-optimization-metric').textContent).toContain('Оптимизировано')
    await user.click(toggle)
    expect(toggle.checked).toBe(false)
    expect(screen.getByTestId('hive-studio-optimization-metric').textContent).toContain('Стандартная раскладка')
    unmount()

    render(<StudioApp />)
    expect((screen.getByLabelText('Королева оптимизирует город') as HTMLInputElement).checked).toBe(false)
  })
})
