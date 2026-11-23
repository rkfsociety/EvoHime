/**
 * Hive Studio — dev-only стенд сцены города Улья.
 *
 * Что даёт:
 * - настоящий `HiveCityScene` (тот же компонент, что в дашборде) в обычном
 *   браузере, без Electron и без собранного Core;
 * - отображает реальную 3D-визуализацию архипелага с симуляцией снапшота
 *   (`hive-studio-simulation.ts`);
 * - живые переключатели сцены: занятость департаментов, статус наблюдения,
 *   работа Королевы, количество спутников (анимации появления/исчезновения).
 *
 * Редактируешь компоненты 3D — Vite перезагружает сцену здесь и сейчас;
 * обновлять установленную EvoHime не нужно. Страница не входит в
 * production-бандл (rollup input перечисляет только index.html).
 */

import { useCallback, useEffect, useMemo, useState } from 'react'

import { buildCityMapView, computeCityLayoutEfficiency } from '@shared/hive-city-map'
import { BrandLotus } from './BrandLotus'
import { HiveCityScene, type HiveCityDepartmentPin } from './components/3d'
import {
  buildHiveStudioSimulation,
  HIVE_STUDIO_DEPARTMENTS,
  type HiveStudioDepartmentState
} from './hive-studio-simulation'
// Порядок важен: styles.css обязан идти раньше hive-studio.css — CSS
// переопределения стенда (.hive-stage--studio) имеют ту же специфичность,
// что классы оболочки, и при обратном порядке импортов проигрывают.
import './styles.css'
import './hive-studio.css'

type StudioStatus =
  | 'observing'
  | 'active'
  | 'waiting_for_limits'
  | 'waiting_for_tasks'
  | 'disabled'
  | 'sleeping'

const STATUS_OPTIONS: readonly { value: StudioStatus; label: string }[] = [
  { value: 'observing', label: 'Наблюдение' },
  { value: 'active', label: 'Активна' },
  { value: 'waiting_for_limits', label: 'Лимит провайдера' },
  { value: 'waiting_for_tasks', label: 'Ожидание задач' },
  { value: 'disabled', label: 'Выключена' },
  { value: 'sleeping', label: 'Спит' }
]

function departmentPins(departments: readonly HiveStudioDepartmentState[]): HiveCityDepartmentPin[] {
  return departments.map((department) => ({
    key: department.key,
    label: department.label,
    subtitle: department.subtitle,
    icon: department.icon,
    activeCount: department.activeCount,
    maxCapacity: department.maxCapacity
  }))
}

function agentHints(departments: readonly HiveStudioDepartmentState[]): {
  readonly id: string
  readonly role: string
  readonly currentAction: string
}[] {
  const hints: { id: string; role: string; currentAction: string }[] = []
  for (const department of departments) {
    for (let index = 0; index < department.activeCount; index += 1) {
      hints.push({
        id: `${department.key}-${index}`,
        role: department.label,
        currentAction: index === 0 ? 'Выполняет задачу' : 'Помогает департаменту'
      })
    }
  }
  return hints
}

const STUDIO_SETTINGS_STORAGE_KEY = 'evohime:hive-studio:settings:v1'

interface PersistedStudioSettings {
  readonly status: StudioStatus
  readonly queenWorking: boolean
  readonly queenOptimized: boolean
  readonly satelliteCount: number
  readonly selectedDepartment: string | null
  readonly panelOpen: boolean
  readonly departments: readonly HiveStudioDepartmentState[]
}

