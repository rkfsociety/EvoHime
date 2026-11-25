/**
 * Генератор сценария города Улья для dev-стенда Hive Studio (План 200).
 *
 * Позволяет в реальном времени (с HMR) проигрывать:
 * - нулевой старт (только Цитадель Королевы);
 * - рост секторов с лимитом 3 агента на сектор;
 * - эстафету ролей и меж-агентное общение через Королеву;
 * - мультипроектность на одной карте (соединение по внешним кольцам);
 * - перестройку города и режим паузы (парковка машин в домах).
 */

import {
  HONEYCOMB_SECTOR_CAPACITY,
  type HoneycombAgent,
  type HoneycombAgentRole,
  type HoneycombCargoType,
  type HoneycombCity,
  type HoneycombEdge,
  type HoneycombIntercityLink,
  type HoneycombModuleNode,
  type HoneycombSector,
  type HoneycombVehicleDispatch,
  type HoneycombWorldState
} from '@shared/hive-city-map'
import { getCityOuterHexRadius, getCityRingExpansion } from './components/hive/honeycomb-geometry'

export type StudioGrowthStage = 'zero' | 'scouting' | 'pipeline' | 'remodeled'

export interface HoneycombStudioOptions {
  readonly stage: StudioGrowthStage
  readonly paused: boolean
  readonly evaSourceConfigured: boolean
  readonly projectCount: number
  readonly agentCount: number
  readonly exitsOverride: 'auto' | 1 | 2 | 3 | 4 | 5 | 6
  readonly remodelSeed: number
}

const ROLE_SEQUENCE: readonly HoneycombAgentRole[] = [
  'executor',
  'analyst',
  'strategist',
  'decision_coordinator'
]

const AGENT_TOPICS: readonly string[] = [
  'Индексация модулей',
  'Анализ зависимостей',
  'План рефакторинга',
  'Координация релизов',
  'Контракт IPC',
  'Хранилище SQLite'
]

const CARGO_BY_ROLE: Record<HoneycombAgentRole, HoneycombCargoType> = {
  executor: 'task',
  analyst: 'report',
  strategist: 'analysis',
  decision_coordinator: 'decision'
}

const EXTRA_PROJECT_NAMES: readonly string[] = ['CodexGUI', 'MemPulse', 'PCPulse']

const SECTOR_LABEL_BY_ROLE: Record<HoneycombAgentRole, string> = {
  executor: 'Исполнители',
  analyst: 'Аналитики',
  strategist: 'Стратеги',
  decision_coordinator: 'Координаторы'
}

const EXTRA_UNIQUE_SECTOR_SPECS: readonly { readonly label: string; readonly role: HoneycombAgentRole }[] = [
  { label: 'Разведчики', role: 'executor' },
  { label: 'Ревизоры', role: 'analyst' },
  { label: 'Архитекторы', role: 'strategist' },
  { label: 'Интеграторы', role: 'decision_coordinator' }
]

function buildModulesForProject(
  cityId: string,
  stage: StudioGrowthStage,
  remodelSeed: number,
  isPrimary: boolean
): { readonly modules: readonly HoneycombModuleNode[]; readonly edges: readonly HoneycombEdge[]; readonly maxRing: number } {
  if (stage === 'zero') {
    return { modules: [], edges: [], maxRing: 0 }
  }

  const baseModules: readonly { name: string; path: string; ring: number; kind: HoneycombModuleNode['kind']; files: number }[] =
    isPrimary
      ? [
          { name: 'evohime-core', path: 'crates/evohime-core', ring: 2, kind: 'citadel', files: 96 },
          { name: 'evohime-hive', path: 'crates/evohime-hive', ring: 2, kind: 'worker', files: 38 },
          { name: 'local-storage', path: 'crates/evohime-local-storage', ring: 2, kind: 'storage', files: 27 },
          { name: 'desktop-ipc', path: 'crates/desktop-ipc', ring: 2, kind: 'gateway', files: 16 },
          { name: 'electron-ui', path: 'desktop/evohime-electron', ring: 3, kind: 'ui', files: 58 },
          { name: 'model-gateway', path: 'crates/model-gateway', ring: 3, kind: 'gateway', files: 44 },
          { name: 'supervisor', path: 'crates/evohime-supervisor', ring: 3, kind: 'service', files: 21 }
        ]
      : [
          { name: 'app-core', path: 'src/core', ring: 2, kind: 'service', files: 22 },
          { name: 'api-bridge', path: 'src/bridge', ring: 2, kind: 'gateway', files: 14 },
          { name: 'ui-views', path: 'src/ui', ring: 3, kind: 'ui', files: 19 }
        ]

  const limit = stage === 'scouting' ? Math.min(3, baseModules.length) : baseModules.length
  const shift = stage === 'remodeled' ? (remodelSeed % 5) + 1 : 0

  const modules: HoneycombModuleNode[] = []
  for (let i = 0; i < limit; i += 1) {
    const spec = baseModules[(i + shift) % baseModules.length]!
    const ring = stage === 'remodeled' && i % 2 === 1 && spec.ring === 2 ? 3 : spec.ring
    modules.push({
      id: `${cityId}:mod:${i}`,
      cityId,
      coord: { q: i + shift, r: ring },
      ring,
      name: spec.name,
      kind: spec.kind,
      relativePath: spec.path,
      fileCount: spec.files,
      lineCount: spec.files * 220,
      language: isPrimary ? 'rust' : 'typescript'
    })
  }

  const edges: HoneycombEdge[] = []
  for (let i = 1; i < modules.length; i += 1) {
    edges.push({
      id: `${cityId}:edge:${i}`,
      cityId,
      fromId: modules[i - 1]!.id,
      toId: modules[i]!.id,
      kind: i % 2 === 0 ? 'ipc' : 'dependency',
      active: stage === 'pipeline' || stage === 'remodeled'
    })
  }

  const maxRing = modules.reduce((acc, item) => Math.max(acc, item.ring), 1)
  return { modules, edges, maxRing }
}

