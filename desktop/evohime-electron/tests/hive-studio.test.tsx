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
import {
  angularDistance,
  doCarBoxesOverlap,
  evalTurnBezier,
  isIntersectionClearForEntry,
  isPathStepBlocked,
  isPoseCollidingWithPeers,
  isVehicleAheadBlocked,
  type PathSegment,
  type VehiclePoseState
} from '../src/renderer/src/components/3d/hive-traffic'

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

  it('keeps the drawn queen citadel at the exact city center when the data has multiple citadels', () => {
    // Реальный манифест столицы содержит две цитадели (evohime-core и
    // evohime-hive). Башня Королевы рисуется по первой и обязана стоять
    // в точном центре города — там же, где центрируются все кольца.
    const base = buildHiveStudioSimulation({ satelliteCount: 0 })
    const capital = base.capital
    const citadel = capital.buildings.find((b) => b.kind === 'citadel')
    expect(citadel).toBeTruthy()
    const snapshot = {
      ...base,
      capital: {
        ...capital,
        buildings: [
          ...capital.buildings,
          { ...citadel!, id: 'crates/evohime-hive', name: 'evohime-hive', relative_path: 'crates/evohime-hive' }
        ]
      }
    }
    const view = buildCityMapView(snapshot, null, { optimizeByQueen: true })
    const citadelViews = view.capital.buildings.filter((b) => b.building.kind === 'citadel')
    expect(citadelViews).toHaveLength(2)
    expect(citadelViews[0]!.x).toBe(0)
    expect(citadelViews[0]!.z).toBe(0)
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

  it('prevents crossing/merging car collisions without deadlocks and guards occupied intersections', () => {
    // 1. Взаимное пересечение под 90°: ровно одна машина уступает, без взаимного deadlock.
    const peersCross: VehiclePoseState[] = [
      { lastVisible: true, lastX: 0, lastZ: -0.25, lastDirX: 0, lastDirZ: 1 },
      { lastVisible: true, lastX: -0.25, lastZ: 0, lastDirX: 1, lastDirZ: 0 }
    ]
    const blocked0 = isVehicleAheadBlocked(0, -0.25, 0, 1, 0, peersCross)
    const blocked1 = isVehicleAheadBlocked(-0.25, 0, 1, 0, 1, peersCross)
    expect(blocked0).toBe(true)
    expect(blocked1).toBe(false)

    // Машина на пути прямо перед носом под 90° блокирует едущую на неё машину.
    const peersAhead: VehiclePoseState[] = [
      { lastVisible: true, lastX: 0, lastZ: 0, lastDirX: 0, lastDirZ: 1 },
      { lastVisible: true, lastX: -0.35, lastZ: 0, lastDirX: 1, lastDirZ: 0 }
    ]
    expect(isVehicleAheadBlocked(-0.35, 0, 1, 0, 1, peersAhead)).toBe(true)
    expect(isVehicleAheadBlocked(0, 0, 0, 1, 0, peersAhead)).toBe(false)

    // 2. Встречные машины на параллельных полосах не блокируют друг друга.
    const peersOncoming: VehiclePoseState[] = [
      { lastVisible: true, lastX: 0.13, lastZ: 0, lastDirX: 0, lastDirZ: 1 },
      { lastVisible: true, lastX: -0.13, lastZ: 0.2, lastDirX: 0, lastDirZ: -1 }
    ]
    expect(isVehicleAheadBlocked(0.13, 0, 0, 1, 0, peersOncoming)).toBe(false)
    expect(isVehicleAheadBlocked(-0.13, 0.2, 0, -1, 1, peersOncoming)).toBe(false)

    // 3. Контроль занятости перекрёстка: блокирует конфликтующую внутри квадрата (±0.49), но пропускает за пределами перекрёстка (5.55).
    const insideIntersection: VehiclePoseState[] = [
      { lastVisible: true, lastX: 0.13, lastZ: 4.3, lastDirX: 0, lastDirZ: 1 },
      { lastVisible: true, lastX: 0.13, lastZ: 5.1, lastDirX: 1, lastDirZ: 0 }
    ]
    expect(isIntersectionClearForEntry(0, 5, 0.13, 4.3, 0, 1, 0, insideIntersection)).toBe(false)

    const outsideIntersection: VehiclePoseState[] = [
      { lastVisible: true, lastX: 0.13, lastZ: 4.3, lastDirX: 0, lastDirZ: 1 },
      { lastVisible: true, lastX: 0.26, lastZ: 5.55, lastDirX: 1, lastDirZ: 0 }
    ]
    expect(isIntersectionClearForEntry(0, 5, 0.13, 4.3, 0, 1, 0, outsideIntersection)).toBe(true)
  })

  it('enforces 2D OBB car collision geometry and blocks turning into occupied building corridors', () => {
    // 1. doCarBoxesOverlap: пересечения под 45° и 90° блокируются, соседние/встречные полосы — нет.
    expect(doCarBoxesOverlap(0, 0, 1, 0, 0.1, 0.1, Math.SQRT1_2, Math.SQRT1_2)).toBe(true)
    expect(doCarBoxesOverlap(0, 0, 1, 0, 0.1, 0.1, 0, 1)).toBe(true)
    expect(doCarBoxesOverlap(0, 0, 1, 0, 0, 0.17, 1, 0)).toBe(false)
    expect(doCarBoxesOverlap(0, 0, 1, 0, 0, 0.26, -1, 0)).toBe(false)

    // 2. isPoseCollidingWithPeers: шаг в сторону столкновения блокируется, шаг прочь разрешается.
    const obstaclePeers: VehiclePoseState[] = [
      { lastVisible: true, lastX: 0, lastZ: 0, lastDirX: 0, lastDirZ: 1 },
      { lastVisible: true, lastX: 0.2, lastZ: 0, lastDirX: 0, lastDirZ: 1 }
    ]
    // Перемещение ближе к мешающей машине (из x=0 в x=0.05) запрещено.
    expect(isPoseCollidingWithPeers(0.05, 0, 1, 0, 0, obstaclePeers, 0, 0)).toBe(true)
    // Разъезд от перекрытия (из x=0 в x=-0.05, дистанция увеличивается 0.20 -> 0.25) разрешён.
    expect(isPoseCollidingWithPeers(-0.05, 0, 1, 0, 0, obstaclePeers, 0, 0)).toBe(false)

    // 3. isPathStepBlocked: запрещает поворот к зданию, если в коридоре поворота мешает другая машина.
    const turnSegments: PathSegment[] = [
      {
        len: 0.5,
        eval: (u) => ({ x: 0, z: u * 0.5, dx: 0, dz: 1 })
      },
      {
        len: 1.0,
        eval: (u) => evalTurnBezier(0, 0.5, 0, 1.0, 0.5, 1.0, u)
      }
    ]
    const totalLength = 1.5
    // Машина на пути заезда к зданию в коридоре поворота:
    const blockedPeers: VehiclePoseState[] = [
      { lastVisible: true, lastX: 0, lastZ: 0.65, lastDirX: 0, lastDirZ: 1 },
      { lastVisible: true, lastX: 0.08, lastZ: 0.82, lastDirX: 1, lastDirZ: 0 }
    ]
    expect(isPathStepBlocked(turnSegments, totalLength, 0.48, 0, blockedPeers, 0, 0.65)).toBe(true)

    // После того как мешающая машина уехала:
    const clearPeers: VehiclePoseState[] = [
      { lastVisible: true, lastX: 0, lastZ: 0.65, lastDirX: 0, lastDirZ: 1 },
      { lastVisible: true, lastX: 0.08, lastZ: 2.0, lastDirX: 1, lastDirZ: 0 }
    ]
    expect(isPathStepBlocked(turnSegments, totalLength, 0.48, 0, clearPeers, 0, 0.65)).toBe(false)
  })

  it('builds and optimizes all satellite cities using Queen radial-ring planning without overlaps', () => {
    const snapshot = buildHiveStudioSimulation({ satelliteCount: 3 })
    const stdView = buildCityMapView(snapshot, null, { optimizeByQueen: false, totalHouses: 18 })
    const optView = buildCityMapView(snapshot, null, { optimizeByQueen: true, totalHouses: 18 })

    expect(optView.satellites.length).toBe(3)
    optView.satellites.forEach((sat, satIdx) => {
      const modules = sat.buildings.filter((b) => b.building.kind !== 'citadel')
      expect(modules.length).toBeGreaterThan(0)
      const gateAngle = Math.atan2(-sat.z, -sat.x)

      for (let i = 0; i < modules.length; i += 1) {
        const a = modules[i]!
        // Не перекрывает центральную площадь маяка.
        expect(Math.hypot(a.x, a.z)).toBeGreaterThanOrEqual(3.5)
        // Въездной проспект со стороны столицы свободен от зданий.
        const buildingAngle = Math.atan2(a.z, a.x)
        expect(angularDistance(buildingAngle, gateAngle)).toBeGreaterThanOrEqual(0.25)

        for (let j = i + 1; j < modules.length; j += 1) {
          const b = modules[j]!
          expect(Math.hypot(a.x - b.x, a.z - b.z)).toBeGreaterThanOrEqual(2.0)
        }
      }

      if (modules.length > 1) {
        const stdSat = stdView.satellites[satIdx]!
        const stdEff = computeCityLayoutEfficiency(stdSat)
        const optEff = computeCityLayoutEfficiency(sat)
        expect(optEff.avgGateDistance).toBeLessThanOrEqual(stdEff.avgGateDistance)
      }
    })
  })

  it('switches simulation speed multiplier buttons x1, x10, x100 in Hive Studio', async () => {
    const user = userEvent.setup()
    render(<StudioApp />)
    const speedGroup = screen.getByRole('group', { name: 'Скорость симуляции' })
    expect(speedGroup).toBeTruthy()
    const x1 = screen.getByRole('button', { name: 'x1' })
    const x10 = screen.getByRole('button', { name: 'x10' })
    const x100 = screen.getByRole('button', { name: 'x100' })
    expect(x1.getAttribute('aria-pressed')).toBe('true')
    expect(x10.getAttribute('aria-pressed')).toBe('false')
    expect(x100.getAttribute('aria-pressed')).toBe('false')

    await user.click(x10)
    expect(x1.getAttribute('aria-pressed')).toBe('false')
    expect(x10.getAttribute('aria-pressed')).toBe('true')

    await user.click(x100)
    expect(x10.getAttribute('aria-pressed')).toBe('false')
    expect(x100.getAttribute('aria-pressed')).toBe('true')
  })
})
