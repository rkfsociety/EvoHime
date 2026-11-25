/**
 * Typed projection of the Hive city-archipelago map served by
 * `evohime-hive-daemon` over the observation control pipe (plan 199.2/199.3).
 *
 * This module mirrors the serde contract of
 * `crates/evohime-hive/src/city_map.rs` (fields stay snake_case, enums are
 * `rename_all = "snake_case"`) and must stay free of Node and Electron
 * imports: it is compiled into the sandboxed renderer bundle.
 */

export const HIVE_CITY_MAP_SCHEMA_VERSION = 1 as const

/** Ceiling of one incoming daemon frame: strictly above the 256 KiB
 * serialization ceiling and strictly below the 4 MiB Core frame limit. */
export const MAX_CITY_MAP_FRAME_BYTES = 512 * 1024

export type CityIslandRole = 'capital' | 'satellite'

export type CityIslandSource = 'live_workspace' | 'embedded_manifest'

export type CityIslandStatus = 'ready' | 'unavailable'

export type CityIslandUnavailableReason =
  | 'repo_not_found'
  | 'repo_unreadable'
  | 'permission_denied'
  | 'invalid_manifest'
  | 'scan_timeout'

export type CityEcosystem =
  | 'rust'
  | 'node'
  | 'python'
  | 'go'
  | 'dotnet'
  | 'mixed'
  | 'generic'

export type CityDistrictKind =
  | 'core'
  | 'shell'
  | 'memory'
  | 'tools'
  | 'voice'
  | 'runtime'
  | 'app'
  | 'packages'
  | 'docs'
  | 'other'

export type CityBuildingKind =
  | 'citadel'
  | 'service'
  | 'library'
  | 'ui'
  | 'storage'
  | 'gateway'
  | 'worker'
  | 'module'

export type CityEdgeKind = 'dependency' | 'ipc' | 'runtime'

export interface CityDistrict {
  readonly id: string
  readonly label: string
  readonly kind: CityDistrictKind
}

export interface CityBuilding {
  /** Normalized relative POSIX path of the module (`crates/evohime-hive`). */
  readonly id: string
  readonly name: string
  readonly district_id: string
  readonly kind: CityBuildingKind
  readonly relative_path: string
  readonly file_count: number
  readonly line_count: number
  /** Integer building height in basis points (`150..=1000`). */
  readonly height_weight_bp: number
  readonly language: string
}

export interface CityEdge {
  readonly from_id: string
  readonly to_id: string
  readonly kind: CityEdgeKind
  /** Saturated visual weight (`1..=100`). */
  readonly weight: number
}

export interface CityIsland {
  /** Stable public id (`capital-evohime` or `sat-<16 hex>`). */
  readonly id: string
  readonly name: string
  readonly role: CityIslandRole
  readonly source: CityIslandSource
  readonly status: CityIslandStatus
  readonly reason: CityIslandUnavailableReason | null
  readonly ecosystem: CityEcosystem
  readonly structure_hash: string
  readonly revision_hash: string
  /** Deterministic accent hue in degrees `0..=359`. */
  readonly accent_hue: number
  readonly districts: readonly CityDistrict[]
  readonly buildings: readonly CityBuilding[]
  readonly edges: readonly CityEdge[]
  readonly total_files: number
  readonly total_lines: number
}

export interface HiveCityMapSnapshot {
  readonly schema_version: number
  readonly generated_at_ms: number
  /** Deterministic snapshot revision hash (32 lowercase hex chars). */
  readonly revision_hash: string
  readonly capital: CityIsland
  readonly satellites: readonly CityIsland[]
}

/**
 * Typed result of one `hive.getCityMap` request. `available: false` covers
 * every degradation: the daemon is not running, the pipe timed out, the frame
 * was malformed, or an older daemon answered the canonical
 * `invalid_request` error frame.
 */
export type HiveCityMapOutcome =
  | { readonly available: true; readonly snapshot: HiveCityMapSnapshot }
  | { readonly available: false; readonly snapshot: null }

const CITY_ISLAND_ROLES: readonly CityIslandRole[] = ['capital', 'satellite']
const CITY_ISLAND_SOURCES: readonly CityIslandSource[] = ['live_workspace', 'embedded_manifest']
const CITY_ISLAND_STATUSES: readonly CityIslandStatus[] = ['ready', 'unavailable']
const CITY_ISLAND_REASONS: readonly CityIslandUnavailableReason[] = [
  'repo_not_found',
  'repo_unreadable',
  'permission_denied',
  'invalid_manifest',
  'scan_timeout'
]
const CITY_ECOSYSTEMS: readonly CityEcosystem[] = [
  'rust',
  'node',
  'python',
  'go',
  'dotnet',
  'mixed',
  'generic'
]
const CITY_DISTRICT_KINDS: readonly CityDistrictKind[] = [
  'core',
  'shell',
  'memory',
  'tools',
  'voice',
  'runtime',
  'app',
  'packages',
  'docs',
  'other'
]
const CITY_BUILDING_KINDS: readonly CityBuildingKind[] = [
  'citadel',
  'service',
  'library',
  'ui',
  'storage',
  'gateway',
  'worker',
  'module'
]
const CITY_EDGE_KINDS: readonly CityEdgeKind[] = ['dependency', 'ipc', 'runtime']

