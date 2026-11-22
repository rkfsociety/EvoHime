/**
 * Hive Studio — dev-only стенд сцены города Улья.
 *
 * Что даёт:
 * - настоящий `HiveCityScene` (тот же компонент, что в дашборде) в обычном
 *   браузере, без Electron и без собранного Core;
 * - источник данных по выбору: симуляция (детерминированный снапшот
 *   `hive-studio-simulation.ts`), реальный демон (вопрос `get_city_map` через
 *   dev-middleware из `vite.studio.config.ts`) или чистая панорама без карты;
 * - живые переключатели сцены: занятость департаментов, статус наблюдения,
 *   работа Королевы, количество спутников (анимации появления/исчезновения).
 *
 * Редактируешь компоненты 3D — Vite перезагружает сцену здесь и сейчас;
 * обновлять установленную EvoHime не нужно. Страница не входит в
 * production-бандл (rollup input перечисляет только index.html).
 */

import { useCallback, useEffect, useMemo, useState } from 'react'

import type { HiveCityMapSnapshot } from '@shared/hive-city-map'
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

type StudioSource = 'simulation' | 'daemon' | 'panorama'

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

const LIVE_POLL_INTERVAL_MS = 2000

type LiveOutcome =
  | { readonly available: true; readonly snapshot: HiveCityMapSnapshot }
  | { readonly available: false; readonly snapshot: null }

const UNAVAILABLE: LiveOutcome = { available: false, snapshot: null }

/** Один опрос наблюдательного pipe демона через dev-middleware. */
async function pollLiveCityMap(signal: AbortSignal): Promise<LiveOutcome> {
  try {
    const response = await fetch('/hive-studio-api/city-map.json', { signal })
    if (!response.ok) return UNAVAILABLE
    const payload = (await response.json()) as LiveOutcome
    if (typeof payload !== 'object' || payload === null) return UNAVAILABLE
    return payload
  } catch {
    return UNAVAILABLE
  }
}

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

