/**
 * Генератор демо-снапшота города Улья для стены разработки Hive Studio
 * (dev-only страница `hive-studio.html`, в production-бандл не попадает).
 *
 * Снапшот типобезопасно повторяет контракт
 * `@shared/hive-city-map` и не должен принимать аргументов из
 * недоверенных источников: это чистая функция с фиксированным сидом.
 *
 * Импортирует только `@shared/hive-city-map` — модуль остаётся чистым
 * рендер-кодом без Node и Electron.
 */

import {
  HIVE_CITY_MAP_SCHEMA_VERSION,
  type CityBuilding,
  type CityBuildingKind,
  type CityDistrict,
  type CityDistrictKind,
  type CityEdge,
  type CityEdgeKind,
  type CityIsland,
  type CityEcosystem,
  type HiveCityMapSnapshot
} from '@shared/hive-city-map'
import type { UiIconName } from './UiIcon'

/** Детерминированный PRNG (mulberry32), чтобы застройка не прыгала между перерисовками. */
function mulberry32(seed: number): () => number {
  let state = seed >>> 0
  return () => {
    state = (state + 0x6d2b79f5) >>> 0
    let t = state
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4_294_967_296
  }
}

function hash32(text: string): number {
  let hash = 0x811c9dc5
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index)
    hash = Math.imul(hash, 0x01000193) >>> 0
  }
  return hash >>> 0
}

interface BuildingPlan {
  readonly name: string
  readonly path: string
  readonly kind: CityBuildingKind
  /** Приблизительный вес высоты (coverage-based), показываем реалистично разными. */
  readonly weight: number
  readonly files: number
  readonly lines: number
  readonly language: string
}

interface DistrictPlan {
  readonly id: string
  readonly label: string
  readonly kind: CityDistrictKind
  readonly buildings: readonly BuildingPlan[]
}

function rustDistrict(
  id: string,
  label: string,
  kind: CityDistrictKind,
  buildings: readonly BuildingPlan[]
): DistrictPlan {
  return { id, label, kind, buildings }
}

function building(
  name: string,
  path: string,
  kind: CityBuildingKind,
  files: number,
  lines: number,
  weight: number,
  language = 'rust'
): BuildingPlan {
  return { name, path, kind, files, lines, weight, language }
}

/** Аберрация EvoHime: контур столицы, близкий к реальному репозиторию. */
const CAPITAL_DISTRICTS: readonly DistrictPlan[] = [
  rustDistrict('district-core', 'Core', 'core', [
    building('Eva runtime', 'crates/evohime-core', 'citadel', 96, 24_800, 940),
    building('Sensitive guardrails', 'crates/evohime-core/sensitive', 'service', 8, 1_400, 260),
    building('Tool registry', 'crates/evohime-core/tools', 'service', 22, 5_300, 560)
  ]),
  rustDistrict('district-runtime', 'Runtime', 'runtime', [
    building('Session runtime', 'crates/evohime-runtime', 'service', 31, 7_600, 720),
    building('Sandbox', 'crates/evohime-sandbox', 'service', 14, 3_100, 480),
    building('Approvals', 'crates/evohime-approvals', 'library', 9, 1_900, 320)
  ]),
  rustDistrict('district-memory', 'Memory', 'memory', [
    building('Local storage', 'crates/evohime-local-storage', 'storage', 27, 6_900, 640),
    building('Event journal', 'crates/evohime-events', 'storage', 12, 2_600, 380)
  ]),
  rustDistrict('district-tools', 'Tools', 'tools', [
    building('Model gateway', 'crates/model-gateway', 'gateway', 44, 11_400, 880),
    building('Provider router', 'crates/lite-router', 'gateway', 19, 4_300, 540),
    building('Hive daemon', 'crates/evohime-hive', 'worker', 38, 9_900, 820),
    building('Supervisor', 'crates/evohime-supervisor', 'worker', 21, 5_100, 600)
  ]),
  rustDistrict('district-shell', 'Shell', 'shell', [
    building('Desktop IPC', 'crates/desktop-ipc', 'service', 16, 3_700, 460),
    building('Electron main', 'desktop/evohime-electron/main', 'ui', 26, 6_400, 560),
    building('Preload bridge', 'desktop/evohime-electron/preload', 'ui', 7, 1_500, 280)
  ]),
  rustDistrict('district-ui', 'UI', 'app', [
    building('Renderer bundle', 'desktop/evohime-electron/renderer', 'ui', 58, 16_200, 900),
    building('Hive city scene', 'desktop/evohime-electron/renderer/3d', 'ui', 12, 3_900, 520),
    building('Updater surface', 'desktop/evohime-electron/updater', 'ui', 9, 1_800, 300)
  ]),
  rustDistrict('district-docs', 'Docs', 'docs', [
    building('Architecture', 'docs/architecture', 'library', 34, 8_800, 420),
    building('Plans', 'docs/plans', 'library', 61, 12_400, 460)
  ])
]