/** Architectural bounding of untrusted payload parts (plan 199.3). */
const MAX_ISLANDS = 13
const MAX_DISTRICTS_PER_ISLAND = 16
const MAX_BUILDINGS_PER_ISLAND = 192
const MAX_EDGES_PER_ISLAND = 512
const MAX_ID_CHARS = 256
const MAX_LABEL_CHARS = 256
const MAX_HASH_CHARS = 64

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function boundedString(value: unknown, maxChars: number): string | null {
  return typeof value === 'string' && value.length <= maxChars ? value : null
}

function enumValue<T extends string>(value: unknown, allowed: readonly T[]): T | null {
  return typeof value === 'string' && (allowed as readonly string[]).includes(value)
    ? (value as T)
    : null
}

function boundedCount(value: unknown, max: number): number | null {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0 && value <= max
    ? value
    : null
}

function parseDistrict(value: unknown): CityDistrict | null {
  if (!isRecord(value)) return null
  const id = boundedString(value['id'], MAX_ID_CHARS)
  const label = boundedString(value['label'], MAX_LABEL_CHARS)
  const kind = enumValue(value['kind'], CITY_DISTRICT_KINDS)
  if (id === null || label === null || kind === null) return null
  return { id, label, kind }
}

function parseBuilding(value: unknown): CityBuilding | null {
  if (!isRecord(value)) return null
  const id = boundedString(value['id'], MAX_ID_CHARS)
  const name = boundedString(value['name'], MAX_LABEL_CHARS)
  const districtId = boundedString(value['district_id'], MAX_ID_CHARS)
  const kind = enumValue(value['kind'], CITY_BUILDING_KINDS)
  const relativePath = boundedString(value['relative_path'], MAX_ID_CHARS)
  const fileCount = boundedCount(value['file_count'], 2 ** 32 - 1)
  const lineCount = boundedCount(value['line_count'], 2 ** 53 - 1)
  const heightWeightBp = boundedCount(value['height_weight_bp'], 65_535)
  const language = boundedString(value['language'], 64)
  if (
    id === null ||
    name === null ||
    districtId === null ||
    kind === null ||
    relativePath === null ||
    fileCount === null ||
    lineCount === null ||
    heightWeightBp === null ||
    language === null
  ) {
    return null
  }
  return {
    id,
    name,
    district_id: districtId,
    kind,
    relative_path: relativePath,
    file_count: fileCount,
    line_count: lineCount,
    height_weight_bp: heightWeightBp,
    language
  }
}

function parseEdge(value: unknown): CityEdge | null {
  if (!isRecord(value)) return null
  const fromId = boundedString(value['from_id'], MAX_ID_CHARS)
  const toId = boundedString(value['to_id'], MAX_ID_CHARS)
  const kind = enumValue(value['kind'], CITY_EDGE_KINDS)
  const weight = boundedCount(value['weight'], 100)
  if (fromId === null || toId === null || kind === null || weight === null) return null
  return { from_id: fromId, to_id: toId, kind, weight }
}

function parseIsland(value: unknown): CityIsland | null {
  if (!isRecord(value)) return null
  const id = boundedString(value['id'], MAX_ID_CHARS)
  const name = boundedString(value['name'], MAX_LABEL_CHARS)
  const role = enumValue(value['role'], CITY_ISLAND_ROLES)
  const source = enumValue(value['source'], CITY_ISLAND_SOURCES)
  const status = enumValue(value['status'], CITY_ISLAND_STATUSES)
  const reasonValue = value['reason']
  const reason =
    reasonValue === undefined || reasonValue === null
      ? null
      : enumValue(reasonValue, CITY_ISLAND_REASONS)
  const ecosystem = enumValue(value['ecosystem'], CITY_ECOSYSTEMS)
  const structureHash = boundedString(value['structure_hash'], MAX_HASH_CHARS)
  const revisionHash = boundedString(value['revision_hash'], MAX_HASH_CHARS)
  const accentHue = boundedCount(value['accent_hue'], 359)
  const totalFiles = boundedCount(value['total_files'], 2 ** 32 - 1)
  const totalLines = boundedCount(value['total_lines'], 2 ** 53 - 1)
  if (
    id === null ||
    name === null ||
    role === null ||
    source === null ||
    status === null ||
    ecosystem === null ||
    structureHash === null ||
    revisionHash === null ||
    accentHue === null ||
    totalFiles === null ||
    totalLines === null
  ) {
    return null
  }
  if (reasonValue !== undefined && reasonValue !== null && reason === null) {
    return null
  }

  const districtsRaw = value['districts']
  const buildingsRaw = value['buildings']
  const edgesRaw = value['edges']
  if (
    !Array.isArray(districtsRaw) ||
    !Array.isArray(buildingsRaw) ||
    !Array.isArray(edgesRaw) ||
    districtsRaw.length > MAX_DISTRICTS_PER_ISLAND ||
    buildingsRaw.length > MAX_BUILDINGS_PER_ISLAND ||
    edgesRaw.length > MAX_EDGES_PER_ISLAND
  ) {
    return null
  }

  const districtIds = new Set<string>()
  const districts: CityDistrict[] = []
  for (const raw of districtsRaw) {
    const district = parseDistrict(raw)
    if (district === null) return null
    districtIds.add(district.id)
    districts.push(district)
  }

  const buildings: CityBuilding[] = []
  const buildingIds = new Set<string>()
  for (const raw of buildingsRaw) {
    const building = parseBuilding(raw)
    if (building === null || !districtIds.has(building.district_id)) return null
    buildingIds.add(building.id)
    buildings.push(building)
  }

  const edges: CityEdge[] = []
  for (const raw of edgesRaw) {
    const edge = parseEdge(raw)
    if (edge === null || !buildingIds.has(edge.from_id) || !buildingIds.has(edge.to_id)) return null
    edges.push(edge)
  }

  return {
    id,
    name,
    role,
    source,
    status,
    reason,
    ecosystem,
    structure_hash: structureHash,
    revision_hash: revisionHash,
    accent_hue: accentHue,
    districts,
    buildings,
    edges,
    total_files: totalFiles,
    total_lines: totalLines
  }
}