/** Экспортирован для smoke-тестов стенда (tests/hive-studio.test.tsx). */
export function StudioApp(): React.JSX.Element {
  const [source, setSource] = useState<StudioSource>('simulation')
  const [status, setStatus] = useState<StudioStatus>('observing')
  const [queenWorking, setQueenWorking] = useState(false)
  const [departments, setDepartments] = useState<readonly HiveStudioDepartmentState[]>(
    HIVE_STUDIO_DEPARTMENTS
  )
  const [selectedDepartment, setSelectedDepartment] = useState<string | null>(null)
  const [satelliteCount, setSatelliteCount] = useState(2)
  const [rescanTick, setRescanTick] = useState(0)
  const [liveOutcome, setLiveOutcome] = useState<LiveOutcome>(UNAVAILABLE)

  useEffect(() => {
    if (source !== 'daemon') return undefined
    const controller = new AbortController()
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | null = null
    const run = async (): Promise<void> => {
      const outcome = await pollLiveCityMap(controller.signal)
      if (cancelled) return
      setLiveOutcome(outcome)
      timer = setTimeout(() => void run(), LIVE_POLL_INTERVAL_MS)
    }
    void run()
    return () => {
      cancelled = true
      controller.abort()
      if (timer !== null) clearTimeout(timer)
    }
  }, [source])

  const simulation = useMemo(
    () =>
      buildHiveStudioSimulation({
        generatedAtMs: source === 'simulation' ? Date.now() + rescanTick : Date.now(),
        satelliteCount,
        seed: 199 + rescanTick
      }),
    [source, satelliteCount, rescanTick]
  )

  const cityMap = useMemo(() => {
    if (source === 'panorama') return null
    if (source === 'daemon') return liveOutcome
    return { available: true, snapshot: simulation } as const
  }, [source, simulation, liveOutcome])

  const adjustActive = useCallback((key: string, delta: number): void => {
    setDepartments((current) =>
      current.map((department) =>
        department.key === key
          ? {
              ...department,
              activeCount: Math.min(
                department.maxCapacity,
                Math.max(0, department.activeCount + delta)
              )
            }
          : department
      )
    )
  }, [])

  const handleSelectDepartment = useCallback((key: string): void => {
    setSelectedDepartment((current) => (current === key ? null : key))
  }, [])

  const selected = departments.find((item) => item.key === selectedDepartment) ?? null

  const queenOverlay = (
    <div className="hive-queen-card">
      <div className="hive-queen-card__head">
        <div className="hive-queen-card__identity">
          <span className="hive-queen-card__emblem" aria-hidden="true">
            <BrandLotus size={20} />
          </span>
          <div>
            <p className="hive-queen__eyebrow">Eva core · EvoHime</p>
            <h2>Королева Ева</h2>
            <p className="hive-queen__role">Центр управления</p>
          </div>
        </div>
        <p className={`hive-queen__status hive-queen__status--${queenWorking ? 'active' : 'observing'}`}>
          <span className="hive-dot" aria-hidden="true" />
          {queenWorking ? 'Работает' : 'Наблюдение'}
        </p>
      </div>
      <label className="hive-settings-toggle" htmlFor="studio-queen-autonomy">
        <span>Автономность Королевы</span>
        <input
          id="studio-queen-autonomy"
          type="checkbox"
          role="switch"
          aria-label="Автономность Королевы"
          checked={queenWorking}
          onChange={(changeEvent) => setQueenWorking(changeEvent.target.checked)}
        />
      </label>
      <div className="hive-queen-card__foot">
        <p className="hive-queen__note">
          {queenWorking
            ? 'Анализирует результат инструмента'
            : 'Координирует агентов и выполняет задачи'}
        </p>
        <button type="button" className="hive-queen-card__chat-btn" disabled>
          Написать Еве
        </button>
      </div>
    </div>
  )

  return (
    <div className="hive-studio">
      <aside className="hive-studio__panel" aria-label="Пульт Hive Studio">
        <div className="hive-studio__brand">
          <BrandLotus size={18} />
          <div>
            <h1>Hive Studio</h1>
            <p>dev-стенд сцены Улья · настоящий HiveCityScene</p>
          </div>
        </div>

        <fieldset className="hive-studio__group">
          <legend>Источник данных</legend>
          {(
            [
              { value: 'simulation', label: 'Симуляция' },
              { value: 'daemon', label: 'Реальный демон' },
              { value: 'panorama', label: 'Панорама (без карты)' }
            ] as const
          ).map((option) => (
            <label key={option.value} className="hive-studio__radio">
              <input
                type="radio"
                name="studio-source"
                checked={source === option.value}
                onChange={() => setSource(option.value)}
              />
              <span>{option.label}</span>
            </label>
          ))}
          {source === 'daemon' ? (
            <p className="hive-studio__hint" data-testid="hive-studio-live-state">
              {liveOutcome.available
                ? `Демон отвечает · спутников: ${liveOutcome.snapshot.satellites.length} · опрос каждые ${LIVE_POLL_INTERVAL_MS / 1000} с`
                : 'Демон недоступен — покажется статус «unavailable»'}
            </p>
          ) : null}
        </fieldset>

        {source !== 'daemon' ? (
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
                onClick={() => setSatelliteCount((value) => Math.min(3, value + 1))}
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
        ) : null}

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
        </fieldset>

        {source !== 'daemon' ? (
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
        ) : null}
      </aside>

      <main className="hive-stage hive-stage--studio">
        <HiveCityScene
          status={status}
          departments={departmentPins(departments)}
          selectedDepartment={selectedDepartment}
          onSelectDepartment={handleSelectDepartment}
          queenWorking={queenWorking}
          queenOverlay={queenOverlay}
          cityMap={cityMap}
          activeAgents={agentHints(departments)}
        />
      </main>
    </div>
  )
}