interface SatellitePlan {
  readonly id: string
  readonly name: string
  readonly ecosystem: CityEcosystem
  readonly districts: readonly DistrictPlan[]
}

const SATELLITE_PLANS: readonly SatellitePlan[] = [
  {
    id: 'sat-c0de910a2b4f8e11',
    name: 'CodexGUI',
    ecosystem: 'dotnet',
    districts: [
      rustDistrict('sat-codexgui-app', 'App', 'core', [
        building('AppServer', 'AppServer', 'gateway', 6, 1_400, 620, 'csharp'),
        building('Application', 'Application', 'service', 2, 520, 420, 'csharp'),
        building('Automations', 'Automations', 'worker', 2, 480, 380, 'csharp')
      ]),
      rustDistrict('sat-codexgui-ui', 'UI', 'app', [
        building('UI', 'UI', 'ui', 33, 6_800, 820, 'csharp'),
        building('Views', 'Views', 'ui', 1, 340, 320, 'csharp')
      ]),
      rustDistrict('sat-codexgui-infra', 'Infra', 'tools', [
        building('Infrastructure', 'Infrastructure', 'service', 8, 1_900, 560, 'csharp'),
        building('Integrations', 'Integrations', 'gateway', 4, 960, 440, 'csharp'),
        building('Git', 'Git', 'service', 3, 720, 400, 'csharp'),
        building('Scripts', 'Scripts', 'worker', 6, 840, 340, 'powershell'),
        building('Tests', 'Tests', 'library', 24, 3_600, 500, 'csharp')
      ])
    ]
  },
  {
    id: 'sat-7a18e42d90cf3b22',
    name: 'MemPulse',
    ecosystem: 'dotnet',
    districts: [
      rustDistrict('sat-mempulse-core', 'App', 'core', [
        building('MemPulse', 'src/MemPulse', 'worker', 21, 4_200, 680, 'csharp'),
        building('MemPulse.Linux', 'src/MemPulse.Linux', 'service', 6, 1_100, 380, 'csharp')
      ]),
      rustDistrict('sat-mempulse-infra', 'Infra', 'tools', [
        building('Installer', 'installer', 'service', 1, 240, 260, 'iss'),
        building('Scripts', 'scripts', 'worker', 2, 310, 280, 'powershell'),
        building('Tests', 'tests', 'library', 12, 2_100, 440, 'csharp')
      ])
    ]
  },
  {
    id: 'sat-ea3f9c12d4b67a05',
    name: 'assistant-tune',
    ecosystem: 'python',
    districts: [
      rustDistrict('sat1-core', 'Pipelines', 'runtime', [
        building('Trainer', 'training/loop.py', 'service', 18, 2_400, 520, 'python'),
        building('Eval suite', 'training/eval.py', 'library', 9, 1_100, 300, 'python')
      ]),
      rustDistrict('sat1-data', 'Data', 'memory', [
        building('Dataset registry', 'data/registry.py', 'storage', 6, 900, 240, 'python')
      ])
    ]
  },
  {
    id: 'sat-5c02f8ab91e34d77',
    name: 'docs-site',
    ecosystem: 'node',
    districts: [
      rustDistrict('sat2-site', 'Site', 'app', [
        building('Landing', 'site/index.tsx', 'ui', 14, 1_900, 380, 'typescript'),
        building('Docs router', 'site/router.ts', 'service', 8, 1_200, 260, 'typescript')
      ])
    ]
  },
  {
    id: 'sat-b14d2039fe6a8c21',
    name: 'bench-runner',
    ecosystem: 'rust',
    districts: [
      rustDistrict('sat3-bench', 'Benchmarks', 'tools', [
        building('Throughput bench', 'benches/throughput.rs', 'worker', 11, 1_600, 420),
        building('Latency bench', 'benches/latency.rs', 'worker', 7, 1_000, 300)
      ])
    ]
  }
]