/**
 * Strict structural validation of one decoded daemon frame. Returns `null`
 * for every malformed input: the client maps it to `unavailable`.
 */
export function parseHiveCityMapSnapshot(value: unknown): HiveCityMapSnapshot | null {
  if (!isRecord(value)) return null
  if (value['schema_version'] !== HIVE_CITY_MAP_SCHEMA_VERSION) return null
  const generatedAtMs = value['generated_at_ms']
  const revisionHash = boundedString(value['revision_hash'], MAX_HASH_CHARS)
  if (
    typeof generatedAtMs !== 'number' ||
    !Number.isInteger(generatedAtMs) ||
    Math.abs(generatedAtMs) > 8.64e15 ||
    revisionHash === null
  ) {
    return null
  }
  const capital = parseIsland(value['capital'])
  if (capital === null || capital.role !== 'capital') return null
  const satellitesRaw = value['satellites']
  if (!Array.isArray(satellitesRaw) || satellitesRaw.length >= MAX_ISLANDS) return null
  const satellites: CityIsland[] = []
  const islandIds = new Set<string>([capital.id])
  for (const raw of satellitesRaw) {
    const island = parseIsland(raw)
    if (island === null || island.role !== 'satellite' || islandIds.has(island.id)) return null
    islandIds.add(island.id)
    satellites.push(island)
  }
  return {
    schema_version: HIVE_CITY_MAP_SCHEMA_VERSION,
    generated_at_ms: generatedAtMs,
    revision_hash: revisionHash,
    capital,
    satellites
  }
}

 /** FNV-1a (32-bit) of a string; mirrors the Rust side for stable layout seeds. */
export function fnv1a32(text: string): number {
  let hash = 0x811c9dc5
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index)
    hash = Math.imul(hash, 0x01000193) >>> 0
  }
  return hash >>> 0
}

/**
 * Curated display hues for island accents (deterministic, client-side only).
 * The raw `accent_hue` from the daemon is a plain hash bucket (0..359) and can
 * land on muddy yellow-greens that clash with the shell palette; here the
 * capital always wears the amber of the EVA CORE accent, and satellites pick
 * from UI-consistent blue/violet/teal hues.
 */
const DISPLAY_HUES: readonly number[] = [218, 262, 162, 200, 288, 172]

/** Display accent hue of an island; deterministic per island id and role. */
export function displayIslandHue(islandId: string, role: string): number {
  if (role === 'capital') return 42
  return DISPLAY_HUES[fnv1a32(islandId) % DISPLAY_HUES.length] ?? 218
}

export interface CityBuildingView {
  readonly building: CityBuilding
  /** Deterministic slot `(x, z)` inside the island, in island units. */
  readonly x: number
  readonly z: number
  /** Deterministic height in island units (`height_weight_bp / 1000`). */
  readonly height: number
  readonly isEdgeTarget: boolean
}

export interface CityIslandView {
  readonly island: CityIsland
  /** Island centre on the archipelago plane (world units, capital at origin). */
  readonly x: number
  readonly z: number
  /** Bounding radius of the laid-out city (world units). */
  readonly radius: number
  readonly buildings: readonly CityBuildingView[]
  readonly phase: IslandLifecyclePhase
}

export type IslandLifecyclePhase = 'entering' | 'active' | 'exiting'


const DISTRICT_ORDER: readonly string[] = [
  'core',
  'runtime',
  'memory',
  'tools',
  'shell',
  'ui',
  'voice',
  'app',
  'packages',
  'docs',
  'other'
]

function districtOrderIndex(kind: string): number {
  const index = DISTRICT_ORDER.indexOf(kind)
  return index === -1 ? DISTRICT_ORDER.length : index
}

/**
 * Радиус кольца домов работников департаментов в столице. Кольцо живёт между
 * башней Королевы (центр) и кварталами модулей проекта, поэтому радиус слегка
 * растёт с числом лотов, но остаётся внутри [`CAPITAL_TOWER_INNER_RADIUS`].
 */
export function capitalHouseRingRadius(total: number): number {
  return Math.max(4.6, (Math.max(1, total) * 1.52) / (Math.PI * 2) * 1.12)
}

/**
 * Динамический радиус Главного кольцевого проспекта столицы: при росте числа домов
 * агентов кольцо проспекта отодвигается наружу, сохраняя зазор под надписи секторов.
 */