export function buildHoneycombStudioWorld(options: HoneycombStudioOptions): HoneycombWorldState {
  const projectCount = Math.max(1, Math.min(4, options.projectCount))
  const effectiveAgentCount =
    options.stage === 'zero'
      ? 0
      : options.stage === 'scouting'
        ? Math.min(3, Math.max(1, options.agentCount))
        : Math.max(1, options.agentCount)

  const autoExits = Math.max(1, Math.min(6, Math.ceil(effectiveAgentCount / 2)))
  const citadelExitsCount = options.exitsOverride === 'auto' ? autoExits : options.exitsOverride

  const primaryProjectName = options.evaSourceConfigured
    ? 'EvoHime'
    : (EXTRA_PROJECT_NAMES[0] ?? 'Project-1')
  const primaryCityId = 'city-primary'

  // Создаём сектора с лимитом 3 агента на сектор, одной ролью и уникальным названием (без #1/#2).
  const sectorSlots: { label: string; role: HoneycombAgentRole; indices: number[] }[] = [
    ...ROLE_SEQUENCE.map((role) => ({ label: SECTOR_LABEL_BY_ROLE[role], role, indices: [] as number[] })),
    ...EXTRA_UNIQUE_SECTOR_SPECS.map((spec) => ({ label: spec.label, role: spec.role, indices: [] as number[] }))
  ]

  for (let i = 0; i < effectiveAgentCount; i += 1) {
    const tier = Math.floor(i / (ROLE_SEQUENCE.length * HONEYCOMB_SECTOR_CAPACITY))
    const roleIdx = i % ROLE_SEQUENCE.length
    const targetIdx = Math.min(sectorSlots.length - 1, tier * ROLE_SEQUENCE.length + roleIdx)
    const target = sectorSlots[targetIdx]
    if (target && target.indices.length < HONEYCOMB_SECTOR_CAPACITY) {
      target.indices.push(i)
    }
  }

  const sectors: HoneycombSector[] = []
  const allAgents: HoneycombAgent[] = []
  let sectorCounter = 0

  for (const spec of sectorSlots) {
    if (spec.indices.length === 0) continue
    const sectorId = `${primaryCityId}:sector:${sectorCounter}`
    const agentsInSector: HoneycombAgent[] = spec.indices.map((globalIdx) => {
      const agent: HoneycombAgent = {
        id: `agent-${globalIdx + 1}`,
        name: `Агент #${globalIdx + 1}`,
        role: spec.role,
        sectorId,
        topic: AGENT_TOPICS[globalIdx % AGENT_TOPICS.length]!,
        busy: !options.paused,
        activeTripId: options.paused ? null : `trip-${globalIdx + 1}`
      }
      allAgents.push(agent)
      return agent
    })

    sectors.push({
      id: sectorId,
      cityId: primaryCityId,
      coord: { q: sectorCounter, r: 1 },
      ring: 1,
      label: spec.label,
      role: spec.role,
      capacity: HONEYCOMB_SECTOR_CAPACITY,
      agents: agentsInSector
    })
    sectorCounter += 1
  }

  const primaryLayout = buildModulesForProject(
    primaryCityId,
    options.stage,
    options.remodelSeed,
    true
  )
  const primaryMaxRing = Math.max(
    sectors.length > 0 ? 1 : 0,
    primaryLayout.maxRing
  )

  const primaryCityDraft: HoneycombCity = {
    id: primaryCityId,
    projectName: primaryProjectName,
    isPrimary: true,
    hasQueenCitadel: true,
    citadelExitsCount,
    worldCenterX: 0,
    worldCenterY: 0,
    maxRing: primaryMaxRing,
    sectors,
    modules: primaryLayout.modules,
    edges: primaryLayout.edges
  }

  const cities: HoneycombCity[] = [primaryCityDraft]

  const intercityLinks: HoneycombIntercityLink[] = []
  const secondaryNames = options.evaSourceConfigured
    ? EXTRA_PROJECT_NAMES
    : EXTRA_PROJECT_NAMES.slice(1)

  const primaryOuterR = getCityOuterHexRadius(primaryMaxRing, getCityRingExpansion(primaryCityDraft))
  const extLayouts = Array.from({ length: Math.max(0, projectCount - 1) }, (_, i) => {
    const cityId = `city-ext-${i + 1}`
    const extLayout = buildModulesForProject(cityId, options.stage, options.remodelSeed + i + 1, false)
    const extMaxRing = Math.max(1, extLayout.maxRing)
    return { cityId, extLayout, extMaxRing, outerR: getCityOuterHexRadius(extMaxRing) }
  })
  const maxExtOuterR = extLayouts.reduce((acc, item) => Math.max(acc, item.outerR), 0)
  const gapPx = 180
  const minDistFromPrimary = primaryOuterR + maxExtOuterR + gapPx
  const extCount = extLayouts.length
  const minDistBetweenExts =
    extCount >= 2 ? (maxExtOuterR * 2 + gapPx) / (2 * Math.sin(Math.PI / extCount)) : 0
  const distance = Math.max(minDistFromPrimary, minDistBetweenExts)

  for (let idx = 1; idx < projectCount; idx += 1) {
    const item = extLayouts[idx - 1]!
    const angle = ((idx - 1) * Math.PI * 2) / Math.max(1, projectCount - 1)
    cities.push({
      id: item.cityId,
      projectName: secondaryNames[idx - 1] ?? `Project-${idx}`,
      isPrimary: false,
      hasQueenCitadel: false,
      citadelExitsCount: 2,
      worldCenterX: Math.cos(angle) * distance,
      worldCenterY: Math.sin(angle) * distance,
      maxRing: item.extMaxRing,
      sectors: [],
      modules: item.extLayout.modules,
      edges: item.extLayout.edges
    })
    if (primaryMaxRing >= 1) {
      intercityLinks.push({
        id: `link-primary-${item.cityId}`,
        fromCityId: primaryCityId,
        toCityId: item.cityId,
        fromRing: primaryMaxRing,
        toRing: item.extMaxRing
      })
    }
  }

  // Генерируем реальные рейсы конвейера (при паузе список пуст — машины в домах).
  const activeDispatches: HoneycombVehicleDispatch[] = []
  if (!options.paused && options.stage !== 'zero') {
    allAgents.forEach((agent, idx) => {
      const targetModules = cities.flatMap((c) => c.modules)
      const moduleTarget = targetModules[idx % Math.max(1, targetModules.length)]
      const queenHubId = `${primaryCityId}:citadel`
      const peerAgent = allAgents[(idx + 1) % allAgents.length]

      // Каждый 4-й агент демонстрирует заезд к Королеве за разрешением на меж-агентное общение.
      if (
        options.stage === 'pipeline' &&
        idx % 4 === 3 &&
        peerAgent &&
        peerAgent.sectorId !== agent.sectorId
      ) {
        activeDispatches.push({
          id: `trip-${idx + 1}`,
          cityId: primaryCityId,
          agentId: agent.id,
          role: agent.role,
          cargo: 'comms_request',
          fromNodeId: agent.sectorId,
          toNodeId: queenHubId,
          finalTargetNodeId: peerAgent.sectorId,
          startedAtMs: 1_000 + idx
        })
      } else {
        activeDispatches.push({
          id: `trip-${idx + 1}`,
          cityId: primaryCityId,
          agentId: agent.id,
          role: agent.role,
          cargo: CARGO_BY_ROLE[agent.role],
          fromNodeId: queenHubId,
          toNodeId: moduleTarget ? moduleTarget.id : agent.sectorId,
          startedAtMs: 1_000 + idx
        })
      }
    })
  }

  return {
    schemaVersion: 2,
    paused: options.paused,
    queenWorking: !options.paused && options.stage !== 'zero',
    primaryCityId,
    cities,
    intercityLinks,
    activeDispatches,
    revision: options.remodelSeed + options.agentCount * 10 + projectCount * 100
  }
}
