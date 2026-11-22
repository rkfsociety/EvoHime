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
 * Deterministic pseudo-random in `[0, 1)` with an extra integer salt stream.
 */
function mixSeed(seed: string, salt: number): number {
  // FNV-1a over `seed` with the salt folded in as a little-endian byte.
  const mixed = fnv1a32(`${seed}#${(salt >>> 0).toString(36)}`)
  return ((mixed >>> 8) & 0xffffff) / 16_777_216
}

const BUILDING_CELL = 2.4
const BUILDING_MAX_COLUMNS = 8

/**
 * Shared deterministic island renderer model: sorted districts, a compact
 * building grid grouped by district, and seeded jitter. Both the SVG 2D
 * renderer and the WebGL renderer consume this layout, so both modes always
 * draw the same city.
 */
export function buildCityMapView(
  snapshot: HiveCityMapSnapshot,
  phases: ReadonlyMap<string, IslandLifecyclePhase> | null = null
): { readonly capital: CityIslandView; readonly satellites: readonly CityIslandView[] } {
  const phaseOf = (islandId: string): IslandLifecyclePhase =>
    phases?.get(islandId) ?? 'active'

  const capital = buildIslandView(snapshot.capital, 0, 0, phaseOf(snapshot.capital.id))

  const visible = snapshot.satellites.filter(
    (island) => phases === null || phases.get(island.id) !== undefined
  )
  const satellites: CityIslandView[] = visible.map((island, index) => {
    const ring = index < 6 ? 0 : 1
    const slot = index < 6 ? index : index - 6
    const countInRing = ring === 0 ? Math.min(visible.length, 6) : visible.length - 6
    const angle =
      (2 * Math.PI * slot) / Math.max(1, countInRing) + ring * (Math.PI / 6)
    const distance = ring === 0
      ? 24
      : 24 + islandRadiusEstimate(snapshot, index) + 14
    const x = Math.cos(angle) * distance
    const z = Math.sin(angle) * distance
    return buildIslandView(island, x, z, phaseOf(island.id))
  })

  return { capital, satellites }
}

/** Conservative outer radius of a satellite from its building count. */
function islandRadiusEstimate(snapshot: HiveCityMapSnapshot, satelliteIndex: number): number {
  const island = snapshot.satellites[satelliteIndex]
  if (!island) return 6
  return Math.min(9, Math.max(4, 2.2 * Math.sqrt(island.buildings.length)))
}

function buildIslandView(
  island: CityIsland,
  x: number,
  z: number,
  phase: IslandLifecyclePhase
): CityIslandView {
  const districtRank = new Map<string, number>()
  island.districts.forEach((district, index) => {
    districtRank.set(district.id, districtOrderIndex(district.kind) * 1000 + index)
  })

  const ordered = [...island.buildings].sort((left, right) => {
    const leftRank = districtRank.get(left.district_id) ?? 99_000
    const rightRank = districtRank.get(right.district_id) ?? 99_000
    return leftRank - rightRank || (left.id < right.id ? -1 : left.id > right.id ? 1 : 0)
  })

  const columns = Math.max(1, Math.min(BUILDING_MAX_COLUMNS, Math.ceil(Math.sqrt(ordered.length))))
  const rows = Math.ceil(ordered.length / columns)

  const buildings = ordered.map((building, index) => {
    const column = index % columns
    const row = Math.floor(index / columns)
    const seed = `${island.id}/${building.id}`
    const jitterX = (mixSeed(seed, 1) - 0.5) * 0.9
    const jitterZ = (mixSeed(seed, 2) - 0.5) * 0.9
    const height = Math.max(0.15, building.height_weight_bp / 1000)
    return {
      building,
      x: (column - (columns - 1) / 2) * BUILDING_CELL + jitterX,
      z: (row - (rows - 1) / 2) * BUILDING_CELL * 0.8 + jitterZ,
      height,
      isEdgeTarget: false
    }
  })

  const layoutRadius = Math.max(
    3.6,
    2.2 * Math.sqrt(Math.max(1, ordered.length)),
    (Math.max(columns, rows) * BUILDING_CELL) / 2 + 1.2
  )
  const radius = Math.min(10, layoutRadius)

  return { island, x, z, radius, buildings, phase }
}

export const CITY_MAP_MAX_SATELLITES = 12