export function capitalMainAvenueRadius(totalHouses: number): number {
  return Math.max(7.2, capitalHouseRingRadius(totalHouses) + 2.32)
}

/**
 * Динамический внутренний радиус кварталов столицы: при росте числа домов
 * агентов кольцо модулей автоматически отодвигается наружу, чтобы город
 * масштабировался без наложений.
 */
export function capitalInnerRadius(totalHouses: number): number {
  return Math.max(CAPITAL_TOWER_INNER_RADIUS, capitalMainAvenueRadius(totalHouses) + 0.4)
}

/**
 * Угол дома департамента на кольце столицы. Распределяет дома равномерно по
 * полному кругу `360°` (при виде сверху слепых зон за шпилем больше нет).
 */
export function capitalHouseAngle(slot: number, total: number): number {
  return Math.PI * 0.5 + ((slot + 0.5) / Math.max(1, total)) * Math.PI * 2
}

/**
 * Внутренний радиус столицы, свободный от кварталов проекта: все модульные
 * башни капитального острова выталкиваются за него, уступая центр башне
 * Королевы и кольцу домов работников.
 */
export const CAPITAL_TOWER_INNER_RADIUS = 7.6
export const SATELLITE_MAIN_AVENUE_RADIUS = 2.6

/**
 * Shared deterministic island renderer model: sorted districts, a compact
 * building grid grouped by district, and seeded jitter. Both the SVG 2D
 * renderer and the WebGL renderer consume this layout, so both modes always
 * draw the same city.
 */
export interface CityLayoutOptions {
  readonly optimizeByQueen?: boolean | undefined
  readonly totalHouses?: number | undefined
}

export interface CityLayoutEfficiency {
  readonly avgEdgeDistance: number
  readonly avgGateDistance: number
}

/**
 * Оптимизирует порядок департаментов для сокращения пробега машин Королевы:
 * наиболее загруженные департаменты размещаются ближе к воротам (углы у ±0 от π/2).
 */
export function optimizeDepartmentOrder<
  T extends { readonly key: string; readonly activeCount: number; readonly maxCapacity: number }
>(departments: readonly T[], optimizeByQueen: boolean): readonly T[] {
  if (!optimizeByQueen) {
    return [...departments].sort((left, right) => left.key.localeCompare(right.key))
  }
  const byLoad = [...departments].sort((left, right) => {
    const loadDiff = right.activeCount - left.activeCount
    if (loadDiff !== 0) return loadDiff
    const ratioDiff =
      right.activeCount / Math.max(1, right.maxCapacity) -
      left.activeCount / Math.max(1, left.maxCapacity)
    if (Math.abs(ratioDiff) > 1e-6) return ratioDiff
    return left.key.localeCompare(right.key)
  })
  // Чередуем размещение у начала (справа от ворот) и у конца (слева от ворот),
  // чтобы самые активные секторы были ближе всего к воротам Королевы.
  const head: T[] = []
  const tail: T[] = []
  byLoad.forEach((item, index) => {
    if (index % 2 === 0) head.push(item)
    else tail.unshift(item)
  })
  return [...head, ...tail]
}

/** Измеряет среднюю длину связей между зданиями и взвешенную по трафику дистанцию до ворот Королевы. */
export function computeCityLayoutEfficiency(view: CityIslandView): CityLayoutEfficiency {
  const byId = new Map<string, CityBuildingView>()
  for (const b of view.buildings) {
    byId.set(b.building.id, b)
  }
  const edgeDegree = new Map<string, number>()
  let edgeSum = 0
  let edgeCount = 0
  for (const edge of view.island.edges) {
    edgeDegree.set(edge.from_id, (edgeDegree.get(edge.from_id) ?? 0) + 1)
    edgeDegree.set(edge.to_id, (edgeDegree.get(edge.to_id) ?? 0) + 1)
    const from = byId.get(edge.from_id)
    const to = byId.get(edge.to_id)
    if (!from || !to) continue
    edgeSum += Math.hypot(to.x - from.x, to.z - from.z)
    edgeCount += 1
  }
  const gateAngle =
    view.island.role === 'capital'
      ? Math.PI / 2
      : view.x !== 0 || view.z !== 0
        ? Math.atan2(-view.z, -view.x)
        : Math.PI / 2
  const gateR = view.island.role === 'capital' ? CAPITAL_TOWER_INNER_RADIUS : view.radius * 0.78
  const gateX = Math.cos(gateAngle) * gateR
  const gateZ = Math.sin(gateAngle) * gateR
  let gateWeightedSum = 0
  let weightSum = 0
  for (const b of view.buildings) {
    if (b.building.kind === 'citadel') continue
    const w = (edgeDegree.get(b.building.id) ?? 0) + 1
    gateWeightedSum += Math.hypot(b.x - gateX, b.z - gateZ) * w
    weightSum += w
  }
  return {
    avgEdgeDistance: edgeCount > 0 ? edgeSum / edgeCount : 0,
    avgGateDistance: weightSum > 0 ? gateWeightedSum / weightSum : 0
  }
}

