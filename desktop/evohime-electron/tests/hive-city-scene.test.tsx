// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { HiveCityMapSnapshot } from '../src/shared/hive-city-map'
import { HiveCityScene, type HiveCityDepartmentPin } from '../src/renderer/src/components/3d'
import {
  QUEEN_GATE_ANGLE,
  computeQueenGateStopAngle,
  computeRingEntryStopProgress
} from '../src/renderer/src/components/3d/HiveArchipelago3D'

describe('HiveCityScene', () => {
  afterEach(() => {
    cleanup()
  })

  it('renders 2.5D city backdrop, fx layer, normalized status, and aria-hidden in decorative mode', () => {
    const { rerender } = render(<HiveCityScene status="observing" />)

    const scene = screen.getByTestId('hive-city-scene')
    expect(scene.getAttribute('data-status')).toBe('observing')
    expect(scene.getAttribute('aria-hidden')).toBe('true')
    expect(screen.getByTestId('hive-city-scene-art')).toBeTruthy()
    expect(screen.getByTestId('hive-city-scene-fx')).toBeTruthy()

    rerender(<HiveCityScene status="active" />)
    expect(screen.getByTestId('hive-city-scene').getAttribute('data-status')).toBe('active')

    rerender(<HiveCityScene status="" />)
    expect(screen.getByTestId('hive-city-scene').getAttribute('data-status')).toBe('idle')
  })

  it('renders interactive department pins, highlights active conduits, and emits selection callbacks', async () => {
    const user = userEvent.setup()
    const onSelectDepartment = vi.fn()
    const departments: readonly HiveCityDepartmentPin[] = [
      {
        key: 'development',
        label: 'Разработка',
        subtitle: 'Разработка кода',
        icon: 'terminal',
        activeCount: 2,
        maxCapacity: 4
      },
      {
        key: 'research',
        label: 'Исследования',
        subtitle: 'Поиск решений',
        icon: 'target',
        activeCount: 0,
        maxCapacity: 4
      }
    ]

    render(
      <HiveCityScene
        status="active"
        departments={departments}
        selectedDepartment="development"
        onSelectDepartment={onSelectDepartment}
      />
    )

    const scene = screen.getByTestId('hive-city-scene')
    expect(scene.getAttribute('aria-hidden')).toBeNull()

    const devPin = screen.getByTestId('hive-pin-development')
    const researchPin = screen.getByTestId('hive-pin-research')
    expect(devPin.getAttribute('aria-pressed')).toBe('true')
    expect(devPin.textContent).toContain('Работает · 2/4')
    expect(researchPin.getAttribute('aria-pressed')).toBe('false')
    expect(researchPin.textContent).toContain('Ожидание')

    const conduits = screen.getAllByTestId('hive-city-conduit')
    const devConduit = conduits.find((item) => item.getAttribute('data-dept') === 'development')
    const researchConduit = conduits.find((item) => item.getAttribute('data-dept') === 'research')
    expect(devConduit?.getAttribute('data-active')).toBe('true')
    expect(researchConduit?.getAttribute('data-active')).toBe('false')

    await user.click(researchPin)
    expect(onSelectDepartment).toHaveBeenCalledWith('research')
  })
})

const satelliteIsland = (id: string, name: string) => ({
  id,
  name,
  role: 'satellite' as const,
  source: 'live_workspace' as const,
  status: 'ready' as const,
  reason: null,
  ecosystem: 'rust' as const,
  structure_hash: 'a'.repeat(16),
  revision_hash: 'b'.repeat(16),
  accent_hue: 212,
  districts: [{ id: `${id}-core`, label: 'Ядро', kind: 'core' as const }],
  buildings: [
    {
      id: `${id}/src/main.rs`,
      name: 'main.rs',
      district_id: `${id}-core`,
      kind: 'worker' as const,
      relative_path: 'src/main.rs',
      file_count: 3,
      line_count: 300,
      height_weight_bp: 320,
      language: 'rust'
    }
  ],
  edges: [],
  total_files: 3,
  total_lines: 300
})

const capitalIsland = {
  id: 'capital-evohime',
  name: 'Столица Евы',
  role: 'capital' as const,
  source: 'live_workspace' as const,
  status: 'ready' as const,
  reason: null,
  ecosystem: 'rust' as const,
  structure_hash: 'c'.repeat(16),
  revision_hash: 'd'.repeat(16),
  accent_hue: 42,
  districts: [{ id: 'capital-core', label: 'Цитадель', kind: 'core' as const }],
  buildings: [
    {
      id: 'crates/evohime-core',
      name: 'evohime-core',
      district_id: 'capital-core',
      kind: 'citadel' as const,
      relative_path: 'crates/evohime-core',
      file_count: 40,
      line_count: 9000,
      height_weight_bp: 1000,
      language: 'rust'
    }
  ],
  edges: [],
  total_files: 40,
  total_lines: 9000
}