function planIsland(
  id: string,
  name: string,
  role: CityIsland['role'],
  ecosystem: CityEcosystem,
  districtsPlan: readonly DistrictPlan[],
  rng: () => number
): CityIsland {
  const districts: CityDistrict[] = districtsPlan.map((plan) => ({
    id: plan.id,
    label: plan.label,
    kind: plan.kind
  }))
  const buildings: CityBuilding[] = []
  const edges: CityEdge[] = []
  let totalFiles = 0
  let totalLines = 0
  for (const districtPlan of districtsPlan) {
    for (const plan of districtPlan.buildings) {
      const id32 = hash32(`${id}:${plan.path}`)
      const idHex = id32.toString(16).padStart(8, '0')
      const fileId = hash32(`${id}:${plan.path}:file`)
      buildings.push({
        id: `b-${idHex}-${(fileId % 0xffff).toString(16).padStart(4, '0')}`,
        name: plan.name,
        district_id: districtPlan.id,
        kind: plan.kind,
        relative_path: plan.path,
        file_count: plan.files,
        line_count: plan.lines,
        height_weight_bp: plan.weight,
        language: plan.language
      })
      totalFiles += plan.files
      totalLines += plan.lines
    }
  }
  // Зависимые рёбра внутри района: каждое здание ссылается на предыдущее.
  const edgeKinds: readonly CityEdgeKind[] = ['dependency', 'ipc', 'runtime']
  for (const districtPlan of districtsPlan) {
    const districtBuildings = buildings.filter(
      (item) => item.district_id === districtPlan.id
    )
    for (let index = 1; index < districtBuildings.length; index += 1) {
      const from = districtBuildings[index - 1]
      const to = districtBuildings[index]
      if (!from || !to) continue
      edges.push({
        from_id: from.id,
        to_id: to.id,
        kind: edgeKinds[index % edgeKinds.length] ?? 'dependency',
        weight: Math.max(1, Math.round(4 + rng() * 30))
      })
    }
  }
  return {
    id,
    name,
    role,
    source: 'live_workspace',
    status: 'ready',
    reason: null,
    ecosystem,
    structure_hash: hash32(`${id}:structure`).toString(16).padStart(8, '0'),
    revision_hash: hash32(`${id}:rev`).toString(16).padStart(8, '0'),
    accent_hue: role === 'capital' ? 42 : 218 + Math.floor(rng() * 100) % 140,
    districts,
    buildings,
    edges,
    total_files: totalFiles,
    total_lines: totalLines
  }
}

export interface HiveStudioSimulationOptions {
  /** Отпечаток времени снапшота (симулирует `generated_at_ms`). */
  readonly generatedAtMs?: number | undefined
  /** Сколько спутников показать (0..=5): удобно проверять появление/исчезновение. */
  readonly satelliteCount?: number | undefined
  /** Соль ведёт к другой раскладке при неизменном контракте. */
  readonly seed?: number | undefined
}

/**
 * Детерминированный демо-снапшот столицы EvoHime и её спутников.
 * Проходит `parseHiveCityMapSnapshot` без потерь — покрыто тестом.
 */
export function buildHiveStudioSimulation(
  options: HiveStudioSimulationOptions = {}
): HiveCityMapSnapshot {
  const generatedAtMs = options.generatedAtMs ?? Date.now()
  const satelliteCount = Math.min(SATELLITE_PLANS.length, Math.max(0, options.satelliteCount ?? 2))
  const rng = mulberry32(options.seed ?? 199_327)

  const capital = planIsland(
    'capital-evohime',
    'EvoHime',
    'capital',
    'rust',
    CAPITAL_DISTRICTS,
    rng
  )

  const satellites: CityIsland[] = []
  for (let index = 0; index < satelliteCount; index += 1) {
    const plan = SATELLITE_PLANS[index]
    if (!plan) break
    satellites.push(
      planIsland(plan.id, plan.name, 'satellite', plan.ecosystem, plan.districts, rng)
    )
  }

  const revisionHash = hash32(
    `${capital.revision_hash}:${satellites.map((item) => item.revision_hash).join(',')}:${generatedAtMs}`
  )
    .toString(16)
    .padStart(8, '0')

  return {
    schema_version: HIVE_CITY_MAP_SCHEMA_VERSION,
    generated_at_ms: generatedAtMs,
    revision_hash: revisionHash.repeat(4).slice(0, 32),
    capital,
    satellites
  }
}

export interface HiveStudioDepartmentState {
  readonly key: string
  readonly label: string
  readonly subtitle: string
  readonly icon: UiIconName
  readonly activeCount: number
  readonly maxCapacity: number
}

/**
 * Департаменты, поднятые со скриншота реального дашборда; анимация активности
 * (`step`) имитирует смену занятости, чтобы можно было смотреть оживление домов.
 */
export const HIVE_STUDIO_DEPARTMENTS: readonly HiveStudioDepartmentState[] = [
  { key: 'analytics', label: 'Аналитика', subtitle: 'Анализ данных', icon: 'activity', activeCount: 3, maxCapacity: 3 },
  { key: 'planning', label: 'Планирование', subtitle: 'Планирование задач', icon: 'compass', activeCount: 2, maxCapacity: 2 },
  { key: 'research', label: 'Исследования', subtitle: 'Поиск решений', icon: 'target', activeCount: 2, maxCapacity: 2 },
  { key: 'development', label: 'Разработка', subtitle: 'Разработка кода', icon: 'terminal', activeCount: 3, maxCapacity: 3 },
  { key: 'testing', label: 'Тестирование', subtitle: 'Проверка качества', icon: 'check-circle', activeCount: 2, maxCapacity: 2 },
  { key: 'integrations', label: 'Интеграции', subtitle: 'Инфраструктура и API', icon: 'link', activeCount: 2, maxCapacity: 2 },
  { key: 'documentation', label: 'Документация', subtitle: 'База знаний и доки', icon: 'folder', activeCount: 2, maxCapacity: 2 },
  { key: 'support', label: 'Поддержка', subtitle: 'Помощь и сервис', icon: 'message', activeCount: 2, maxCapacity: 2 }
]