export function buildCityMapView(
  snapshot: HiveCityMapSnapshot,
  phases: ReadonlyMap<string, IslandLifecyclePhase> | null = null,
  options?: CityLayoutOptions
): { readonly capital: CityIslandView; readonly satellites: readonly CityIslandView[] } {
  const phaseOf = (islandId: string): IslandLifecyclePhase =>
    phases?.get(islandId) ?? 'active'

  const capital = buildIslandView(snapshot.capital, 0, 0, phaseOf(snapshot.capital.id), options)

  const visible = snapshot.satellites.filter(
    (island) => phases === null || phases.get(island.id) !== undefined
  )
  // Сотовая (гексагональная) раскладка городов-спутников вокруг столицы:
  // плоские грани соседних шестиугольников обращены строго друг к другу.
  const hexFaceAngles = [
    Math.PI / 6,
    (5 * Math.PI) / 6,
    -Math.PI / 2,
    -Math.PI / 6,
    (-5 * Math.PI) / 6,
    Math.PI / 2
  ] as const
  const prebuiltSatellites = visible.map((island) =>
    buildIslandView(island, 0, 0, phaseOf(island.id), options)
  )
  const firstRingMaxR = prebuiltSatellites
    .slice(0, 6)
    .reduce((max, item) => Math.max(max, item.radius), 4)
  const hexApothem = Math.cos(Math.PI / 6)
  const ring1Distance = (capital.radius + firstRingMaxR) * hexApothem + 3.8

  const satellites: CityIslandView[] = prebuiltSatellites.map((satView, index) => {
    let x = 0
    let z = 0
    if (index < 6) {
      const angle = hexFaceAngles[index] ?? 0
      x = Math.cos(angle) * ring1Distance
      z = Math.sin(angle) * ring1Distance
    } else {
      const outerSlot = index - 6
      const outerCount = Math.max(1, prebuiltSatellites.length - 6)
      const angle = (2 * Math.PI * outerSlot) / outerCount + Math.PI / 6
      const outerDistance = ring1Distance * 1.78 + satView.radius * hexApothem
      x = Math.cos(angle) * outerDistance
      z = Math.sin(angle) * outerDistance
    }
    return buildIslandView(satView.island, x, z, phaseOf(satView.island.id), options)
  })

  return { capital, satellites }
}