function loadStudioSettings(): PersistedStudioSettings {
  const defaults: PersistedStudioSettings = {
    status: 'active',
    queenWorking: true,
    queenOptimized: true,
    satelliteCount: 1,
    selectedDepartment: null,
    panelOpen: false,
    departments: HIVE_STUDIO_DEPARTMENTS
  }
  try {
    if (typeof window === 'undefined' || !window.localStorage) return defaults
    const raw = window.localStorage.getItem(STUDIO_SETTINGS_STORAGE_KEY)
    if (!raw) return defaults
    const parsed = JSON.parse(raw) as Record<string, unknown>
    const statusCandidate = parsed.status
    const validStatus = STATUS_OPTIONS.some((item) => item.value === statusCandidate)
      ? (statusCandidate as StudioStatus)
      : defaults.status
    const queenWorking = typeof parsed.queenWorking === 'boolean' ? parsed.queenWorking : defaults.queenWorking
    const queenOptimized =
      typeof parsed.queenOptimized === 'boolean' ? parsed.queenOptimized : defaults.queenOptimized
    const satelliteCount =
      typeof parsed.satelliteCount === 'number' && Number.isInteger(parsed.satelliteCount)
        ? Math.max(0, Math.min(6, parsed.satelliteCount))
        : defaults.satelliteCount
    const panelOpen = typeof parsed.panelOpen === 'boolean' ? parsed.panelOpen : defaults.panelOpen
    const savedDepts = Array.isArray(parsed.departments) ? parsed.departments : []
    const byKey = new Map<string, { activeCount?: unknown; maxCapacity?: unknown }>()
    for (const entry of savedDepts) {
      if (entry && typeof entry === 'object' && typeof (entry as { key?: unknown }).key === 'string') {
        byKey.set((entry as { key: string }).key, entry as { activeCount?: unknown; maxCapacity?: unknown })
      }
    }
    const departments = HIVE_STUDIO_DEPARTMENTS.map((base) => {
      const saved = byKey.get(base.key)
      if (!saved) return base
      const maxCapacity =
        typeof saved.maxCapacity === 'number' && Number.isInteger(saved.maxCapacity)
          ? Math.max(1, Math.min(5, saved.maxCapacity))
          : base.maxCapacity
      const activeCount =
        typeof saved.activeCount === 'number' && Number.isInteger(saved.activeCount)
          ? Math.max(0, Math.min(maxCapacity, saved.activeCount))
          : Math.min(maxCapacity, base.activeCount)
      return { ...base, maxCapacity, activeCount }
    })
    const selectedDepartment =
      typeof parsed.selectedDepartment === 'string' &&
      departments.some((item) => item.key === parsed.selectedDepartment)
        ? parsed.selectedDepartment
        : null
    return {
      status: validStatus,
      queenWorking,
      queenOptimized,
      satelliteCount,
      selectedDepartment,
      panelOpen,
      departments
    }
  } catch {
    return defaults
  }
}

