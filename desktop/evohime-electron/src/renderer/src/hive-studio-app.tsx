/**
 * Hive Studio — dev-only стенд 2D-сцены города Улья (План 200).
 *
 * Запускается через `npm run studio` (http://localhost:5178/hive-studio.html)
 * с HMR без Electron и без пересборки Евы.
 */

import { useMemo, useState } from 'react'

import { BrandLotus } from './BrandLotus'
import { HiveCityScene } from './components/hive'
import {
  buildHoneycombStudioWorld,
  type StudioGrowthStage
} from './hive-studio-simulation'
import './styles.css'
import './hive-studio.css'

const STAGE_OPTIONS: readonly { value: StudioGrowthStage; label: string }[] = [
  { value: 'zero', label: '0. Нулевой старт (только Цитадель)' },
  { value: 'scouting', label: '1. Разведка проекта' },
  { value: 'pipeline', label: '2. Полный конвейер и общение' },
  { value: 'remodeled', label: '3. Перестроенный город' }
]

/** Экспортирован для smoke-тестов стенда (tests/hive-studio.test.tsx). */
export function StudioApp(): React.JSX.Element {
  const [panelOpen, setPanelOpen] = useState(true)
  const [stage, setStage] = useState<StudioGrowthStage>('pipeline')
  const [paused, setPaused] = useState(false)
  const [evaSourceConfigured, setEvaSourceConfigured] = useState(true)
  const [projectCount, setProjectCount] = useState(2)
  const [agentCount, setAgentCount] = useState(5)
  const [exitsOverride, setExitsOverride] = useState<'auto' | 1 | 2 | 3 | 4 | 5 | 6>('auto')
  const [remodelSeed, setRemodelSeed] = useState(0)
  const [timeScale, setTimeScale] = useState<1 | 10 | 100>(1)

  const world = useMemo(
    () =>
      buildHoneycombStudioWorld({
        stage,
        paused,
        evaSourceConfigured,
        projectCount,
        agentCount,
        exitsOverride,
        remodelSeed
      }),
    [stage, paused, evaSourceConfigured, projectCount, agentCount, exitsOverride, remodelSeed]
  )

  const primaryCity = world.cities[0]

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

      <div className="hive-studio__speed" role="group" aria-label="Скорость симуляции">
        {([1, 10, 100] as const).map((speed) => (
          <button
            key={speed}
            type="button"
            className={`hive-studio__speed-btn${timeScale === speed ? ' hive-studio__speed-btn--active' : ''}`}
            aria-pressed={timeScale === speed}
            onClick={() => setTimeScale(speed)}
          >
            x{speed}
          </button>
        ))}
      </div>

      {panelOpen ? (
        <aside className="hive-studio__panel" aria-label="Пульт Hive Studio">
          <div className="hive-studio__brand">
            <BrandLotus size={18} />
            <div>
              <h1>Hive Studio · 2D Соты</h1>
              <p>Стенд Плана 200 · ПДД + конвейер Королевы</p>
            </div>
          </div>

          <fieldset className="hive-studio__group">
            <legend>Этап роста города</legend>
            <label className="hive-studio__row">
              <span>Сценарий</span>
              <select
                value={stage}
                onChange={(e) => setStage(e.target.value as StudioGrowthStage)}
              >
                {STAGE_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="hive-studio__checkbox">
              <input
                type="checkbox"
                checked={paused}
                onChange={(e) => setPaused(e.target.checked)}
              />
              <span>Пауза Улья (машины в домах)</span>
            </label>
            <div className="hive-studio__row">
              <button
                type="button"
                onClick={() => {
                  setStage('remodeled')
                  setRemodelSeed((s) => s + 1)
                }}
              >
                Перестроить город (анимация)
              </button>
            </div>
          </fieldset>

          <fieldset className="hive-studio__group">
            <legend>Агенты и сектора (3 места / сота)</legend>
            <div className="hive-studio__row">
              <button
                type="button"
                onClick={() => setAgentCount((v) => Math.max(0, v - 1))}
                aria-label="Убрать агента"
              >
                − агент
              </button>
              <span className="hive-studio__count" data-testid="studio-agent-summary">
                агентов: {agentCount} · секторов: {primaryCity?.sectors.length ?? 0}
              </span>
              <button
                type="button"
                onClick={() => setAgentCount((v) => Math.min(15, v + 1))}
                aria-label="Добавить агента"
              >
                + агент
              </button>
            </div>
            <label className="hive-studio__row">
              <span>Выходы Цитадели</span>
              <select
                value={String(exitsOverride)}
                onChange={(e) => {
                  const val = e.target.value
                  setExitsOverride(val === 'auto' ? 'auto' : (Number(val) as 1 | 2 | 3 | 4 | 5 | 6))
                }}
              >
                <option value="auto">Авто ({primaryCity?.citadelExitsCount ?? 1})</option>
                {[1, 2, 3, 4, 5, 6].map((n) => (
                  <option key={n} value={n}>
                    {n} {n === 1 ? 'выход' : n < 5 ? 'выхода' : 'выходов'}
                  </option>
                ))}
              </select>
            </label>
          </fieldset>

          <fieldset className="hive-studio__group">
            <legend>Мультипроектность</legend>
            <label className="hive-studio__checkbox">
              <input
                type="checkbox"
                checked={evaSourceConfigured}
                onChange={(e) => setEvaSourceConfigured(e.target.checked)}
              />
              <span>Исходники Евы настроены (приоритет)</span>
            </label>
            <div className="hive-studio__row">
              <button
                type="button"
                onClick={() => setProjectCount((v) => Math.max(1, v - 1))}
                aria-label="Убрать проект"
              >
                − проект
              </button>
              <span className="hive-studio__count">городов: {projectCount}</span>
              <button
                type="button"
                onClick={() => setProjectCount((v) => Math.min(4, v + 1))}
                aria-label="Добавить проект"
              >
                + проект
              </button>
            </div>
            <p className="hive-studio__hint">
              Основной город: <strong>{primaryCity?.projectName ?? '—'}</strong> · СКМ — перемещение,
              колёсико — зум.
            </p>
          </fieldset>
        </aside>
      ) : null}

      <main className="hive-stage hive-stage--studio">
        <HiveCityScene
          status={paused ? 'disabled' : 'active'}
          world={world}
          queenWorking={world.queenWorking}
          timeScale={timeScale}
        />
      </main>
    </div>
  )
}