function buildIslandView(
  island: CityIsland,
  x: number,
  z: number,
  phase: IslandLifecyclePhase,
  options?: CityLayoutOptions
): CityIslandView {
  const districtRank = new Map<string, number>()
  island.districts.forEach((district, index) => {
    districtRank.set(district.id, districtOrderIndex(district.kind) * 1000 + index)
  })

  const optimizeByQueen = options?.optimizeByQueen === true
  const edgeDegree = new Map<string, number>()
  if (optimizeByQueen) {
    for (const edge of island.edges) {
      edgeDegree.set(edge.from_id, (edgeDegree.get(edge.from_id) ?? 0) + 1)
      edgeDegree.set(edge.to_id, (edgeDegree.get(edge.to_id) ?? 0) + 1)
    }
  }

  const ordered = [...island.buildings].sort((left, right) => {
    if (optimizeByQueen) {
      const leftScore = (edgeDegree.get(left.id) ?? 0) * 500 + left.height_weight_bp
      const rightScore = (edgeDegree.get(right.id) ?? 0) * 500 + right.height_weight_bp
      if (rightScore !== leftScore) return rightScore - leftScore
    }
    const leftRank = districtRank.get(left.district_id) ?? 99_000
    const rightRank = districtRank.get(right.district_id) ?? 99_000
    return leftRank - rightRank || (left.id < right.id ? -1 : left.id > right.id ? 1 : 0)
  })

  // Цитадель(и) Королевы занимают точный центр города, вне сетки кварталов.
  const citadels = ordered.filter((item) => item.kind === 'citadel')
  const quarter = ordered.filter((item) => item.kind !== 'citadel')

  const buildings: CityBuildingView[] = []
  citadels.forEach((building, index) => {
    // Первая цитадель рисуется башней Королевы (QueenTower3D) и обязана стоять
    // в точном центре города, соосно кольцам, воротам и спиц-сетке: все круги
    // города рисуются от центра острова, поэтому и башня должна быть в нём.
    // Данные часто дают две цитадели (evohime-core и evohime-hive): остальные
    // образуют компактный дворец вплотную к центру (сами башни не рисуются).
    const angle = (2 * Math.PI * index) / Math.max(1, citadels.length)
    const offset = index === 0 ? 0 : 0.8
    buildings.push({
      building,
      x: Math.cos(angle) * offset,
      z: Math.sin(angle) * offset,
      height: Math.max(0.15, building.height_weight_bp / 1000),
      isEdgeTarget: false
    })
  })

  const isCapital = island.role === 'capital'
  const innerRadius = isCapital ? capitalInnerRadius(options?.totalHouses ?? 0) : CAPITAL_TOWER_INNER_RADIUS
  /** Радиус самого внешнего кольца радиальной застройки города. */
  let outerRingRadius = isCapital ? innerRadius + 0.8 : SATELLITE_MAIN_AVENUE_RADIUS
  const gateAngle = isCapital
    ? Math.PI * 0.5
    : x !== 0 || z !== 0
      ? Math.atan2(-z, -x)
      : Math.PI * 0.5

  if (quarter.length > 0) {
    // Единая радиально-кольцевая застройка Королевы для всех городов:
    // кольца кварталов вокруг центра города со свободным проездом от главных ворот / магистрали.
    // Углы зданий задаются общей сеткой спиц для всех колец: каждое кольцо
    // занимает спицы с постоянным шагом (stride), поэтому столбцы кварталов
    // сходятся в прямые лучи и город читается выровненным, а не смятой сеткой.
    const ringStart = isCapital ? innerRadius + 1.2 : SATELLITE_MAIN_AVENUE_RADIUS + 1.6
    const ringStep = 3.2
    const spokesTotal = 32
    const spokeArc = (2 * Math.PI) / spokesTotal
    // Ворота: свободный коридор по центру въезда. Сетка сдвинута на полслота
    // от направления ворот, поэтому коридор просто выкалывает по два спица-
    // слота с каждой стороны, а спицы никогда не попадают на кардинальные
    // коннекторы развязок (RING_CONNECTOR_ANGLES кратны четвертям окружности,
    // тогда как спицы сдвинуты на полслота).
    const gateHalfGap = Math.min(
      Math.PI * 0.42,
      Math.max((isCapital ? 1.7 : 1.5) / Math.max(1, ringStart), 2 * spokeArc)
    )
    const freeSpokes: number[] = []
    for (let t = 0; t < spokesTotal; t += 1) {
      // Угол спицы относительно ворот; сетка сдвинута на полслота.
      const spokeOffset = ((t + 0.5) / spokesTotal) * 2 * Math.PI
      const distanceFromGate = Math.min(spokeOffset, 2 * Math.PI - spokeOffset)
      if (distanceFromGate >= gateHalfGap) freeSpokes.push(t)
    }
    let ringRadius = ringStart
    let offset = 0
    while (offset < quarter.length) {
      const stride = Math.max(
        1,
        Math.ceil(((isCapital ? 2.8 : 2.6) * spokesTotal) / (2 * Math.PI * ringRadius))
      )
      let countOnRing = Math.max(
        isCapital ? 8 : 4,
        Math.floor(freeSpokes.length / stride),
        1
      )
      countOnRing = Math.min(countOnRing, quarter.length - offset)
      const slice = quarter.slice(offset, offset + countOnRing)
      // В режиме оптимизации Королевы самые приоритетные здания ставятся симметрично
      // ближе всего к прямому въездному проспекту, оставляя проезд свободным.
      const orderedRingSlice = optimizeByQueen
        ? (() => {
            const arranged: CityBuilding[] = new Array(countOnRing)
            slice.forEach((item, idx) => {
              const targetIdx = idx % 2 === 0 ? idx / 2 : countOnRing - 1 - Math.floor(idx / 2)
              arranged[targetIdx] = item
            })
            return arranged
          })()
        : slice
      // Блок спиц кольца строится с постоянным шагом `stride` и центрируется
      // на дуге напротив ворот — соседние кольца с тем же stride занимают
      // те же спицы, поэтому застройка сходится в лучи.
      const usedSlots = countOnRing * stride
      const startSlot = Math.floor((freeSpokes.length - Math.min(usedSlots, freeSpokes.length)) / 2)
      for (let i = 0; i < countOnRing; i += 1) {
        const building = orderedRingSlice[i]
        if (!building) continue
        const spokeT = freeSpokes[startSlot + i * stride]
        if (spokeT === undefined) continue
        const angle = gateAngle + ((spokeT + 0.5) / spokesTotal) * 2 * Math.PI
        buildings.push({
          building,
          x: Math.cos(angle) * ringRadius,
          z: Math.sin(angle) * ringRadius,
          height: Math.max(0.15, building.height_weight_bp / 1000),
          isEdgeTarget: false
        })
      }
      offset += countOnRing
      if (offset < quarter.length) {
        ringRadius += ringStep
      }
    }
    outerRingRadius = ringRadius
  }

  // Шестиугольная плита вписана в окружность радиуса R, её апофема равна R * cos(30°).
  // Умножаем на 1 / cos(30°), чтобы крайние башни стояли на платформе с запасом.
  const hexApothemFactor = 1 / Math.cos(Math.PI / 6)
  const layoutRadius = Math.max(
    isCapital ? 4.8 : 6.2,
    (outerRingRadius + (isCapital ? 3.0 : 2.4)) * hexApothemFactor
  )
  const radius = layoutRadius

  return { island, x, z, radius, buildings, phase }
}

export const CITY_MAP_MAX_SATELLITES = 12

// ---------------------------------------------------------------------------
// План 200: Сотовая архитектура Улья, конвейер Королевы и 2D-симуляция
// ---------------------------------------------------------------------------

/** Фиксированная вместимость одного сектора 1-го кольца (ровно 3 агента). */
export const HONEYCOMB_SECTOR_CAPACITY = 3 as const
export const HONEYCOMB_MIN_CITADEL_EXITS = 1 as const
export const HONEYCOMB_MAX_CITADEL_EXITS = 6 as const

export type HoneycombAgentRole =
  | 'executor'
  | 'analyst'
  | 'strategist'
  | 'decision_coordinator'

export type HoneycombCargoType =
  | 'task'
  | 'report'
  | 'analysis'
  | 'decision'
  | 'comms_request'
  | 'comms_approved'