/** Экспортирован для smoke-тестов стенда (tests/hive-studio.test.tsx). */
export function StudioApp(): React.JSX.Element {
  const [initial] = useState<PersistedStudioSettings>(loadStudioSettings)
  const [status, setStatus] = useState<StudioStatus>(initial.status)
  const [queenWorking, setQueenWorking] = useState(initial.queenWorking)
  const [queenOptimized, setQueenOptimized] = useState(initial.queenOptimized)
  const [departments, setDepartments] = useState<readonly HiveStudioDepartmentState[]>(
    initial.departments
  )
  const [selectedDepartment, setSelectedDepartment] = useState<string | null>(initial.selectedDepartment)
  const [satelliteCount, setSatelliteCount] = useState(initial.satelliteCount)
  const [rescanTick, setRescanTick] = useState(0)
  const [panelOpen, setPanelOpen] = useState(initial.panelOpen)

  useEffect(() => {
    try {
      if (typeof window === 'undefined' || !window.localStorage) return
      const payload = {
        status,
        queenWorking,
        queenOptimized,
        satelliteCount,
        selectedDepartment,
        panelOpen,
        departments: departments.map((item) => ({
          key: item.key,
          activeCount: item.activeCount,
          maxCapacity: item.maxCapacity
        }))
      }
      window.localStorage.setItem(STUDIO_SETTINGS_STORAGE_KEY, JSON.stringify(payload))
    } catch {
      // Игнорируем ограничения квоты/приватного режима.
    }
  }, [status, queenWorking, queenOptimized, satelliteCount, selectedDepartment, panelOpen, departments])

  const simulation = useMemo(
    () =>
      buildHiveStudioSimulation({
        generatedAtMs: Date.now() + rescanTick,
        satelliteCount,
        seed: 199 + rescanTick
      }),
    [satelliteCount, rescanTick]
  )

  const cityMap = useMemo(() => ({ available: true, snapshot: simulation }) as const, [simulation])

  const adjustActive = useCallback((key: string, delta: number): void => {
    setDepartments((current) =>
      current.map((department) => {
        if (department.key !== key) return department
        if (delta > 0 && department.activeCount >= department.maxCapacity && department.maxCapacity < 5) {
          // Расширение сектора при добавлении нового агента сверх текущей вместимости.
          const nextCap = department.maxCapacity + 1
          return { ...department, maxCapacity: nextCap, activeCount: nextCap }
        }
        return {
          ...department,
          activeCount: Math.min(
            department.maxCapacity,
            Math.max(0, department.activeCount + delta)
          )
        }
      })
    )
  }, [])

  const handleSelectDepartment = useCallback((key: string): void => {
    setSelectedDepartment((current) => (current === key ? null : key))
  }, [])

  const pins = useMemo(() => departmentPins(departments), [departments])
  const activeAgentHints = useMemo(() => agentHints(departments), [departments])

  const selected = departments.find((item) => item.key === selectedDepartment) ?? null

  const optimizationSummary = useMemo(() => {
    const baseView = buildCityMapView(simulation, null, { optimizeByQueen: false }).capital
    const optView = buildCityMapView(simulation, null, { optimizeByQueen: true }).capital
    const baseEff = computeCityLayoutEfficiency(baseView)
    const optEff = computeCityLayoutEfficiency(optView)
    const gateDeltaPct =
      baseEff.avgGateDistance > 0
        ? Math.round(((baseEff.avgGateDistance - optEff.avgGateDistance) / baseEff.avgGateDistance) * 100)
        : 0
    return {
      gateDeltaPct,
      avgGateDistance: (queenOptimized ? optEff.avgGateDistance : baseEff.avgGateDistance).toFixed(1)
    }
  }, [simulation, queenOptimized])

  return (
    <div className="hive-studio">
      <button
        type="button"
        className="hive-studio__toggle"
        aria-expanded={panelOpen}
        aria-label={panelOpen ? 'Скрыть пульт' : 'Открыть пульт'}
        onClick={() => setPanelOpen((open) => !open)}
      >
        {panelOpen ? '×' : 'Пульт'}
      </button>

      {panelOpen ? (
      <aside className="hive-studio__panel" aria-label="Пульт Hive Studio">
        <div className="hive-studio__brand">
          <BrandLotus size={18} />
          <div>
            <h1>Hive Studio</h1>
            <p>dev-стенд сцены Улья · настоящий HiveCityScene</p>
          </div>
        </div>

        <fieldset className="hive-studio__group">
          <legend>Архипелаг</legend>
          <div className="hive-studio__row">
            <button
              type="button"
              onClick={() => setSatelliteCount((value) => Math.max(0, value - 1))}
              aria-label="Убрать спутник"
            >
              − спутник
            </button>
            <span className="hive-studio__count">спутников: {satelliteCount}</span>
            <button
              type="button"
              onClick={() => setSatelliteCount((value) => Math.min(4, value + 1))}
              aria-label="Добавить спутник"
            >
              + спутник
            </button>
          </div>
          <div className="hive-studio__row">
            <button type="button" onClick={() => setRescanTick((value) => value + 1)}>
              Пересборка (перерисовать)
            </button>
          </div>
        </fieldset>

        <fieldset className="hive-studio__group">
          <legend>Сцена</legend>
          <label className="hive-studio__row">
            <span>Статус</span>
            <select
              value={status}
              onChange={(changeEvent) => setStatus(changeEvent.target.value as StudioStatus)}
            >
              {STATUS_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label className="hive-studio__checkbox">
            <input
              type="checkbox"
              checked={queenWorking}
              onChange={(changeEvent) => setQueenWorking(changeEvent.target.checked)}
            />
            <span>Королева работает</span>
          </label>
          <label className="hive-studio__checkbox">
            <input
              type="checkbox"
              checked={queenOptimized}
              onChange={(changeEvent) => setQueenOptimized(changeEvent.target.checked)}
            />
            <span>Королева оптимизирует город</span>
          </label>
          <p className="hive-studio__hint" data-testid="hive-studio-optimization-metric">
            {queenOptimized
              ? `Оптимизировано · ср. путь: ${optimizationSummary.avgGateDistance} (−${Math.max(0, optimizationSummary.gateDeltaPct)}%)`
              : `Стандартная раскладка · ср. путь: ${optimizationSummary.avgGateDistance}`}
          </p>
        </fieldset>

        <fieldset className="hive-studio__group">
          <legend>Департаменты (активные / всего)</legend>
          <ul className="hive-studio__list">
            {departments.map((department) => (
              <li key={department.key}>
                <button
                  type="button"
                  className={`hive-studio__dept${selectedDepartment === department.key ? ' hive-studio__dept--selected' : ''}`}
                  onClick={() => handleSelectDepartment(department.key)}
                >
                  <span>{department.label}</span>
                  <span className="hive-studio__dept-cap">
                    {department.activeCount} / {department.maxCapacity}
                  </span>
                </button>
                <span className="hive-studio__stepper">
                  <button
                    type="button"
                    aria-label={`Меньше: ${department.label}`}
                    onClick={() => adjustActive(department.key, -1)}
                  >
                    −
                  </button>
                  <button
                    type="button"
                    aria-label={`Больше: ${department.label}`}
                    onClick={() => adjustActive(department.key, 1)}
                  >
                    +
                  </button>
                </span>
              </li>
            ))}
          </ul>
          {selected !== null ? (
            <p className="hive-studio__hint">Выбран департамент: {selected.label}</p>
          ) : null}
        </fieldset>
      </aside>
      ) : null}

      <main className="hive-stage hive-stage--studio">
        <HiveCityScene
          status={status}
          departments={pins}
          selectedDepartment={selectedDepartment}
          onSelectDepartment={handleSelectDepartment}
          queenWorking={queenWorking}
          queenOptimized={queenOptimized}
          cityMap={cityMap}
          activeAgents={activeAgentHints}
        />
      </main>
    </div>
  )
}