const archipelagoSnapshot: HiveCityMapSnapshot = {
  schema_version: 1,
  generated_at_ms: 1_700_000_000_000,
  revision_hash: 'd'.repeat(32),
  capital: capitalIsland,
  satellites: [satelliteIsland('sat-alpha', 'Альфа')]
}

describe('HiveCityScene — живой архипелаг (план 199.3)', () => {
  afterEach(() => {
    cleanup()
  })

  it('в jsdom рендерит архипелаг в 2D-режиме с чипами и башнями', () => {
    render(
      <HiveCityScene
        status="observing"
        cityMap={{ available: true, snapshot: archipelagoSnapshot }}
      />
    )

    const scene = screen.getByTestId('hive-city-scene')
    expect(scene.getAttribute('data-mode')).toBe('2d')
    expect(screen.getByTestId('hive-archipelago-2d')).toBeTruthy()
    // Чипы: столица + спутник.
    expect(screen.getByTestId('hive-archipelago-chip-capital-evohime').textContent).toContain('Столица Евы')
    expect(screen.getByTestId('hive-archipelago-chip-sat-alpha').textContent).toContain('Альфа')
    // Башни зданий из детерминированной раскладки.
    expect(screen.getByTestId('hive-tower-crates/evohime-core')).toBeTruthy()
    expect(screen.getByTestId('hive-tower-sat-alpha/src/main.rs')).toBeTruthy()
    // Архипелаг дополняет панораму, HUD-виньетка не рисуется в архиве 2D.
    expect(screen.queryByTestId('hive-city-scene-art')).toBeNull()
  })

  it('клик по башне открывает инспектор, Escape и кнопка закрывают', async () => {
    const user = userEvent.setup()
    render(
      <HiveCityScene
        status="observing"
        cityMap={{ available: true, snapshot: archipelagoSnapshot }}
      />
    )

    expect(screen.queryByTestId('hive-archipelago-inspector')).toBeNull()
    fireEvent.pointerDown(screen.getByTestId('hive-tower-sat-alpha/src/main.rs'))

    const inspector = screen.getByTestId('hive-archipelago-inspector')
    expect(inspector.getAttribute('role')).toBe('dialog')
    // Инспектор показывает модуль здания: имя, тип, относительный путь.
    expect(inspector.textContent).toContain('main.rs')
    expect(inspector.textContent).toContain('src/main.rs')

    // Спутник в фокусе: чип альфа нажат.
    expect(screen.getByTestId('hive-archipelago-chip-sat-alpha').getAttribute('aria-pressed')).toBe('true')

    await user.click(screen.getByTestId('hive-archipelago-inspector-close'))
    expect(screen.queryByTestId('hive-archipelago-inspector')).toBeNull()
    // Фокус снят после закрытия инспектора? Нет: Escape нужен для возврата обзора.
    fireEvent.keyDown(screen.getByTestId('hive-city-scene'), { key: 'Escape' })
    expect(screen.getByTestId('hive-archipelago-chip-sat-alpha').getAttribute('aria-pressed')).toBe('false')
  })

  it('клик по чипу-спутнику выделяет остров, повторный клик и фон снимают фокус', async () => {
    const user = userEvent.setup()
    render(
      <HiveCityScene
        status="observing"
        cityMap={{ available: true, snapshot: archipelagoSnapshot }}
      />
    )

    await user.click(screen.getByTestId('hive-archipelago-chip-sat-alpha'))
    expect(screen.getByTestId('hive-archipelago-chip-sat-alpha').getAttribute('aria-pressed')).toBe('true')

    // Клик по фону SVG (по воде) возвращает общий обзор.
    const svg = screen.getByTestId('hive-archipelago-2d')
    fireEvent.pointerDown(svg, { target: svg })
    expect(screen.getByTestId('hive-archipelago-chip-sat-alpha').getAttribute('aria-pressed')).toBe('false')
  })

  it('недоступный демон сохраняет панораму и показывает статус', () => {
    render(
      <HiveCityScene
        status="observing"
        cityMap={{ available: false, snapshot: null }}
      />
    )
    const scene = screen.getByTestId('hive-city-scene')
    expect(scene.getAttribute('data-mode')).toBe('panorama')
    expect(screen.getByTestId('hive-city-scene-art')).toBeTruthy()
    expect(screen.getByTestId('hive-city-unavailable')).toBeTruthy()
  })

  it('чипы ограничены 13 островами (столица + до 12 спутников)', () => {
    const fullSnapshot: HiveCityMapSnapshot = {
      ...archipelagoSnapshot,
      satellites: Array.from({ length: 14 }, (_, index) =>
        satelliteIsland(`sat-${index}`, `Спутник ${index}`)
      )
    }
    render(
      <HiveCityScene
        status="observing"
        cityMap={{ available: true, snapshot: fullSnapshot }}
      />
    )
    const chips = screen.getAllByTestId(/^hive-archipelago-chip-/)
    expect(chips).toHaveLength(13)
  })

  it('в архипелаге департаменты — дома на окраине столицы, а не-floating пины', async () => {
    const user = userEvent.setup()
    const onSelectDepartment = vi.fn()
    const departments: readonly HiveCityDepartmentPin[] = [
      {
        key: 'development',
        label: 'Разработка',
        subtitle: 'Разработка кода',
        icon: 'terminal',
        activeCount: 2,
        maxCapacity: 3
      },
      {
        key: 'research',
        label: 'Исследования',
        subtitle: 'Поиск решений',
        icon: 'target',
        activeCount: 0,
        maxCapacity: 2
      }
    ]
    render(
      <HiveCityScene
        status="observing"
        departments={departments}
        selectedDepartment="development"
        onSelectDepartment={onSelectDepartment}
        cityMap={{ available: true, snapshot: archipelagoSnapshot }}
      />
    )

    // Пины панорамы скрыты, дома работников отрисованы на окраине столицы.
    expect(screen.queryByTestId('hive-pin-development')).toBeNull()
    // 3 слота разработки + 2 слота исследований = 5 домов.
    const houses = screen.getAllByTestId(/^hive-archipelago-house-/)
    expect(houses).toHaveLength(5)

    const devHouses = houses.filter((house) => house.getAttribute('data-dept') === 'development')
    const activeDevHouses = devHouses.filter((house) => house.getAttribute('data-active') === 'true')
    expect(devHouses).toHaveLength(3)
    expect(activeDevHouses).toHaveLength(2)

    const house = screen.getByTestId('hive-archipelago-house-development-0')
    await user.click(house)
    expect(onSelectDepartment).toHaveBeenCalledWith('development')
  })

  it('жизненный цикл спутника: entering → active → exiting → размонтирование', () => {
    vi.useFakeTimers()
    try {
      const { rerender } = render(
        <HiveCityScene status="observing" cityMap={{ available: true, snapshot: archipelagoSnapshot }} />
      )

      // Добавляем спутник: фаза entering.
      const withTwo: HiveCityMapSnapshot = {
        ...archipelagoSnapshot,
        satellites: [satelliteIsland('sat-alpha', 'Альфа'), satelliteIsland('sat-beta', 'Бета')]
      }
      rerender(
        <HiveCityScene status="observing" cityMap={{ available: true, snapshot: withTwo }} />
      )

      const betaIsland = screen.getByTestId('hive-tower-sat-beta/src/main.rs').closest('[data-island]')
      expect(betaIsland?.getAttribute('class')).toContain('hive-archipelago__island--entering')

      act(() => {
        vi.advanceTimersByTime(700)
      })
      const betaIslandAfter = screen.getByTestId('hive-tower-sat-beta/src/main.rs').closest('[data-island]')
      expect(betaIslandAfter?.getAttribute('class'))
        .toContain('hive-archipelago__island--active')

      // Убираем спутники Бета: exiting, затем размонтирование.
      rerender(
        <HiveCityScene status="observing" cityMap={{ available: true, snapshot: archipelagoSnapshot }} />
      )
      expect(screen.getByTestId('hive-tower-sat-beta/src/main.rs').closest('[data-island]')?.getAttribute('class'))
        .toContain('hive-archipelago__island--exiting')

      act(() => {
        vi.advanceTimersByTime(800)
      })
      expect(screen.queryByTestId('hive-tower-sat-beta/src/main.rs')).toBeNull()
      // Альфа осталась активной.
      expect(screen.getByTestId('hive-tower-sat-alpha/src/main.rs')).toBeTruthy()
    } finally {
      vi.useRealTimers()
    }
  })

  it('keeps waiting cars strictly before intersection borders at queen gate and ring entries', () => {
    for (const innerR of [2.9, 3.5, 4.4]) {
      const stopAngle = computeQueenGateStopAngle(innerR)
      // Полуширина дороги ворот (0.32) + полудлина машины (0.18) = 0.50 по дуге.
      const frontBumperArc = (QUEEN_GATE_ANGLE - stopAngle) * innerR - 0.18
      expect(frontBumperArc).toBeGreaterThan(0.32)

      const homeEdgeR = innerR + 0.8
      const stopProgress = computeRingEntryStopProgress(innerR, homeEdgeR)
      const carCenterR = homeEdgeR - stopProgress * (homeEdgeR - innerR)
      // Внешний бордюр кольца на innerR + 0.42; передний бампер (center - 0.18) должен оставаться снаружи.
      expect(carCenterR - 0.18).toBeGreaterThan(innerR + 0.42)
    }
  })
})