export interface HoneycombHexCoord {
  readonly q: number
  readonly r: number
}

export interface HoneycombAgent {
  readonly id: string
  readonly name: string
  readonly role: HoneycombAgentRole
  readonly sectorId: string
  /** Тема/направление, в рамках которого агент хранит контекст и переиспользуется. */
  readonly topic: string
  readonly busy: boolean
  readonly activeTripId: string | null
}

export interface HoneycombSector {
  readonly id: string
  readonly cityId: string
  readonly coord: HoneycombHexCoord
  readonly ring: 1
  readonly label: string
  /** Специализация сектора: в одном секторе живут только агенты одинаковой роли. */
  readonly role: HoneycombAgentRole
  readonly capacity: typeof HONEYCOMB_SECTOR_CAPACITY
  readonly agents: readonly HoneycombAgent[]
}

export interface HoneycombModuleNode {
  readonly id: string
  readonly cityId: string
  readonly coord: HoneycombHexCoord
  /** Кольцо >= 2: структура изучаемого проекта. */
  readonly ring: number
  readonly name: string
  readonly kind: CityBuildingKind
  readonly relativePath: string
  readonly fileCount: number
  readonly lineCount: number
  readonly language: string
}

export interface HoneycombEdge {
  readonly id: string
  readonly cityId: string
  readonly fromId: string
  readonly toId: string
  readonly kind: CityEdgeKind
  readonly active: boolean
}

export interface HoneycombIntercityLink {
  readonly id: string
  readonly fromCityId: string
  readonly toCityId: string
  /** Внешние (последние) кольца, через которые соединяются города. */
  readonly fromRing: number
  readonly toRing: number
}

export interface HoneycombVehicleDispatch {
  readonly id: string
  readonly cityId: string
  readonly agentId: string
  readonly role: HoneycombAgentRole
  readonly cargo: HoneycombCargoType
  readonly fromNodeId: string
  readonly toNodeId: string
  /** Если задано — после заезда к Королеве за разрешением машина едет к целевому агенту. */
  readonly finalTargetNodeId?: string | undefined
  readonly startedAtMs: number
}

export type HoneycombMutationEvent =
  | { readonly type: 'sector.create'; readonly cityId: string; readonly sector: HoneycombSector }
  | { readonly type: 'sector.remove'; readonly cityId: string; readonly sectorId: string }
  | { readonly type: 'agent.spawn'; readonly cityId: string; readonly sectorId: string; readonly agent: HoneycombAgent }
  | { readonly type: 'agent.retire'; readonly cityId: string; readonly agentId: string }
  | { readonly type: 'building.construct'; readonly cityId: string; readonly node: HoneycombModuleNode }
  | { readonly type: 'building.remodel'; readonly cityId: string; readonly node: HoneycombModuleNode }
  | { readonly type: 'building.demolish'; readonly cityId: string; readonly nodeId: string }
  | { readonly type: 'edge.link'; readonly cityId: string; readonly edge: HoneycombEdge }
  | { readonly type: 'edge.unlink'; readonly cityId: string; readonly edgeId: string }
  | { readonly type: 'comms.authorize'; readonly cityId: string; readonly fromAgentId: string; readonly toAgentId: string }
  | { readonly type: 'vehicle.dispatch'; readonly dispatch: HoneycombVehicleDispatch }

export interface HoneycombCity {
  readonly id: string
  readonly projectName: string
  /** Основной город Королевы (исходники Евы, либо первый добавленный проект). */
  readonly isPrimary: boolean
  /** Единственная Цитадель Королевы на всей карте — true только у основного города. */
  readonly hasQueenCitadel: boolean
  /** От 1 до 6 выходов строго перпендикулярно граням внешней соты Цитадели. */
  readonly citadelExitsCount: number
  /** Координаты центра города на общей 2D-карте (мировые единицы). */
  readonly worldCenterX: number
  readonly worldCenterY: number
  /** Наибольшее внешнее кольцо города (используется для межгородских дорог). */
  readonly maxRing: number
  readonly sectors: readonly HoneycombSector[]
  readonly modules: readonly HoneycombModuleNode[]
  readonly edges: readonly HoneycombEdge[]
}

export interface HoneycombWorldState {
  readonly schemaVersion: 2
  readonly paused: boolean
  readonly queenWorking: boolean
  readonly primaryCityId: string
  readonly cities: readonly HoneycombCity[]
  readonly intercityLinks: readonly HoneycombIntercityLink[]
  readonly activeDispatches: readonly HoneycombVehicleDispatch[]
  readonly revision: number
}

export type HoneycombWorldOutcome =
  | { readonly available: true; readonly world: HoneycombWorldState }
  | { readonly available: false; readonly world: null }

export interface HoneycombWorldDelta {
  readonly schemaVersion: 2
  readonly fromRevision: number
  readonly toRevision: number
  readonly events: readonly HoneycombMutationEvent[]
  readonly requiresFullResync: boolean
  readonly paused?: boolean
  readonly queenWorking?: boolean
}

export type HoneycombWorldDeltaOutcome =
  | { readonly available: true; readonly delta: HoneycombWorldDelta }
  | { readonly available: false; readonly delta: null }

export function parseHoneycombWorldState(raw: unknown): HoneycombWorldState | null {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return null
  const obj = raw as Record<string, unknown>
  if (obj['schemaVersion'] !== 2) return null
  if (typeof obj['paused'] !== 'boolean') return null
  if (typeof obj['queenWorking'] !== 'boolean') return null
  if (typeof obj['primaryCityId'] !== 'string') return null
  if (!Array.isArray(obj['cities'])) return null
  if (!Array.isArray(obj['intercityLinks'])) return null
  if (!Array.isArray(obj['activeDispatches'])) return null
  if (typeof obj['revision'] !== 'number' || !Number.isFinite(obj['revision'])) return null
  return raw as HoneycombWorldState
}

export function parseHoneycombWorldDelta(raw: unknown): HoneycombWorldDelta | null {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return null
  const obj = raw as Record<string, unknown>
  if (obj['schemaVersion'] !== 2) return null
  if (typeof obj['fromRevision'] !== 'number' || !Number.isFinite(obj['fromRevision'])) return null
  if (typeof obj['toRevision'] !== 'number' || !Number.isFinite(obj['toRevision'])) return null
  if (!Array.isArray(obj['events'])) return null
  if (typeof obj['requiresFullResync'] !== 'boolean') return null
  return raw as unknown as HoneycombWorldDelta
}

/** Применяет инкрементальную дельту мутаций Королевы к текущему состоянию 2D-мира. */
export function applyHoneycombDelta(
  world: HoneycombWorldState,
  delta: HoneycombWorldDelta
): HoneycombWorldState {
  if (
    delta.events.length === 0
    && delta.toRevision === world.revision
    && (delta.paused === undefined || delta.paused === world.paused)
    && (delta.queenWorking === undefined || delta.queenWorking === world.queenWorking)
  ) {
    return world
  }

  let cities = world.cities.map((c) => ({
    ...c,
    sectors: [...c.sectors],
    modules: [...c.modules],
    edges: [...c.edges]
  }))
  const activeDispatches = [...world.activeDispatches]
  let sawVehicleDispatch = false

  for (const event of delta.events) {
    switch (event.type) {
      case 'sector.create': {
        const city = cities.find((c) => c.id === event.cityId)
        if (city && !city.sectors.some((s) => s.id === event.sector.id)) {
          city.sectors.push(event.sector)
          city.maxRing = Math.max(city.maxRing, 1)
        }
        break
      }
      case 'sector.remove': {
        const city = cities.find((c) => c.id === event.cityId)
        if (city) {
          city.sectors = city.sectors.filter((s) => s.id !== event.sectorId)
        }
        break
      }
      case 'agent.spawn': {
        const city = cities.find((c) => c.id === event.cityId)
        const sectorIdx = city?.sectors.findIndex((s) => s.id === event.sectorId) ?? -1
        if (city && sectorIdx >= 0) {
          const target = city.sectors[sectorIdx]!
          if (!target.agents.some((a) => a.id === event.agent.id)) {
            city.sectors[sectorIdx] = {
              ...target,
              agents: [...target.agents, event.agent]
            }
          }
        }
        break
      }
      case 'agent.retire': {
        const city = cities.find((c) => c.id === event.cityId)
        if (city) {
          city.sectors = city.sectors.map((s) => ({
            ...s,
            agents: s.agents.filter((a) => a.id !== event.agentId)
          }))
        }
        break
      }
      case 'building.construct': {
        const city = cities.find((c) => c.id === event.cityId)
        if (city && !city.modules.some((m) => m.id === event.node.id)) {
          city.modules.push(event.node)
          city.maxRing = Math.max(city.maxRing, event.node.ring)
        }
        break
      }
      case 'building.remodel': {
        const city = cities.find((c) => c.id === event.cityId)
        if (city) {
          city.modules = city.modules.map((m) => (m.id === event.node.id ? event.node : m))
        }
        break
      }
      case 'building.demolish': {
        const city = cities.find((c) => c.id === event.cityId)
        if (city) {
          city.modules = city.modules.filter((m) => m.id !== event.nodeId)
        }
        break
      }
      case 'edge.link': {
        const city = cities.find((c) => c.id === event.cityId)
        if (city && !city.edges.some((e) => e.id === event.edge.id)) {
          city.edges.push(event.edge)
        }
        break
      }
      case 'edge.unlink': {
        const city = cities.find((c) => c.id === event.cityId)
        if (city) {
          city.edges = city.edges.filter((e) => e.id !== event.edgeId)
        }
        break
      }
      case 'vehicle.dispatch': {
        sawVehicleDispatch = true
        if (!activeDispatches.some((d) => d.id === event.dispatch.id)) {
          activeDispatches.push(event.dispatch)
        }
        break
      }
      case 'comms.authorize':
        break
    }
  }

  const nextPaused = delta.paused !== undefined
    ? delta.paused
    : sawVehicleDispatch
      ? false
      : world.paused
  const nextQueenWorking = delta.queenWorking !== undefined
    ? delta.queenWorking
    : sawVehicleDispatch
      ? true
      : world.queenWorking
  const nextDispatches = nextPaused ? [] : activeDispatches

  return {
    ...world,
    paused: nextPaused,
    queenWorking: nextQueenWorking,
    cities,
    activeDispatches: nextDispatches,
    revision: delta.toRevision
  }
}
