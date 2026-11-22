//! Read-only architecture scanner behind the Hive city-archipelago map
//! (plan 199.1).
//!
//! The module owns the versioned [`HiveCityMapSnapshot`] contract and a pure
//! read-only scanner that turns a repository checkout into a [`CityIsland`]:
//! districts, buildings, and dependency edges derived from bounded manifest
//! parsing (`Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`,
//! `.sln`/`.csproj`) plus a generic top-level fallback. The scanner never runs
//! external processes (`cargo`, `node`, `git` are not invoked) and never writes
//! to the file system.
//!
//! Security posture:
//! - canonicalized repository root, symlink entries are skipped (checked via
//!   [`std::fs::symlink_metadata`] directly before descent/open); the local
//!   TOCTOU race of the file system is accepted, kernel-level atomic
//!   `O_NOFOLLOW` is not claimed;
//! - Windows path normalization to POSIX view (`\` → `/`, `\\?\` prefix
//!   stripped, no `..` components);
//! - absolute OS paths never leak into `relative_path`, `name`, `label`, `id`
//!   or `reason`; read errors map only to [`CityIslandUnavailableReason`];
//! - hard limits on depth, entries per directory, files, manifests, and
//!   payload size, with deterministic truncation (see [`fit_snapshot_to_payload`]).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

/// Version of the [`HiveCityMapSnapshot`] schema. Any field addition, removal,
/// or semantic change requires incrementing this constant.
pub const HIVE_CITY_MAP_SCHEMA_VERSION: u32 = 1;

/// Hard ceiling of the serialized snapshot JSON produced inside
/// `evohime-hive`. Strictly below the Electron client limit (`512 KiB`) and
/// the transport limit (`MAX_FRAME_BYTES = 4 MiB`).
pub const MAX_CITY_MAP_PAYLOAD_BYTES: usize = 256 * 1024;

/// Maximum number of satellite islands in one snapshot.
pub const MAX_SATELLITES: usize = 12;

/// Maximum number of buildings inside one island.
pub const MAX_BUILDINGS_PER_ISLAND: usize = 48;

/// Maximum number of edges inside one island.
pub const MAX_EDGES_PER_ISLAND: usize = 128;

/// Maximum directory traversal depth (relative to the repository root).
pub const MAX_SCAN_DEPTH: usize = 6;

/// Maximum number of directory entries considered per directory; the rest is
/// ignored deterministically (entries are sorted lexicographically first).
pub const MAX_ENTRIES_PER_DIR: usize = 512;

/// Maximum number of regular files counted per repository scan.
pub const MAX_FILES_PER_REPO: usize = 4_000;

/// Maximum number of bytes read from one manifest file.
pub const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// Maximum number of bytes read from one source text file for line counting.
pub const MAX_SOURCE_FILE_BYTES: u64 = 512 * 1024;

/// Scan budget for a single repository (the capital included).
pub const MAX_REPO_SCAN_DURATION: Duration = Duration::from_millis(1500);

/// Total scan budget for the whole archipelago build.
pub const MAX_TOTAL_SCAN_DURATION: Duration = Duration::from_millis(5000);

/// Fixed public identifier of the capital island.
pub const CAPITAL_ISLAND_ID: &str = "capital-evohime";

/// Directory names skipped on any depth.
const SKIPPED_DIR_NAMES: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".evohime",
    "dist",
    "out",
    "build",
    ".venv",
    "__pycache__",
];

fn is_skipped_dir_name(name: &str, dotnet_context: bool, within_src: bool) -> bool {
    if SKIPPED_DIR_NAMES.contains(&name) {
        return true;
    }
    // Dot-directories (including `.evohime-cargo`, `.evohime-native`, `.vibe`,
    // `.github`) are local tooling state, never architecture source trees.
    if name.starts_with('.') {
        return true;
    }
    // `bin`/`obj` are skipped only for .NET projects and never inside `src/`,
    // so Rust `src/bin/daemon.rs` stays visible.
    if (name == "bin" || name == "obj") && dotnet_context && !within_src {
        return true;
    }
    false
}

/// Extensions treated as definitely binary for line counting.
const BINARY_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "ico", "icns", "bmp", "exe", "dll", "so", "dylib", "lib",
    "a", "obj", "bin", "zip", "7z", "gz", "tar", "br", "xz", "whl", "jar", "class", "woff",
    "woff2", "ttf", "otf", "eot", "mp3", "mp4", "wav", "ogg", "webm", "mov", "avi", "pdf", "onnx",
    "bin.dat", "wasm", "pdb", "rlib", "rmeta", "d", "node", "snappkg",
];

/// Top-level source directory names recognized by the generic fallback.
const FALLBACK_SOURCE_DIR_NAMES: &[&str] =
    &["src", "app", "lib", "services", "packages", "apps", "cmd", "internal", "server", "client"];

/// Upper bound applied to every free-form string field of the schema.
const MAX_SCHEMA_STRING_CHARS: usize = 128;

/// Deterministic FNV-1a 128-bit hash (no external crypto crates).
pub fn fnv1a_128(bytes: &[u8]) -> u128 {
    // Standard FNV-1a 128-bit offset basis.
    const OFFSET_BASIS: u128 = 0x6c62272e_07bb0142_62b82175_6295c58d;
    // Standard FNV-1a 128-bit prime.
    const PRIME: u128 = 0x0000_0100_0000_0000_0000_0000_0000_013b;
    let mut hash = OFFSET_BASIS;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

fn fnv_hex(value: u128) -> String {
    format!("{value:032x}")
}

fn fnv_hex_short(value: u128, chars: usize) -> String {
    let full = fnv_hex(value);
    full.chars().take(chars).collect()
}

/// Normalizes an OS path into the canonical POSIX-like string used for island
/// identifiers: `\` → `/`, `\\?\` prefix stripped, trailing separators removed,
/// no `..` components, backslash-only drives kept. On Windows the canonical
/// string is lowercased (case-insensitive file system).
pub fn canonical_path_string(raw: &str) -> String {
    let mut value = raw.trim().replace('\\', "/");
    if let Some(stripped) = value.strip_prefix("\\\\?\\") {
        // replace('\\') turned the marker into "//?"
        value = stripped.to_owned();
    }
    if let Some(stripped) = value.strip_prefix("//?/") {
        value = stripped.to_owned();
    }
    while value.len() > 1 && value.ends_with('/') {
        value.truncate(value.len() - 1);
    }
    // Collapse repeated separators.
    let mut collapsed = String::with_capacity(value.len());
    let mut previous_was_slash = false;
    for ch in value.chars() {
        if ch == '/' {
            if !previous_was_slash {
                collapsed.push('/');
            }
            previous_was_slash = true;
        } else {
            collapsed.push(ch);
            previous_was_slash = false;
        }
    }
    if cfg!(windows) {
        collapsed.to_ascii_lowercase()
    } else {
        collapsed
    }
}

fn sanitize_schema_string(raw: &str) -> String {
    let mut value: String = raw
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .take(MAX_SCHEMA_STRING_CHARS)
        .collect();
    value = value.trim().to_owned();
    if value.is_empty() {
        value = "unnamed".to_owned();
    }
    value
}

fn join_relative(parent: &str, component: &std::ffi::OsStr) -> String {
    let component = component.to_string_lossy();
    let component = component.replace('\\', "/");
    let component = component.replace("..", "_");
    if parent.is_empty() {
        component
    } else {
        format!("{parent}/{component}")
    }
}

fn extension_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => extension.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Counts text lines in a file bounded by [`MAX_SOURCE_FILE_BYTES`]; binary
/// files (by extension or NUL byte in the first chunk) are skipped.
fn count_file_lines(path: &Path, extension: &str) -> io::Result<Option<u64>> {
    if BINARY_EXTENSIONS.contains(&extension) {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE_FILE_BYTES {
        return Ok(None);
    }
    let bytes = fs::read(path)?;
    if bytes[..bytes.len().min(8192)].contains(&0u8) {
        return Ok(None);
    }
    let lines =
        u64::try_from(bytes.iter().filter(|byte| **byte == b'\n').count()).unwrap_or(u64::MAX);
    Ok(if !bytes.is_empty() && bytes.last().is_some_and(|last| *last != b'\n') {
        Some(lines + 1)
    } else {
        Some(lines)
    })
}

// ---------------------------------------------------------------------------
// Contract types
// ---------------------------------------------------------------------------

/// Role of one island inside the archipelago.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CityIslandRole {
    /// The EvoHime capital in the center of the archipelago.
    Capital,
    /// A satellite city for one watched repository.
    Satellite,
}

/// Provenance of an island structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CityIslandSource {
    /// Live read-only scan of the configured checkout.
    LiveWorkspace,
    /// Built-in canonical manifest of the EvoHime architecture.
    EmbeddedManifest,
}

/// Availability of an island structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CityIslandStatus {
    /// Structure was built from a scan or manifest.
    Ready,
    /// Structure is temporarily unavailable; see `reason`.
    Unavailable,
}

/// Diagnostic reason for an [`CityIslandStatus::Unavailable`] island.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CityIslandUnavailableReason {
    /// Repository path does not exist.
    RepoNotFound,
    /// Repository exists but could not be read.
    RepoUnreadable,
    /// Access to the repository or a required file was denied.
    PermissionDenied,
    /// A manifest exists but cannot be parsed.
    InvalidManifest,
    /// Scan budget (`MAX_REPO_SCAN_DURATION`) was exhausted.
    ScanTimeout,
}

/// Detected technology ecosystem of a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CityEcosystem {
    /// Rust workspace or package.
    Rust,
    /// Node / TypeScript (including Electron).
    Node,
    /// Python project (`pyproject.toml`).
    Python,
    /// Go module (`go.mod`).
    Go,
    /// .NET solution or project.
    Dotnet,
    /// Several ecosystems detected at once (the EvoHime checkout).
    Mixed,
    /// No recognized manifest; generic top-level grouping.
    Generic,
}

/// Functional district of a module inside an island.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CityDistrictKind {
    /// Core runtime crates.
    Core,
    /// Electron shell and UI surfaces.
    Shell,
    /// Storage and memory layers.
    Memory,
    /// Tool catalogs and tool runtime.
    Tools,
    /// Voice and listener subsystems.
    Voice,
    /// Process lifecycle, IPC and supervision.
    Runtime,
    /// User-facing applications and CLIs.
    App,
    /// Shared packages.
    Packages,
    /// Documentation trees.
    Docs,
    /// Anything not covered by the named districts.
    Other,
}

/// Kind of a building (module) inside an island.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CityBuildingKind {
    /// The central citadel (`evohime-core`, `evohime-hive`).
    Citadel,
    /// Runnable service or command-line entry point.
    Service,
    /// Library crate or package.
    Library,
    /// UI module (Electron renderer / shell).
    Ui,
    /// Storage backend.
    Storage,
    /// Transport or gateway layer (desktop IPC, model gateway).
    Gateway,
    /// Long-running worker (supervisor, updater, listener).
    Worker,
    /// Secondary module without a named role.
    Module,
}

/// Kind of an edge between two buildings of the same island.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CityEdgeKind {
    /// Compile-time or package dependency.
    Dependency,
    /// IPC transport relation (desktop IPC, control pipes).
    Ipc,
    /// Runtime supervision relation.
    Runtime,
}

/// One city district inside an island.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CityDistrict {
    /// Stable district identifier (the district kind name).
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Functional kind of the district.
    pub kind: CityDistrictKind,
}

/// One building (module) inside an island.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CityBuilding {
    /// Normalized relative POSIX path of the module inside the repository
    /// (for example `"crates/evohime-hive"`). Stable across background
    /// rescaffolding, so inspector selection survives updates.
    pub id: String,
    /// Display name of the module.
    pub name: String,
    /// Identifier of the parent [`CityDistrict`].
    pub district_id: String,
    /// Building kind.
    pub kind: CityBuildingKind,
    /// Normalized relative POSIX path of the module root.
    pub relative_path: String,
    /// Number of source files counted inside the module.
    pub file_count: u32,
    /// Approximate number of text lines counted inside the module.
    pub line_count: u64,
    /// Integer building height in basis points (`150..=1000`, `1000` = 1.00).
    pub height_weight_bp: u16,
    /// Primary language of the module (`"rust"`, `"typescript"`, ...).
    pub language: String,
}

/// One dependency/IPC edge between two buildings of the same island.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CityEdge {
    /// Identifier of the source building; must exist in the same island.
    pub from_id: String,
    /// Identifier of the target building; must exist in the same island and
    /// differ from `from_id` (self-edges are forbidden).
    pub to_id: String,
    /// Edge kind.
    pub kind: CityEdgeKind,
    /// Saturated visual weight (`1..=100`).
    pub weight: u16,
}

/// One island: the capital or a satellite city of the archipelago.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CityIsland {
    /// Stable public identifier (`"capital-evohime"` or `"sat-<16 hex>"`).
    pub id: String,
    /// Display name of the island.
    pub name: String,
    /// Island role in the archipelago.
    pub role: CityIslandRole,
    /// Provenance of the structure.
    pub source: CityIslandSource,
    /// Availability status.
    pub status: CityIslandStatus,
    /// Diagnostic reason when `status` is `unavailable`.
    pub reason: Option<CityIslandUnavailableReason>,
    /// Detected ecosystem of the repository.
    pub ecosystem: CityEcosystem,
    /// Deterministic hash of the structural content (32 lowercase hex chars).
    pub structure_hash: String,
    /// Canonical revision hash: structure plus volatile metrics (file and line
    /// counts); excludes `generated_at_ms`.
    pub revision_hash: String,
    /// Deterministic accent hue in degrees `0..=359`.
    pub accent_hue: u16,
    /// Districts of the city, canonically sorted by `id`.
    pub districts: Vec<CityDistrict>,
    /// Buildings of the city, canonically sorted by `(district_id, id)`.
    pub buildings: Vec<CityBuilding>,
    /// Edges of the city, canonically sorted by `(from_id, to_id, kind)`.
    pub edges: Vec<CityEdge>,
    /// Total number of counted source files.
    pub total_files: u32,
    /// Approximate total number of counted text lines.
    pub total_lines: u64,
}

/// Versioned snapshot of the whole archipelago served over
/// `HiveObservationControlRequest::GetCityMap`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveCityMapSnapshot {
    /// Schema version; see [`HIVE_CITY_MAP_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Wall-clock generation time in Unix milliseconds; volatile field,
    /// excluded from `revision_hash`.
    pub generated_at_ms: i64,
    /// Deterministic snapshot revision hash (32 lowercase hex chars).
    pub revision_hash: String,
    /// The EvoHime capital island.
    pub capital: CityIsland,
    /// Satellite islands, canonically sorted by `(id, name)`.
    pub satellites: Vec<CityIsland>,
}

/// Validation error of the city-map contract.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CityMapError {
    /// A field value is outside its documented range.
    #[error("city map value out of range: {0}")]
    ValueOutOfRange(&'static str),
    /// An edge references a missing building or itself.
    #[error("city map edge references missing building: {0}")]
    EdgeReference(String),
    /// A contract string exceeds its bounded length.
    #[error("city map string is unbounded: {0}")]
    UnboundedString(&'static str),
}

/// Validates one island against the documented field ranges and edge rules.
pub fn validate_island(island: &CityIsland) -> Result<(), CityMapError> {
    if island.accent_hue > 360 {
        return Err(CityMapError::ValueOutOfRange("accent_hue"));
    }
    for district in &island.districts {
        if district.id.is_empty() || district.id.chars().count() > MAX_SCHEMA_STRING_CHARS {
            return Err(CityMapError::UnboundedString("district.id"));
        }
    }
    for building in &island.buildings {
        if building.id.contains("..") || building.id.chars().count() > MAX_SCHEMA_STRING_CHARS {
            return Err(CityMapError::UnboundedString("building.id"));
        }
        if building.height_weight_bp < 150 || building.height_weight_bp > 1000 {
            return Err(CityMapError::ValueOutOfRange("height_weight_bp"));
        }
        if !island.districts.iter().any(|d| d.id == building.district_id) {
            return Err(CityMapError::EdgeReference(building.id.clone()));
        }
    }
    for edge in &island.edges {
        if edge.weight == 0 || edge.weight > 100 {
            return Err(CityMapError::ValueOutOfRange("edge.weight"));
        }
        if edge.from_id == edge.to_id {
            return Err(CityMapError::EdgeReference(edge.from_id.clone()));
        }
        let known = |id: &str| island.buildings.iter().any(|b| b.id == id);
        if !known(&edge.from_id) || !known(&edge.to_id) {
            return Err(CityMapError::EdgeReference(edge.from_id.clone()));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Canonical assembly, hashes and saturation
// ---------------------------------------------------------------------------

fn saturate_weight(raw: u64, base: u16) -> u16 {
    let encoded = raw.min(u64::from(u16::MAX - 1));
    let base = u64::from(base);
    u16::try_from(base.saturating_add(encoded % 50)).unwrap_or(100).clamp(1, 100)
}

fn dedupe_edges(edges: Vec<CityEdge>) -> Vec<CityEdge> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::with_capacity(edges.len());
    for mut edge in edges {
        if edge.from_id == edge.to_id {
            continue;
        }
        edge.weight = edge.weight.clamp(1, 100);
        let key = (edge.from_id.clone(), edge.to_id.clone(), edge.kind);
        if seen.insert(key) {
            unique.push(edge);
        }
    }
    unique
}

fn structure_hash_content(island_id: &str, role: CityIslandRole) -> String {
    format!("schema={};island={};role={:?}", HIVE_CITY_MAP_SCHEMA_VERSION, island_id, role)
}

/// Sorts collections canonically, saturates weights, normalizes heights and
/// computes `structure_hash`. Buildings/edges must already be complete. This
/// is the single source of determinism for island projection.
fn finalize_island(island: &mut CityIsland) {
    island.districts.sort_by(|a, b| a.id.cmp(&b.id));
    island.districts.dedup_by(|a, b| a.id == b.id);
    island.buildings.sort_by(|a, b| (a.district_id.as_str(), a.id.as_str()).cmp(&(b.district_id.as_str(), b.id.as_str())));
    island.edges = dedupe_edges(std::mem::take(&mut island.edges));
    island.edges.sort_by(|a, b| {
        (a.from_id.as_str(), a.to_id.as_str(), a.kind).cmp(&(b.from_id.as_str(), b.to_id.as_str(), b.kind))
    });
    // Heights: integer basis points relative to the largest building.
    let max_lines = island
        .buildings
        .iter()
        .map(|b| b.line_count)
        .max()
        .unwrap_or(1)
        .max(1);
    for building in &mut island.buildings {
        let scaled = building.line_count.saturating_mul(850) / max_lines;
        let normalized = u32::try_from(scaled).unwrap_or(850).min(850);
        building.height_weight_bp = u16::try_from(150 + normalized).unwrap_or(1000).min(1000);
    }
    let mut content = structure_hash_content(&island.id, island.role);
    for district in &island.districts {
        content.push_str(&format!(";d:{}:{:?}", district.id, district.kind));
    }
    for building in &island.buildings {
        content.push_str(&format!(
            ";b:{}:{}:{}",
            building.id,
            building.district_id,
            building.kind as u8
        ));
    }
    for edge in &island.edges {
        content.push_str(&format!(";e:{}>{}:{:?}", edge.from_id, edge.to_id, edge.kind));
    }
    island.structure_hash = fnv_hex(fnv1a_128(content.as_bytes()));
    island.accent_hue = u16::try_from(fnv1a_128(island.id.as_bytes()) % 360).unwrap_or(0);
}

/// Computes `revision_hash` after the structural content including volatile
/// metric fields (file/line counts). `generated_at_ms` is never included.
fn compute_revision_hash(island: &CityIsland) -> String {
    let mut content = format!("rev={};structure={};files={};lines={}", island.structure_hash, island.status as u8, island.total_files, island.total_lines);
    for building in &island.buildings {
        content.push_str(&format!(";bm:{}:{}:{}", building.id, building.file_count, building.line_count));
    }
    fnv_hex(fnv1a_128(content.as_bytes()))
}

fn compute_snapshot_revision_hash(capital: &CityIsland, satellites: &[CityIsland]) -> String {
    let mut content = format!("snap={};cap={}", HIVE_CITY_MAP_SCHEMA_VERSION, capital.revision_hash);
    for satellite in satellites {
        content.push_str(&format!(";sat={}:{}", satellite.id, satellite.revision_hash));
    }
    fnv_hex(fnv1a_128(content.as_bytes()))
}

/// Fills in the derived fields of a [`CityIsland`] (sorting, hashes, heights,
/// status consistency) before publishing it.
pub fn prepare_island(mut island: CityIsland) -> CityIsland {
    if island.status == CityIslandStatus::Unavailable {
        island.reason.get_or_insert(CityIslandUnavailableReason::RepoUnreadable);
        island.districts.clear();
        island.buildings.clear();
        island.edges.clear();
    }
    finalize_island(&mut island);
    island.revision_hash = compute_revision_hash(&island);
    island
}

/// Assembles a snapshot from a prepared capital and prepared satellites,
/// enforcing canononical sorting, the satellite cap, and the payload budget.
pub fn assemble_snapshot(
    capital: CityIsland,
    mut satellites: Vec<CityIsland>,
    generated_at_ms: i64,
) -> HiveCityMapSnapshot {
    for satellite in &mut satellites {
        if satellite.role != CityIslandRole::Satellite {
            satellite.role = CityIslandRole::Satellite;
        }
    }
    satellites.sort_by(|a, b| (a.id.as_str(), a.name.as_str()).cmp(&(b.id.as_str(), b.name.as_str())));
    satellites.truncate(MAX_SATELLITES);
    let snapshot = HiveCityMapSnapshot {
        schema_version: HIVE_CITY_MAP_SCHEMA_VERSION,
        generated_at_ms,
        revision_hash: compute_snapshot_revision_hash(&capital, &satellites),
        capital,
        satellites,
    };
    fit_snapshot_to_payload(snapshot)
}

/// Deterministically truncates a snapshot so its serialized JSON fits
/// [`MAX_CITY_MAP_PAYLOAD_BYTES`]. Truncation order:
/// 1. drop satellite edges with the lowest `weight` first (batches), then
///    satellite secondary `module` buildings with the lowest `line_count`
///    (together with edges referencing them);
/// 2. only then the same for the capital (the capital is prioritized);
/// 3. extreme fallback: the embedded capital alone.
pub fn fit_snapshot_to_payload(mut snapshot: HiveCityMapSnapshot) -> HiveCityMapSnapshot {
    if serialized_len(&snapshot) <= MAX_CITY_MAP_PAYLOAD_BYTES {
        return snapshot;
    }
    let mut satellite_prune_passes = 0u8;
    loop {
        if serialized_len(&snapshot) <= MAX_CITY_MAP_PAYLOAD_BYTES {
            snapshot.revision_hash = compute_snapshot_revision_hash(&snapshot.capital, &snapshot.satellites);
            return snapshot;
        }
        if satellite_prune_passes < 3 {
            prune_satellite_payload(&mut snapshot.satellites);
            satellite_prune_passes = satellite_prune_passes.saturating_add(1);
            if serialized_len(&snapshot) <= MAX_CITY_MAP_PAYLOAD_BYTES {
                snapshot.revision_hash = compute_snapshot_revision_hash(&snapshot.capital, &snapshot.satellites);
                return snapshot;
            }
        }
        if reduce_capital_payload(&mut snapshot.capital) {
            continue;
        }
        // Extreme fallback (embedded capital alone always fits comfortably).
        let fallback = HiveCityMapSnapshot {
            schema_version: HIVE_CITY_MAP_SCHEMA_VERSION,
            generated_at_ms: snapshot.generated_at_ms,
            revision_hash: String::new(),
            capital: embedded_capital_island(),
            satellites: Vec::new(),
        };
        return HiveCityMapSnapshot {
            revision_hash: compute_snapshot_revision_hash(&fallback.capital, &[]),
            ..fallback
        };
    }
}

/// Drops one pruning batch from every satellite island that still carries
/// removable payload; islands keep their metadata even at zero structure.
fn prune_satellite_payload(satellites: &mut [CityIsland]) {
    for island in satellites.iter_mut() {
        if !island.edges.is_empty() {
            island.edges.sort_by(|a, b| a.weight.cmp(&b.weight));
            let cut = island.edges.len().div_ceil(4).max(1);
            island.edges.drain(..cut);
            continue;
        }
        drop_secondary_buildings(island);
    }
}

fn reduce_capital_payload(capital: &mut CityIsland) -> bool {
    if !capital.edges.is_empty() {
        capital.edges.sort_by(|a, b| a.weight.cmp(&b.weight));
        let cut = capital.edges.len().div_ceil(4).max(1);
        capital.edges.drain(..cut);
        return true;
    }
    drop_secondary_buildings(capital)
}

/// Removes secondary `module` buildings with the lowest `line_count` (a
/// quarter batch) and every edge that referenced them. Returns `true` when
/// anything was removed.
fn drop_secondary_buildings(island: &mut CityIsland) -> bool {
    let mut candidates: Vec<&CityBuilding> = island
        .buildings
        .iter()
        .filter(|building| building.kind == CityBuildingKind::Module)
        .collect();
    if candidates.is_empty() {
        return false;
    }
    candidates.sort_by(|a, b| {
        (b.line_count, a.id.as_str()).cmp(&(a.line_count, b.id.as_str()))
    });
    let cut = candidates.len().div_ceil(4).max(1);
    let remove: BTreeSet<String> = candidates
        .into_iter()
        .take(cut)
        .map(|building| building.id.clone())
        .collect();
    island.buildings.retain(|building| !remove.contains(&building.id));
    island.edges.retain(|edge| !remove.contains(&edge.from_id) && !remove.contains(&edge.to_id));
    true
}

fn serialized_len(snapshot: &HiveCityMapSnapshot) -> usize {
    serde_json::to_vec(snapshot).map(|bytes| bytes.len()).unwrap_or(usize::MAX)
}

// ---------------------------------------------------------------------------
// Isolated island producers
// ---------------------------------------------------------------------------

/// Builds an island that reflects an unavailable repository.
pub fn unavailable_island(
    id: String,
    name: String,
    role: CityIslandRole,
    reason: CityIslandUnavailableReason,
) -> CityIsland {
    prepare_island(CityIsland {
        id,
        name: sanitize_schema_string(&name),
        role,
        source: CityIslandSource::LiveWorkspace,
        status: CityIslandStatus::Unavailable,
        reason: Some(reason),
        ecosystem: CityEcosystem::Generic,
        structure_hash: String::new(),
                revision_hash: String::new(),
        accent_hue: 0,
        districts: Vec::new(),
        buildings: Vec::new(),
        edges: Vec::new(),
        total_files: 0,
        total_lines: 0,
    })
}

/// Public identifier of a satellite island derived from the canonical
/// normalized repository path: `"sat-"` plus the first 16 hex characters of
/// the FNV-1a 128-bit hash of that path.
pub fn satellite_id_for(canonical_path: &str) -> String {
    format!("sat-{}", fnv_hex_short(fnv1a_128(canonical_path.as_bytes()), 16))
}

/// Builds the built-in canonical capital manifest of the EvoHime architecture
/// (used whenever `eva_workspace_path` is missing, unreadable, or timed out).
pub fn embedded_capital_island() -> CityIsland {
    let entries: &[(&str, CityBuildingKind, CityDistrictKind, &str)] = &[
        ("crates/evohime-core", CityBuildingKind::Citadel, CityDistrictKind::Core, "rust"),
        ("crates/evohime-hive", CityBuildingKind::Citadel, CityDistrictKind::Core, "rust"),
        ("crates/desktop-ipc", CityBuildingKind::Gateway, CityDistrictKind::Runtime, "rust"),
        ("crates/evohime-supervisor", CityBuildingKind::Worker, CityDistrictKind::Runtime, "rust"),
        ("crates/evohime-local-storage", CityBuildingKind::Storage, CityDistrictKind::Memory, "rust"),
        ("crates/permissions", CityBuildingKind::Library, CityDistrictKind::Core, "rust"),
        ("crates/tool-runtime", CityBuildingKind::Library, CityDistrictKind::Tools, "rust"),
        ("crates/model-gateway", CityBuildingKind::Gateway, CityDistrictKind::Core, "rust"),
        ("crates/evohime-listener", CityBuildingKind::Worker, CityDistrictKind::Voice, "rust"),
        ("crates/evohime-listener-audio", CityBuildingKind::Library, CityDistrictKind::Voice, "rust"),
        ("crates/evohime-listener-contract", CityBuildingKind::Library, CityDistrictKind::Voice, "rust"),
        ("crates/evohime-listener-ipc", CityBuildingKind::Gateway, CityDistrictKind::Voice, "rust"),
        ("crates/evohime-updater", CityBuildingKind::Worker, CityDistrictKind::App, "rust"),
        ("crates/evohime-update-agent", CityBuildingKind::Worker, CityDistrictKind::App, "rust"),
        ("crates/evohime-cli", CityBuildingKind::Service, CityDistrictKind::App, "rust"),
        ("crates/evohime-cli-contract", CityBuildingKind::Library, CityDistrictKind::App, "rust"),
        ("crates/evohime-cli-protocol", CityBuildingKind::Library, CityDistrictKind::App, "rust"),
        ("crates/evohime-hive-contract", CityBuildingKind::Library, CityDistrictKind::Core, "rust"),
        ("crates/evohime-remote", CityBuildingKind::Library, CityDistrictKind::Core, "rust"),
        ("crates/evohime-receipts", CityBuildingKind::Library, CityDistrictKind::Core, "rust"),
        ("crates/evohime-model-provenance", CityBuildingKind::Library, CityDistrictKind::Core, "rust"),
        ("crates/evohime-eval", CityBuildingKind::Library, CityDistrictKind::Core, "rust"),
        ("crates/context-budget", CityBuildingKind::Library, CityDistrictKind::Core, "rust"),
        ("crates/test-stub-exitcode", CityBuildingKind::Library, CityDistrictKind::Other, "rust"),
        ("desktop/evohime-electron", CityBuildingKind::Ui, CityDistrictKind::Shell, "typescript"),
    ];
    let mut districts: BTreeMap<String, CityDistrict> = BTreeMap::new();
    let mut buildings = Vec::new();
    for (index, (relative_path, kind, district_kind, language)) in entries.iter().enumerate() {        let district = districts
            .entry(district_kind_label(*district_kind))
            .or_insert_with(|| CityDistrict {
                id: district_kind_label(*district_kind),
                label: district_label(*district_kind),
                kind: *district_kind,
            })
            .clone();
        let name = relative_path.rsplit('/').next().unwrap_or(relative_path);
        buildings.push(CityBuilding {
            id: (*relative_path).to_owned(),
            name: sanitize_schema_string(name),
            district_id: district.id,
            kind: *kind,
            relative_path: (*relative_path).to_owned(),
            file_count: 0,
            // Deterministic pseudo-metric for the offline silhouette: stable
            // indices reproduce recognizable city proportions.
            line_count: u64::try_from(entries.len() - index).unwrap_or(1),
            height_weight_bp: 150,
            language: (*language).to_owned(),
        });
    }
    // Deterministic architecture spine of the embedded capital, so the map
    // always shows the core IPC and dependency relations even offline.
    let capital_edges: &[(&str, &str, CityEdgeKind)] = &[
        (
            "desktop/evohime-electron",
            "crates/desktop-ipc",
            CityEdgeKind::Ipc,
        ),
        (
            "crates/desktop-ipc",
            "crates/evohime-core",
            CityEdgeKind::Ipc,
        ),
        (
            "crates/evohime-core",
            "crates/evohime-hive",
            CityEdgeKind::Dependency,
        ),
        (
            "crates/evohime-core",
            "crates/evohime-supervisor",
            CityEdgeKind::Runtime,
        ),
        (
            "crates/evohime-core",
            "crates/model-gateway",
            CityEdgeKind::Dependency,
        ),
        (
            "crates/evohime-core",
            "crates/evohime-local-storage",
            CityEdgeKind::Dependency,
        ),
        (
            "crates/evohime-core",
            "crates/tool-runtime",
            CityEdgeKind::Dependency,
        ),
        (
            "crates/evohime-core",
            "crates/permissions",
            CityEdgeKind::Dependency,
        ),
        (
            "crates/evohime-core",
            "crates/evohime-listener",
            CityEdgeKind::Dependency,
        ),
        (
            "crates/evohime-cli",
            "crates/evohime-core",
            CityEdgeKind::Dependency,
        ),
    ];
    let mut island = CityIsland {
        id: CAPITAL_ISLAND_ID.to_owned(),
        name: "EvoHime".to_owned(),
        role: CityIslandRole::Capital,
        source: CityIslandSource::EmbeddedManifest,
        status: CityIslandStatus::Ready,
        reason: None,
        ecosystem: CityEcosystem::Mixed,
        structure_hash: String::new(),
                revision_hash: String::new(),
        accent_hue: 0,
        districts: districts.into_values().collect(),
        buildings,
        edges: capital_edges
            .iter()
            .map(|(from_id, to_id, kind)| CityEdge {
                from_id: (*from_id).to_owned(),
                to_id: (*to_id).to_owned(),
                kind: *kind,
                weight: saturate_weight(
                    0,
                    if *kind == CityEdgeKind::Ipc { 60 } else { 30 },
                ),
            })
            .collect(),
        total_files: 0,
        total_lines: 0,
    };
    finalize_island(&mut island);
    island.revision_hash = compute_revision_hash(&island);
    island
}

fn district_kind_label(kind: CityDistrictKind) -> String {
    match kind {
        CityDistrictKind::Core => "core",
        CityDistrictKind::Shell => "shell",
        CityDistrictKind::Memory => "memory",
        CityDistrictKind::Tools => "tools",
        CityDistrictKind::Voice => "voice",
        CityDistrictKind::Runtime => "runtime",
        CityDistrictKind::App => "app",
        CityDistrictKind::Packages => "packages",
        CityDistrictKind::Docs => "docs",
        CityDistrictKind::Other => "other",
    }
    .to_owned()
}

fn district_label(kind: CityDistrictKind) -> String {
    match kind {
        CityDistrictKind::Core => "Core",
        CityDistrictKind::Shell => "Shell",
        CityDistrictKind::Memory => "Memory",
        CityDistrictKind::Tools => "Tools",
        CityDistrictKind::Voice => "Voice",
        CityDistrictKind::Runtime => "Runtime",
        CityDistrictKind::App => "App",
        CityDistrictKind::Packages => "Packages",
        CityDistrictKind::Docs => "Docs",
        CityDistrictKind::Other => "Other",
    }
    .to_owned()
}

// ---------------------------------------------------------------------------
// Bounded Cargo.toml subset parser
// ---------------------------------------------------------------------------

/// Reference of one parsed dependency entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CargoDependencyRef {
    /// `{ path = "..." }` or `path = "..."` string value.
    Path(String),
    /// `foo.workspace = true` (spec lives in `[workspace.dependencies]`).
    Workspace,
    /// Anything else (registry version spec without a local path).
    Spec,
}

/// Result of the strict documented `Cargo.toml` subset parse.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct CargoManifest {
    /// The `[workspace]` section was present.
    pub is_workspace: bool,
    /// Raw `members = [...]` strings (globs included verbatim).
    pub members: Vec<String>,
    /// `[package] name = "..."` value.
    pub package_name: Option<String>,
    /// `[workspace.dependencies]` entries that resolve to a local path.
    pub workspace_dependency_paths: Vec<(String, String)>,
    /// Dependency entries of the manifest sections resolved to a local path
    /// relative to the manifest directory.
    pub dependency_paths: Vec<(String, CargoDependencyRef)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CargoSection {
    Other,
    Workspace,
    WorkspaceDependencies,
    Package,
    Dependencies,
}

/// Parses the documented subset of a `Cargo.toml` file: sections
/// `[workspace]`, `[package]`, `[dependencies]`, `[dev-dependencies]`,
/// `[build-dependencies]`, `[target.*.dependencies]`, `[workspace.dependencies]`;
/// keys `members` (multi-line arrays included), `name`, `path`, dotted
/// `foo.workspace = true` and inline tables; `#` comments and string literals
/// are respected. Unknown keys and syntax are ignored silently — the parser is
/// a structural hint source, not a validator.
pub(crate) fn parse_cargo_manifest_subset(bytes: &[u8]) -> CargoManifest {
    let text = String::from_utf8_lossy(bytes);
    let mut manifest = CargoManifest::default();
    let mut section = CargoSection::Other;
    let mut lines = text.lines().peekable();
    while let Some(raw_line) = lines.next() {
        let line = strip_outside_string_comments(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(inner) = trimmed.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')) {
            section = classify_cargo_section(inner);
            if section == CargoSection::Workspace {
                manifest.is_workspace = true;
            }
            continue;
        }
        let Some((key, value)) = split_manifest_assignment(trimmed) else {
            continue;
        };
        let mut value = value;
        // Multi-line array: keep consuming lines until brackets balance.
        if value.starts_with('[') && !brackets_balanced(&value) {
            let mut chunk = value.clone();
            while !brackets_balanced(&chunk) {
                match lines.next() {
                    Some(next_line) => {
                        chunk.push(' ');
                        chunk.push_str(strip_outside_string_comments(next_line).trim());
                    }
                    None => break,
                }
            }
            value = chunk;
        }
        match section {
            CargoSection::Other => {}
            CargoSection::Workspace => {
                if key == "members" {
                    manifest.members = extract_quoted_strings(&value);
                }
            }
            CargoSection::Package => {
                if key == "name" {
                    manifest.package_name = extract_quoted_strings(&value).first().cloned();
                }
            }
            CargoSection::WorkspaceDependencies => {
                if let Some(path) = inline_table_string(&value, "path") {
                    let dep_name = key.split('.').next().unwrap_or(&key);
                    manifest.workspace_dependency_paths.push((dep_name.to_owned(), path));
                }
            }
            CargoSection::Dependencies => {
                let dep_name = key.split('.').next().unwrap_or(&key);
                let property = key.split_once('.').map(|(_, rest)| rest.trim());
                if property == Some("workspace") && value == "true" {
                    manifest.dependency_paths.push((dep_name.to_owned(), CargoDependencyRef::Workspace));
                } else if inline_table_contains_workspace(&value) {
                    manifest.dependency_paths.push((dep_name.to_owned(), CargoDependencyRef::Workspace));
                } else if let Some(path) = inline_table_string(&value, "path") {
                    manifest.dependency_paths.push((dep_name.to_owned(), CargoDependencyRef::Path(path)));
                } else if !value.trim().is_empty() {
                    // Registry spec (`foo = "1.2"`) or any other non-empty
                    // value: no local path resolution.
                    manifest.dependency_paths.push((dep_name.to_owned(), CargoDependencyRef::Spec));
                }
            }
        }
    }
    manifest
}

fn classify_cargo_section(inner: &str) -> CargoSection {
    let inner = inner.trim();
    if inner == "workspace" {
        CargoSection::Workspace
    } else if inner == "workspace.dependencies" {
        CargoSection::WorkspaceDependencies
    } else if inner == "package" {
        CargoSection::Package
    } else if inner == "dependencies"
        || inner == "dev-dependencies"
        || inner == "build-dependencies"
        || (inner.starts_with("target.") && inner.ends_with(".dependencies"))
    {
        CargoSection::Dependencies
    } else {
        CargoSection::Other
    }
}

fn strip_outside_string_comments(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut in_string = false;
    let mut escaped = false;
    for ch in line.chars() {
        if in_string {
            result.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
            result.push(ch);
        } else if ch == '#' {
            break;
        } else {
            result.push(ch);
        }
    }
    result
}

fn split_manifest_assignment(line: &str) -> Option<(String, String)> {
    let bytes = line.as_bytes();
    let mut in_string = false;
    let mut escaped = false;
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth = depth.saturating_sub(1),
            b'=' if depth == 0 => {
                let key = line[..index].trim().to_owned();
                let value = line[index + 1..].trim().to_owned();
                return if key.is_empty() { None } else { Some((key, value)) };
            }
            _ => {}
        }
    }
    None
}

fn brackets_balanced(value: &str) -> bool {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for ch in value.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '[' | '{' => depth += 1,
            ']' | '}' => depth = depth.saturating_sub(1),
            '#' => break,
            _ => {}
        }
    }
    depth == 0 && !in_string
}

fn extract_quoted_strings(value: &str) -> Vec<String> {
    let mut strings = Vec::new();
    let mut bytes = value.char_indices().peekable();
    while let Some((_, ch)) = bytes.next() {
        if ch != '"' {
            continue;
        }
        let mut literal = String::new();
        let mut closed = false;
        for (_, inner) in bytes.by_ref() {
            if inner == '\\' {
                continue;
            }
            if inner == '"' {
                closed = true;
                break;
            }
            literal.push(inner);
        }
        if closed {
            strings.push(literal);
        }
    }
    strings
}

fn inline_table_string(value: &str, key: &str) -> Option<String> {
    let needle = format!("{key} =");
    let search_from = value.find(&needle)?;
    let remainder = &value[search_from + needle.len()..];
    extract_quoted_strings(remainder).first().cloned()
}

fn inline_table_contains_workspace(value: &str) -> bool {
    let patterns = ["workspace = true", "workspace=true"];
    patterns.iter().any(|pattern| value.contains(pattern))
}

// ---------------------------------------------------------------------------
// Bounded package.json reader
// ---------------------------------------------------------------------------

/// Result of the bounded `package.json` read.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct NodePackage {
    /// `name` field, if present.
    pub name: Option<String>,
    /// Raw `workspaces` patterns (glob strings included verbatim).
    pub workspaces: Vec<String>,
    /// Dependency keys (internal package names only, resolved later).
    pub dependency_names: Vec<String>,
}

/// Parses a `package.json` file through `serde_json` with bounded extract.
pub(crate) fn parse_node_package(bytes: &[u8]) -> Result<NodePackage, ()> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| ())?;
    let name = value
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(sanitize_schema_string);
    let mut workspaces = Vec::new();
    match value.get("workspaces") {
        Some(serde_json::Value::Array(items)) => {
            for item in items {
                if let Some(pattern) = item.as_str() {
                    workspaces.push(pattern.to_owned());
                }
            }
        }
        Some(serde_json::Value::Object(map)) => {
            if let Some(items) = map.get("packages").and_then(serde_json::Value::as_array) {
                for item in items {
                    if let Some(pattern) = item.as_str() {
                        workspaces.push(pattern.to_owned());
                    }
                }
            }
        }
        _ => {}
    }
    let mut dependency_names = Vec::new();
    for field in ["dependencies", "devDependencies"] {
        if let Some(map) = value.get(field).and_then(serde_json::Value::as_object) {
            for key in map.keys() {
                dependency_names.push(key.clone());
            }
        }
    }
    Ok(NodePackage {
        name,
        workspaces,
        dependency_names,
    })
}

// ---------------------------------------------------------------------------
// Repository fingerprint (bounded cache signature)
// ---------------------------------------------------------------------------

/// One entry of the bounded recursive repository fingerprint: a directory
/// summary or a manifest file record, canonically sorted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoFingerprintEntry {
    /// Normalized relative POSIX path of the fingerprinted item.
    pub relative_path: String,
    /// For directories: number of entries seen (after the skip rules); for
    /// manifest files: byte size.
    pub entry_count: usize,
    /// Modification time in Unix milliseconds (`0` when unavailable).
    pub modified_at_ms: i64,
    /// Whether the entry is one of the recognized manifest files.
    pub is_manifest: bool,
}

/// Bounded cache signature of a repository: `(relative_path, entry_count,
/// mtime)` for directories within the scan depth plus `(relative_path, size,
/// mtime)` for key manifests. Edits of source file contents that change
/// neither the file list nor the manifests do not move the signature; they
/// are picked up by the planned full rescan (`line_count` is approximate).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoFingerprint {
    /// Canonically sorted fingerprint entries.
    pub entries: Vec<RepoFingerprintEntry>,
}

const MANIFEST_NAMES: &[&str] = &["cargo.toml", "package.json", "pyproject.toml", "go.mod"];

fn is_manifest_name(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    MANIFEST_NAMES.contains(&lower.as_str())
        || lower.ends_with(".sln")
        || lower.ends_with(".csproj")
}

/// Computes the bounded fingerprint of a repository (read-only walk, same
/// skip rules and limits as the scanner).
pub fn repository_fingerprint(repo_root: &Path) -> Result<RepoFingerprint, CityIslandUnavailableReason> {
    let metadata = fs::symlink_metadata(repo_root)
        .map_err(map_walk_error)?;
    if !metadata.is_dir() {
        return Err(CityIslandUnavailableReason::RepoNotFound);
    }
    let canonical = fs::canonicalize(repo_root).map_err(map_walk_error)?;
    let mut entries = Vec::new();
    fingerprint_recursive(&canonical, "", 0, &mut entries);
    entries.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(RepoFingerprint { entries })
}

fn fingerprint_recursive(dir: &Path, relative: &str, depth: usize, out: &mut Vec<RepoFingerprintEntry>) {
    if depth > MAX_SCAN_DEPTH {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<(std::ffi::OsString, fs::DirEntry)> = Vec::new();
    for entry in read {
        let Ok(entry) = entry else { continue };
        items.push((entry.file_name(), entry));
    }
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items.truncate(MAX_ENTRIES_PER_DIR);
    let dotnet_here = items
        .iter()
        .any(|(name, _)| name.to_string_lossy().to_ascii_lowercase().ends_with(".sln"));
    for (name, entry) in items.iter() {
        let name_string = name.to_string_lossy();
        if is_skipped_dir_name(
            &name_string,
            dotnet_here,
            relative.split('/').any(|part| part == "src"),
        ) {
            continue;
        }
        // Symlinks (including Windows reparse points for junction directories)
        // are never descended into: the local TOCTOU race is accepted, escape
        // beyond the canonical root is not.
        let Ok(default_meta) = entry.metadata() else { continue };
        if default_meta.file_type().is_symlink() {
            continue;
        }
        let child_relative = join_relative(relative, name.as_os_str());
        if default_meta.is_dir() {
            fingerprint_recursive(&entry.path(), &child_relative, depth + 1, out);
        }
    }
    // Directory summary entry for the signature (bounded input, cheap compare).
    let modified_at_ms = fs::symlink_metadata(dir)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(system_time_to_unix_ms)
        .unwrap_or(0);
    out.push(RepoFingerprintEntry {
        relative_path: relative.to_owned(),
        entry_count: 0,
        modified_at_ms,
        is_manifest: false,
    });
    // Manifest files of this directory are signature entries as well.
    for (name, _) in &items {
        let file_name = name.to_string_lossy();
        if is_manifest_name(&file_name) {
            let path = dir.join(name);
            let Ok(meta) = fs::symlink_metadata(&path) else { continue };
            if meta.is_symlink() || !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
                continue;
            }
            out.push(RepoFingerprintEntry {
                relative_path: join_relative(relative, name),
                entry_count: usize::try_from(meta.len()).unwrap_or(usize::MAX),
                modified_at_ms: meta
                    .modified()
                    .ok()
                    .and_then(system_time_to_unix_ms)
                    .unwrap_or(0),
                is_manifest: true,
            });
        }
    }
}

fn system_time_to_unix_ms(time: SystemTime) -> Option<i64> {
    let duration = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

fn map_walk_error(error: io::Error) -> CityIslandUnavailableReason {
    match error.kind() {
        io::ErrorKind::NotFound => CityIslandUnavailableReason::RepoNotFound,
        io::ErrorKind::PermissionDenied => CityIslandUnavailableReason::PermissionDenied,
        _ => CityIslandUnavailableReason::RepoUnreadable,
    }
}

// ---------------------------------------------------------------------------
// Island scanner
// ---------------------------------------------------------------------------

/// Read of one manifest file bounded by [`MAX_MANIFEST_BYTES`].
fn read_bounded_file(path: &Path, max_bytes: u64) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return None;
    }
    fs::read(path).ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleCandidate {
    /// Normalized relative POSIX path of the module root (`""` = repo root).
    relative_path: String,
    /// Display name (package name or last path segment).
    name: String,
    kind: CityBuildingKind,
    district: CityDistrictKind,
    language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EcosystemFlags {
    rust: bool,
    node: bool,
    python: bool,
    go: bool,
    dotnet: bool,
}

impl EcosystemFlags {
    fn count(self) -> usize {
        usize::from(self.rust)
            + usize::from(self.node)
            + usize::from(self.python)
            + usize::from(self.go)
            + usize::from(self.dotnet)
    }

    fn resolve(self) -> CityEcosystem {
        match self.count() {
            0 => CityEcosystem::Generic,
            1 if self.rust => CityEcosystem::Rust,
            1 if self.node => CityEcosystem::Node,
            1 if self.python => CityEcosystem::Python,
            1 if self.go => CityEcosystem::Go,
            1 => CityEcosystem::Dotnet,
            _ => CityEcosystem::Mixed,
        }
    }
}

/// Classification of a module into building kind and district. Deterministic,
/// name-driven heap of the documented EvoHime domains; unknown modules are
/// `Library` in `core` (or `Module` in `other` for the generic fallback).
fn classify_crate(relative_path: &str, package_name: &str, has_bins: bool) -> (CityBuildingKind, CityDistrictKind) {
    let haystack = format!("{relative_path}|{package_name}");
    if haystack.contains("evohime-core") || haystack.contains("evohime-hive") {
        return (CityBuildingKind::Citadel, CityDistrictKind::Core);
    }
    if haystack.contains("local-storage") {
        return (CityBuildingKind::Storage, CityDistrictKind::Memory);
    }
    if haystack.contains("listener") {
        return (CityBuildingKind::Worker, CityDistrictKind::Voice);
    }
    if haystack.contains("tool-runtime") {
        return (CityBuildingKind::Library, CityDistrictKind::Tools);
    }
    if haystack.contains("supervisor") {
        return (CityBuildingKind::Worker, CityDistrictKind::Runtime);
    }
    if haystack.contains("desktop-ipc") || haystack.contains("model-gateway") {
        return (CityBuildingKind::Gateway, CityDistrictKind::Runtime);
    }
    if haystack.contains("updater") || haystack.contains("update-agent") {
        return (CityBuildingKind::Worker, CityDistrictKind::App);
    }
    if haystack.ends_with("-cli") || haystack.contains("/evohime-cli-") {
        return (CityBuildingKind::Service, CityDistrictKind::App);
    }
    if haystack.contains("electron") || haystack.contains("-ui") {
        return (CityBuildingKind::Ui, CityDistrictKind::Shell);
    }
    if has_bins {
        return (CityBuildingKind::Service, CityDistrictKind::App);
    }
    (CityBuildingKind::Library, CityDistrictKind::Core)
}

fn classify_node(relative_path: &str, package_name: &str) -> (CityBuildingKind, CityDistrictKind) {
    let haystack = format!("{relative_path}|{package_name}");
    if haystack.contains("electron") || haystack.contains("renderer") || haystack.contains("preload") {
        (CityBuildingKind::Ui, CityDistrictKind::Shell)
    } else {
        (CityBuildingKind::Library, CityDistrictKind::Packages)
    }
}

/// Deadline provider shared through the bounded walk.
#[derive(Debug, Clone, Copy)]
struct ScanBudget {
    deadline: Option<Instant>,
}

impl ScanBudget {
    fn expired(self) -> bool {
        self.deadline.is_some_and(|deadline| Instant::now() >= deadline)
    }
}

/// A single walked source file pending attribution to a module.
#[derive(Debug, Clone)]
struct WalkedFile {
    relative_path: String,
    absolute_path: PathBuf,
    extension: String,
}

/// Read-only single-repository scan producing a ready island structure.
///
/// `deadline` is the absolute wall deadline (already including the repo
/// budget); on exhaustion the caller receives
/// [`CityIslandUnavailableReason::ScanTimeout`] and decides between the
/// cached island and a fresh `unavailable` island.
pub fn scan_island(
    repo_root: &Path,
    island_id: String,
    display_name: &str,
    role: CityIslandRole,
    deadline: Option<Instant>,
) -> Result<CityIsland, CityIslandUnavailableReason> {
    let metadata = fs::symlink_metadata(repo_root).map_err(map_walk_error)?;
    if !metadata.is_dir() {
        return Err(CityIslandUnavailableReason::RepoNotFound);
    }
    let canonical = fs::canonicalize(repo_root).map_err(map_walk_error)?;
    let budget = ScanBudget { deadline };

    // 1. Ecosystem detection from the bounded root listing.
    let root_entries = bounded_sorted_entries(&canonical, ScanBudget { deadline: None })?;
    let mut flags = EcosystemFlags {
        rust: manifest_exists(&root_entries, &canonical, "Cargo.toml"),
        node: manifest_exists(&root_entries, &canonical, "package.json"),
        python: manifest_exists(&root_entries, &canonical, "pyproject.toml"),
        go: manifest_exists(&root_entries, &canonical, "go.mod"),
        dotnet: root_entries
            .0
            .iter()
            .any(|(name, _)| name.to_ascii_lowercase().ends_with(".sln")),
    };

    // 2. Building candidates: explicit manifests first, generic fallback last.
    let root_cargo = read_bounded_file(&canonical.join("Cargo.toml"), MAX_MANIFEST_BYTES)
        .map(|bytes| parse_cargo_manifest_subset(&bytes));
    if root_cargo.as_ref().is_some_and(|manifest| manifest.is_workspace) {
        flags.rust = true;
    }
    let mut candidates = Vec::new();
    let mut node_dependency_edges: Vec<(String, String)> = Vec::new();
    candidates.extend(rust_module_candidates(
        &canonical,
        root_cargo.as_ref(),
        budget,
    )?);
    candidates.extend(node_module_candidates(
        &canonical,
        &root_entries,
        &mut node_dependency_edges,
        budget,
    )?);
    candidates.extend(python_module_candidates(&canonical, budget)?);
    candidates.extend(go_module_candidates(&canonical, budget)?);
    candidates.extend(dotnet_module_candidates(&canonical, &root_entries, budget)?);
    if candidates.is_empty() {
        candidates.extend(fallback_module_candidates(&canonical, budget)?);
    }
    candidates.truncate(MAX_BUILDINGS_PER_ISLAND);
    candidates.sort_by(|a, b| (b.relative_path.len(), a.relative_path.as_str()).cmp(&(a.relative_path.len(), b.relative_path.as_str())));

    // 3. Walk source files once and attribute them to the longest-prefix module.
    let mut walked = Vec::new();
    walk_source_files(&canonical, "", 0, &mut walked, budget)?;
    let mut buildings: Vec<CityBuilding> = Vec::new();
    let mut districts: BTreeMap<String, CityDistrict> = BTreeMap::new();
    let mut total_files = 0u64;
    let mut total_lines = 0u64;
    let mut building_stats: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    if budget.expired() {
        return Err(CityIslandUnavailableReason::ScanTimeout);
    }
    for file in &walked {
        total_files += 1;
        let lines = count_file_lines(&file.absolute_path, &file.extension).ok().flatten().unwrap_or(0);
        total_lines += lines;
        let owner = candidates
            .iter()
            .filter(|candidate| !candidate.relative_path.is_empty())
            .find(|candidate| file.relative_path.starts_with(&format!("{}/", candidate.relative_path)))
            .or_else(|| {
                candidates
                    .iter()
                    .find(|candidate| candidate.relative_path.is_empty())
            });
        if let Some(owner) = owner {
            let entry = building_stats.entry(owner.relative_path.clone()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += lines;
        }
    }
    for candidate in &candidates {
        let stats = building_stats
            .get(&candidate.relative_path)
            .copied()
            .unwrap_or((0, 0));
        let district = districts
            .entry(district_kind_label(candidate.district))
            .or_insert_with(|| CityDistrict {
                id: district_kind_label(candidate.district),
                label: district_label(candidate.district),
                kind: candidate.district,
            });
        let district_id = district.id.clone();
        buildings.push(CityBuilding {
            id: if candidate.relative_path.is_empty() {
                ".".to_owned()
            } else {
                candidate.relative_path.clone()
            },
            name: sanitize_schema_string(&candidate.name),
            district_id,
            kind: candidate.kind,
            relative_path: if candidate.relative_path.is_empty() {
                ".".to_owned()
            } else {
                candidate.relative_path.clone()
            },
            file_count: u32::try_from(stats.0).unwrap_or(u32::MAX),
            line_count: stats.1,
            height_weight_bp: 150,
            language: candidate.language.clone(),
        });
    }

    // 4. Edges between modules (cargo path/workspace deps, node internal deps).
    let mut edges = Vec::new();
    edges.extend(cargo_edges(root_cargo.as_ref(), &candidates, &canonical, budget)?);
    edges.extend(node_edges(&node_dependency_edges, &candidates));
    edges.truncate(MAX_EDGES_PER_ISLAND);

    let mut island = CityIsland {
        id: island_id,
        name: sanitize_schema_string(display_name),
        role,
        source: CityIslandSource::LiveWorkspace,
        status: CityIslandStatus::Ready,
        reason: None,
        ecosystem: flags.resolve(),
        structure_hash: String::new(),
                revision_hash: String::new(),
        accent_hue: 0,
        districts: Vec::new(),
        buildings,
        edges: edges
            .into_iter()
            .map(|(from_id, to_id, kind)| CityEdge {
                from_id,
                to_id,
                kind,
                weight: saturate_weight(0, if kind == CityEdgeKind::Ipc { 60 } else { 30 }),
            })
            .collect(),
        total_files: u32::try_from(total_files).unwrap_or(u32::MAX),
        total_lines,
    };
    island.districts = std::mem::take(&mut districts).into_values().collect();
    finalize_island(&mut island);
    island.revision_hash = compute_revision_hash(&island);
    Ok(island)
}

struct SortedEntries(Vec<(String, PathBuf)>);

fn bounded_sorted_entries(dir: &Path, budget: ScanBudget) -> Result<SortedEntries, CityIslandUnavailableReason> {
    if budget.expired() {
        return Err(CityIslandUnavailableReason::ScanTimeout);
    }
    let read = fs::read_dir(dir).map_err(map_walk_error)?;
    let mut items: Vec<(String, PathBuf)> = Vec::new();
    for entry in read {
        let Ok(entry) = entry else { continue };
        items.push((entry.file_name().to_string_lossy().trim().to_owned(), entry.path()));
    }
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items.truncate(MAX_ENTRIES_PER_DIR);
    Ok(SortedEntries(items))
}

fn manifest_exists(entries: &SortedEntries, _root: &Path, name: &str) -> bool {
    entries
        .0
        .iter()
        .any(|(entry_name, _)| entry_name.eq_ignore_ascii_case(name))
}

fn bounded_read_and_parse(
    path: &Path,
    max_bytes: u64,
) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "file size exceeds manifest bound"));
    }
    fs::read(path)
}

struct RustModule {
    relative_path: String,
    package_name: String,
    has_bins: bool,
}

/// Resolves root `Cargo.toml` (workspace or single package) into rust module
/// candidates and their parsed manifests.
fn rust_module_candidates(
    root: &Path,
    root_manifest: Option<&CargoManifest>,
    budget: ScanBudget,
) -> Result<Vec<ModuleCandidate>, CityIslandUnavailableReason> {
    let mut modules: Vec<RustModule> = Vec::new();
    if let Some(manifest) = root_manifest {
        if manifest.is_workspace {
            for raw_member in &manifest.members {
                let resolved = resolve_member_path(raw_member, root);
                for member_rel in resolved {
                    let member_abs = root.join(&member_rel);
                    let member_meta = match fs::symlink_metadata(&member_abs) {
                        Ok(meta) => meta,
                        Err(_) => continue,
                    };
                    if member_meta.is_symlink() || !member_meta.is_dir() {
                        continue;
                    }
                    let manifest_bytes = match bounded_read_and_parse(
                        &member_abs.join("Cargo.toml"),
                        MAX_MANIFEST_BYTES,
                    ) {
                        Ok(bytes) => bytes,
                        Err(_) => continue,
                    };
                    let member_manifest = parse_cargo_manifest_subset(&manifest_bytes);
                    let package_name = member_manifest
                        .package_name
                        .clone()
                        .unwrap_or_else(|| last_segment(&canonical_path_string(&member_rel)));
                    let has_bins = member_abs.join("src").join("bin").is_dir();
                    modules.push(RustModule {
                        relative_path: canonical_path_string(&member_rel),
                        package_name,
                        has_bins,
                    });
                }
            }
        } else if let Some(name) = &manifest.package_name {
            let has_bins = root.join("src").join("bin").is_dir();
            modules.push(RustModule {
                relative_path: String::new(),
                package_name: name.clone(),
                has_bins,
            });
        }
    }
    if budget.expired() {
        return Err(CityIslandUnavailableReason::ScanTimeout);
    }
    Ok(modules
        .into_iter()
        .map(|module| {
            let (kind, district) =
                classify_crate(&module.relative_path, &module.package_name, module.has_bins);
            ModuleCandidate {
                relative_path: module.relative_path,
                name: module.package_name,
                kind,
                district,
                language: "rust".to_owned(),
            }
        })
        .collect())
}

/// Expands a raw member entry: supports a single-level trailing glob
/// (`crates/*`) against the repository root and normalizes separators.
fn resolve_member_path(raw: &str, root: &Path) -> Vec<String> {
    let normalized = canonical_path_string(raw);
    if normalized.is_empty() || normalized.contains("..") {
        return Vec::new();
    }
    if normalized.contains('*') {
        let (glob_prefix, glob_suffix) = match normalized.split_once('*') {
            Some((prefix, suffix)) => (prefix.to_owned(), suffix.trim_start_matches('/').to_owned()),
            None => return Vec::new(),
        };
        // Only a trailing `*` (one directory level) is supported.
        if !glob_suffix.is_empty() && glob_suffix.contains('/') {
            return Vec::new();
        }
        let (parent_dir, name_prefix) = match glob_prefix.rsplit_once('/') {
            Some((dir, name)) => (dir.to_owned(), name.to_owned()),
            None => (String::new(), glob_prefix.clone()),
        };
        let scan_dir = if parent_dir.is_empty() {
            root.to_path_buf()
        } else {
            root.join(&parent_dir)
        };
        let Ok(read) = fs::read_dir(&scan_dir) else {
            return Vec::new();
        };
        let mut matches = Vec::new();
        for entry in read {
            let Ok(entry) = entry else { continue };
            let Ok(meta) = entry.metadata() else { continue };
            if meta.file_type().is_symlink() || !meta.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().replace('\\', "/");
            let name = name.clone();
            if !name.starts_with(&name_prefix) || !entry.path().join("Cargo.toml").exists() {
                continue;
            }
            let member = if parent_dir.is_empty() {
                name
            } else {
                format!("{parent_dir}/{name}")
            };
            let member = canonical_path_string(&member);
            if glob_suffix.is_empty() || member.ends_with(&glob_suffix) {
                matches.push(member);
            }
        }
        matches.sort();
        matches.dedup();
        return matches;
    }
    if root.join(&normalized).join("Cargo.toml").exists() {
        vec![normalized]
    } else {
        Vec::new()
    }
}

fn last_segment(canonical: &str) -> String {
    canonical.rsplit('/').next().unwrap_or(canonical).to_owned()
}

fn cargo_edges(
    root_manifest: Option<&CargoManifest>,
    candidates: &[ModuleCandidate],
    root: &Path,
    budget: ScanBudget,
) -> Result<Vec<(String, String, CityEdgeKind)>, CityIslandUnavailableReason> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    // Map module root dirs to candidate indices.
    let module_by_path: BTreeMap<&str, usize> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.relative_path.as_str(), index))
        .collect();
    // Per-candidate manifests: reparse from the crate manifests collected in
    // the candidate pass — candidates carry only names, so manifests are read
    // again here (bounded, cheap).
    let mut edges = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if candidate.language != "rust" {
            continue;
        }
        let manifest_dir = if candidate.relative_path.is_empty() {
            root.to_path_buf()
        } else {
            root.join(&candidate.relative_path)
        };
        let Ok(bytes) =
            bounded_read_and_parse(&manifest_dir.join("Cargo.toml"), MAX_MANIFEST_BYTES)
        else {
            continue;
        };
        if budget.expired() {
            return Err(CityIslandUnavailableReason::ScanTimeout);
        }
        let manifest = parse_cargo_manifest_subset(&bytes);
        let root_workspace_paths = root_manifest.map(|m| m.workspace_dependency_paths.as_slice());
        for (dep_name, reference) in &manifest.dependency_paths {
            let target_index = match reference {
                CargoDependencyRef::Path(path) => {
                    let dep_path = canonical_path_string(path);
                    // Path is relative to this manifest directory.
                    let joined = if candidate.relative_path.is_empty() {
                        dep_path.clone()
                    } else {
                        format!("{}/{}", candidate.relative_path, dep_path)
                    };
                    let joined = resolve_dot_segments(&joined);
                    module_by_path.get(joined.as_str()).copied()
                }
                CargoDependencyRef::Workspace => {
                    root_workspace_paths
                        .unwrap_or(&[])
                        .iter()
                        .find(|(key, _)| key == dep_name)
                        .and_then(|(_, path)| {
                            let joined = resolve_dot_segments(&canonical_path_string(path));
                            module_by_path.get(joined.as_str()).copied()
                        })
                }
                CargoDependencyRef::Spec => None,
            };
            if let Some(target_index) = target_index {
                if target_index == index {
                    continue;
                }
                let to_candidate = &candidates[target_index];
                let kind = if dep_name.contains("desktop-ipc") || to_candidate.relative_path.contains("desktop-ipc") {
                    CityEdgeKind::Ipc
                } else {
                    CityEdgeKind::Dependency
                };
                let from_id = candidate_module_id(candidate);
                let to_id = candidate_module_id(to_candidate);
                if from_id != to_id {
                    edges.push((from_id, to_id, kind));
                }
            }
        }
    }
    Ok(edges)
}

fn candidate_module_id(candidate: &ModuleCandidate) -> String {
    if candidate.relative_path.is_empty() {
        ".".to_owned()
    } else {
        candidate.relative_path.clone()
    }
}

/// Resolves `../` segments in an already-normalized canonical relative path.
fn resolve_dot_segments(input: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for segment in input.split('/') {
        if segment == ".." {
            stack.pop();
        } else if segment == "." || segment.is_empty() {
            continue;
        } else {
            stack.push(segment);
        }
    }
    stack.join("/")
}

fn node_module_candidates(
    _root: &Path,
    root_entries: &SortedEntries,
    dependency_edges: &mut Vec<(String, String)>,
    budget: ScanBudget,
) -> Result<Vec<ModuleCandidate>, CityIslandUnavailableReason> {
    let mut candidates = Vec::new();
    let mut collected: Vec<(String, NodePackage)> = Vec::new();
    // Nested package.json at depth 1 (their children give depth 2 paths).
    for (name, path) in &root_entries.0 {
        let package_path = path.join("package.json");
        let Some(bytes) = read_bounded_file(&package_path, MAX_MANIFEST_BYTES) else {
            continue;
        };
        if budget.expired() {
            return Err(CityIslandUnavailableReason::ScanTimeout);
        }
        if let Ok(package) = parse_node_package(&bytes) {
            let relative = canonical_path_string(name);
            dependency_edges.extend(
                package
                    .dependency_names
                    .iter()
                    .cloned()
                    .map(|dep| (relative.clone(), dep)),
            );
            collected.push((relative, package));
        }
    }
    for (relative, package) in collected {
        let name = package
            .name
            .clone()
            .unwrap_or_else(|| last_segment(&relative));
        let (kind, district) = classify_node(&relative, &name);
        candidates.push(ModuleCandidate {
            relative_path: relative,
            name,
            kind,
            district,
            language: "typescript".to_owned(),
        });
    }
    Ok(candidates)
}

fn node_edges(
    dependency_edges: &[(String, String)],
    candidates: &[ModuleCandidate],
) -> Vec<(String, String, CityEdgeKind)> {
    let id_by_relative: BTreeMap<&str, String> = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.relative_path.as_str(),
                if candidate.relative_path.is_empty() {
                    ".".to_owned()
                } else {
                    candidate.relative_path.clone()
                },
            )
        })
        .collect();
    let id_by_package_name: BTreeMap<&str, String> = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.name.as_str(),
                if candidate.relative_path.is_empty() {
                    ".".to_owned()
                } else {
                    candidate.relative_path.clone()
                },
            )
        })
        .collect();
    let mut edges = Vec::new();
    for (from_relative, dep_name) in dependency_edges {
        let Some(from_id) = id_by_relative.get(from_relative.as_str()) else {
            continue;
        };
        let Some(to_id) = id_by_package_name.get(dep_name.as_str()) else {
            continue;
        };
        if from_id != to_id {
            edges.push((from_id.clone(), to_id.clone(), CityEdgeKind::Dependency));
        }
    }
    edges
}

fn python_module_candidates(
    root: &Path,
    budget: ScanBudget,
) -> Result<Vec<ModuleCandidate>, CityIslandUnavailableReason> {
    if !root.join("pyproject.toml").exists() {
        return Ok(Vec::new());
    }
    if budget.expired() {
        return Err(CityIslandUnavailableReason::ScanTimeout);
    }
    let mut candidates = vec![ModuleCandidate {
        relative_path: String::new(),
        name: last_segment(&canonical_path_string(&root.to_string_lossy())),
        kind: CityBuildingKind::Library,
        district: CityDistrictKind::Packages,
        language: "python".to_owned(),
    }];
    if let Ok(read) = fs::read_dir(root) {
        let mut items: Vec<_> = read.flatten().collect();
        items.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
        for entry in items.into_iter().take(MAX_ENTRIES_PER_DIR) {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if !entry.path().join("__init__.py").exists() {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.file_type().is_symlink() || !meta.is_dir() {
                continue;
            }
            candidates.push(ModuleCandidate {
                relative_path: canonical_path_string(&entry.file_name().to_string_lossy()),
                name: entry.file_name().to_string_lossy().trim().to_owned(),
                kind: CityBuildingKind::Module,
                district: CityDistrictKind::Packages,
                language: "python".to_owned(),
            });
        }
    }
    Ok(candidates)
}

fn go_module_candidates(
    root: &Path,
    budget: ScanBudget,
) -> Result<Vec<ModuleCandidate>, CityIslandUnavailableReason> {
    if !root.join("go.mod").exists() {
        return Ok(Vec::new());
    }
    if budget.expired() {
        return Err(CityIslandUnavailableReason::ScanTimeout);
    }
    let mut candidates = vec![ModuleCandidate {
        relative_path: String::new(),
        name: last_segment(&canonical_path_string(&root.to_string_lossy())),
        kind: CityBuildingKind::Service,
        district: CityDistrictKind::Runtime,
        language: "go".to_owned(),
    }];
    if root.join("cmd").is_dir() {
        candidates.push(ModuleCandidate {
            relative_path: "cmd".to_owned(),
            name: "cmd".to_owned(),
            kind: CityBuildingKind::Service,
            district: CityDistrictKind::App,
            language: "go".to_owned(),
        });
    }
    Ok(candidates)
}

fn dotnet_module_candidates(
    _root: &Path,
    root_entries: &SortedEntries,
    budget: ScanBudget,
) -> Result<Vec<ModuleCandidate>, CityIslandUnavailableReason> {
    let has_solution = root_entries
        .0
        .iter()
        .any(|(name, _)| name.to_ascii_lowercase().ends_with(".sln"));
    if !has_solution {
        return Ok(Vec::new());
    }
    if budget.expired() {
        return Err(CityIslandUnavailableReason::ScanTimeout);
    }
    let mut candidates = Vec::new();
    for (name, path) in &root_entries.0 {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".csproj") {
            candidates.push(ModuleCandidate {
                relative_path: canonical_path_string(name.trim_end_matches(".csproj")),
                name: name.trim_end_matches(".csproj").to_owned(),
                kind: CityBuildingKind::Library,
                district: CityDistrictKind::App,
                language: "csharp".to_owned(),
            });
        } else {
            // Depth-2 .csproj search inside top-level directories.
            if let Ok(read) = fs::read_dir(path) {
                for entry in read.flatten().take(MAX_ENTRIES_PER_DIR) {
                    let child_name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                    if !child_name.ends_with(".csproj") {
                        continue;
                    }
                    if budget.expired() {
                        return Err(CityIslandUnavailableReason::ScanTimeout);
                    }
                    let Ok(meta) = entry.metadata() else { continue };
                    if meta.is_symlink() || !meta.is_file() {
                        continue;
                    }
                    let dir_relative = canonical_path_string(name);
                    let project_relative = canonical_path_string(&entry.file_name().to_string_lossy())
                        .trim_end_matches(".csproj")
                        .to_owned();
                    candidates.push(ModuleCandidate {
                        relative_path: format!("{dir_relative}/{project_relative}"),
                        name: entry
                            .file_name()
                            .to_string_lossy()
                            .trim_end_matches(".csproj")
                            .to_owned(),
                        kind: CityBuildingKind::Library,
                        district: CityDistrictKind::App,
                        language: "csharp".to_owned(),
                    });
                }
            }
        }
    }
    Ok(candidates)
}

fn fallback_module_candidates(
    root: &Path,
    budget: ScanBudget,
) -> Result<Vec<ModuleCandidate>, CityIslandUnavailableReason> {
    if budget.expired() {
        return Err(CityIslandUnavailableReason::ScanTimeout);
    }
    let mut candidates = Vec::new();
    for source_dir in FALLBACK_SOURCE_DIR_NAMES {
        let dir = root.join(source_dir);
        let Ok(meta) = fs::symlink_metadata(&dir) else { continue };
        if meta.is_symlink() || !meta.is_dir() {
            continue;
        }
        candidates.push(ModuleCandidate {
            relative_path: (*source_dir).to_owned(),
            name: (*source_dir).to_owned(),
            kind: CityBuildingKind::Module,
            district: CityDistrictKind::Other,
            language: "generic".to_owned(),
        });
    }
    Ok(candidates)
}

/// Single bounded walk over the repository collecting regular source files.
fn walk_source_files(
    dir: &Path,
    relative: &str,
    depth: usize,
    out: &mut Vec<WalkedFile>,
    budget: ScanBudget,
) -> Result<(), CityIslandUnavailableReason> {
    if depth > MAX_SCAN_DEPTH || out.len() >= MAX_FILES_PER_REPO {
        return Ok(());
    }
    let entries = bounded_sorted_entries(dir, budget)?;
    let dotnet_here = entries
        .0
        .iter()
        .any(|(name, _)| name.to_ascii_lowercase().ends_with(".sln"));
    let within_src = relative.split('/').any(|part| part == "src");
    for (name, path) in entries.0 {
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        if meta.is_symlink() {
            continue;
        }
        let child_relative = join_relative(relative, Path::new(&name).as_os_str());
        if meta.is_dir() {
            if is_skipped_dir_name(&name, dotnet_here, within_src) {
                continue;
            }
            walk_source_files(&path, &child_relative, depth + 1, out, budget)?;
        } else if meta.is_file() {
            if out.len() >= MAX_FILES_PER_REPO {
                return Ok(());
            }
            let extension = extension_of(&name);
            out.push(WalkedFile {
                relative_path: child_relative,
                absolute_path: path,
                extension,
            });
        }
        if budget.expired() {
            return Err(CityIslandUnavailableReason::ScanTimeout);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Archipelago orchestration
// ---------------------------------------------------------------------------

/// Inputs of the archipelago build; the scanner never mutates them.
pub struct CityMapInputs<'a> {
    /// Configured isolated Eva checkout (the capital), when set.
    pub eva_workspace_path: Option<&'a str>,
    /// Watched repository roots (absolute paths).
    pub watched_repositories: &'a [String],
    /// Previously published satellites keyed by public island id; reused when
    /// the scan budget is exhausted or the repository fingerprint is
    /// unchanged at the caller level.
    pub cached_satellites: &'a BTreeMap<String, CityIsland>,
}

/// Builds the full archipelago with the documented scan budgets.
pub fn build_archipelago(inputs: &CityMapInputs, now_ms: i64) -> HiveCityMapSnapshot {
    build_archipelago_with_budgets(
        inputs,
        now_ms,
        MAX_REPO_SCAN_DURATION,
        MAX_TOTAL_SCAN_DURATION,
    )
}

/// Testable variant with explicit budgets; the contract matches
/// [`build_archipelago`].
pub fn build_archipelago_with_budgets(
    inputs: &CityMapInputs,
    now_ms: i64,
    repo_budget: Duration,
    total_budget: Duration,
) -> HiveCityMapSnapshot {
    let started = Instant::now();
    // The capital is scanned first: its live scan is capped by the repo
    // budget, and any failure or timeout falls back to the embedded manifest.
    let capital = match inputs.eva_workspace_path {
        Some(raw) if !raw.trim().is_empty() => {
            let deadline = Instant::now() + repo_budget.min(total_budget);
            match scan_island(
                Path::new(raw),
                CAPITAL_ISLAND_ID.to_owned(),
                "EvoHime",
                CityIslandRole::Capital,
                Some(deadline),
            ) {
                Ok(island) => island,
                Err(_) => embedded_capital_island(),
            }
        }
        _ => embedded_capital_island(),
    };
    let _ = started;

    // Satellites: canonical order by public id, budget shared.
    let mut entries: Vec<(String, String, PathBuf)> = Vec::new();
    let mut seen_canonical: BTreeSet<String> = BTreeSet::new();
    for raw in inputs.watched_repositories {
        let canonical = resolve_repo_root(raw);
        if canonical.is_empty() || !seen_canonical.insert(canonical.clone()) {
            continue;
        }
        let id = satellite_id_for(&canonical);
        let name = last_segment(&canonical);
        entries.push((id, name, PathBuf::from(raw)));
    }
    entries.sort_by(|a, b| (a.0.as_str(), a.1.as_str()).cmp(&(b.0.as_str(), b.1.as_str())));
    entries.truncate(MAX_SATELLITES);

    let mut satellites = Vec::with_capacity(entries.len());
    for (id, name, root) in entries {
        let elapsed = started.elapsed();
        let remaining = total_budget.saturating_sub(elapsed);
        if remaining.is_zero() {
            satellites.push(
                inputs
                    .cached_satellites
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| unavailable_island(id, name, CityIslandRole::Satellite, CityIslandUnavailableReason::ScanTimeout)),
            );
            continue;
        }
        let budget = repo_budget.min(remaining);
        let deadline = Instant::now() + budget;
        match scan_island(&root, id.clone(), &name, CityIslandRole::Satellite, Some(deadline)) {
            Ok(island) => satellites.push(island),
            Err(reason) => satellites.push(
                inputs
                    .cached_satellites
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| unavailable_island(id, name, CityIslandRole::Satellite, reason)),
            ),
        }
    }
    assemble_snapshot(capital, satellites, now_ms)
}

/// Canonicalized repository root used for satellite identity: an fs-resolved
/// canonical path when possible, a normalized string otherwise.
fn resolve_repo_root(raw: &str) -> String {
    if raw.trim().is_empty() {
        return String::new();
    }
    match fs::canonicalize(raw) {
        Ok(path) => canonical_path_string(&path.to_string_lossy()),
        Err(_) => canonical_path_string(raw),
    }
}

/// Instantly builds the cold-start snapshot without touching the file system:
/// the embedded capital and no satellites. Used while the first background
/// scan of [`build_archipelago`] is still in flight.
pub fn offline_snapshot(now_ms: i64) -> HiveCityMapSnapshot {
    let capital = embedded_capital_island();
    HiveCityMapSnapshot {
        schema_version: HIVE_CITY_MAP_SCHEMA_VERSION,
        generated_at_ms: now_ms,
        revision_hash: compute_snapshot_revision_hash(&capital, &[]),
        capital,
        satellites: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().expect("parent exists in fixtures"))
            .expect("fixture mkdir");
        fs::write(path, contents).expect("fixture write");
    }

    #[test]
    fn fnv_is_deterministic_and_input_sensitive() {
        let a = fnv1a_128(b"crates/evohime-hive");
        let b = fnv1a_128(b"crates/evohime-hive");
        let c = fnv1a_128(b"crates/evohime-permissions");
        assert_eq!(a, b);
        assert_ne!(a, c);
        // FNV-1a 128 of the empty message equals the offset basis.
        assert_eq!(fnv1a_128(b""), 0x6c62272e_07bb0142_62b82175_6295c58d);
        assert_eq!(fnv_hex(fnv1a_128(b"")).len(), 32);
    }

    #[test]
    fn canonical_path_string_normalizes_separators_and_unc_prefix() {
        // Both expectations are lowercased on both sides so the test is
        // independent of the host file-system case sensitivity.
        assert_eq!(
            canonical_path_string(r"\\?\C:\Repo\Sub").to_ascii_lowercase(),
            "c:/repo/sub"
        );
        assert_eq!(canonical_path_string("/tmp/repo/"), "/tmp/repo");
        assert_eq!(canonical_path_string("a//b///c"), "a/b/c");
        // `..` is not resolved here by contract (it is forbidden downstream).
        assert!(canonical_path_string("a/b/../c").contains(".."));
    }

    #[test]
    fn satellite_id_is_stable_and_hex_shaped() {
        let id = satellite_id_for("d:/github/evohime");
        assert!(id.starts_with("sat-"));
        assert_eq!(id.len(), 4 + 16);
        // Case- and separator-insensitive identity on Windows.
        if cfg!(windows) {
            assert_eq!(
                satellite_id_for(&canonical_path_string("D:\\GitHub\\EvoHime")),
                satellite_id_for("d:/github/evohime")
            );
        }
        assert_ne!(
            satellite_id_for("d:/github/evohime"),
            satellite_id_for("d:/github/evohime-eva")
        );
    }

    #[test]
    fn parses_real_evohime_workspace_cargo_toml() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .expect("CARGO_MANIFEST_DIR is set by cargo");
        // CARGO_MANIFEST_DIR points at crates/evohime-hive; the repository
        // root is two ancestors above it.
        let repo_root = manifest_dir
            .ancestors()
            .nth(2)
            .expect("repo root ancestor");
        let bytes = fs::read(repo_root.join("Cargo.toml")).expect("root Cargo.toml exists");
        let manifest = parse_cargo_manifest_subset(&bytes);
        assert!(manifest.is_workspace);
        assert!(manifest
            .members
            .iter()
            .any(|member| member == "crates/evohime-hive"));
        assert!(manifest
            .members
            .iter()
            .any(|member| member == "crates/evohime-core"));
        assert!(manifest.members.len() >= 20);
    }

    #[test]
    fn parses_comments_strings_and_workspace_deps() {
        let sample = r#"
[package]
name = "demo-worker" # package name
description = "a = b # c"

[dependencies]
serde = { workspace = true }
tracing = { workspace = true, features = ["fmt"] }
desktop-bridge = { path = "../desktop-bridge" }
plain-dep = "1.2"

[dev-dependencies]
tempfile = { workspace = true }

[target.'cfg(windows)'.dependencies]
windows-svc = "0.1"

[workspace]
members = [
    "crates/alpha", # comment inside array
    "crates/beta",
]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
desktop-bridge = { path = "crates/desktop-bridge" }
"#;
        let manifest = parse_cargo_manifest_subset(sample.as_bytes());
        assert!(manifest.is_workspace);
        assert_eq!(manifest.package_name.as_deref(), Some("demo-worker"));
        assert_eq!(manifest.members.len(), 2);
        assert!(manifest.members.contains(&"crates/alpha".to_owned()));
        assert!(manifest
            .dependency_paths
            .iter()
            .any(|(name, reference)| name == "desktop-bridge"
                && reference == &CargoDependencyRef::Path("../desktop-bridge".to_owned())));
        assert!(manifest
            .dependency_paths
            .iter()
            .any(|(name, reference)| name == "serde" && *reference == CargoDependencyRef::Workspace));
        assert!(manifest
            .workspace_dependency_paths
            .iter()
            .any(|(name, path)| name == "desktop-bridge" && path == "crates/desktop-bridge"));
    }

    #[test]
    fn scans_rust_workspace_with_internal_dependency_edges() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path();
        let root_manifest = "\
[workspace]\nmembers = [\"crates/alpha\", \"crates/beta\", \"crates/gamma\"]\n\n[workspace.dependencies]\ngamma = { path = \"crates/gamma\" }\n";
        write_file(&root.join("Cargo.toml"), root_manifest);
        write_file(
            &root.join("crates/alpha/Cargo.toml"),
            "[package]\nname = \"alpha\"\n\n[dependencies]\nbeta = { path = \"../beta\" }\ngamma = { workspace = true }\n",
        );
        write_file(&root.join("crates/alpha/src/lib.rs"), "pub fn alpha() {}\n");
        write_file(&root.join("crates/beta/Cargo.toml"), "[package]\nname = \"beta\"\n");
        write_file(&root.join("crates/beta/src/lib.rs"), "pub fn beta() {}\n");
        write_file(&root.join("crates/gamma/Cargo.toml"), "[package]\nname = \"gamma\"\n");
        write_file(&root.join("crates/gamma/src/lib.rs"), "pub fn gamma() {}\n");

        let island = scan_island(
            root,
            "sat-abc".to_owned(),
            "alpha",
            CityIslandRole::Satellite,
            Some(Instant::now() + MAX_REPO_SCAN_DURATION),
        )
        .expect("scan succeeds");

        assert_eq!(island.ecosystem, CityEcosystem::Rust);
        assert_eq!(island.status, CityIslandStatus::Ready);
        assert_eq!(island.buildings.len(), 3);
        assert!(island.buildings.iter().all(|building| building.file_count >= 1));
        // alpha -> beta (path dep) and alpha -> gamma (workspace dep).
        assert_eq!(island.edges.len(), 2);
        let ab = island
            .edges
            .iter()
            .find(|edge| edge.from_id == "crates/alpha" && edge.to_id == "crates/beta")
            .expect("alpha->beta edge");
        assert_eq!(ab.kind, CityEdgeKind::Dependency);
        let ag = island
            .edges
            .iter()
            .find(|edge| edge.from_id == "crates/alpha" && edge.to_id == "crates/gamma")
            .expect("alpha->gamma edge");
        assert_eq!(ag.kind, CityEdgeKind::Dependency);
        validate_island(&island).expect("island valid");
    }

    #[test]
    fn scanning_is_deterministic() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path();
        write_file(&root.join("Cargo.toml"), "[package]\nname = \"loan\"\n");
        write_file(&root.join("src/lib.rs"), "pub struct Loan;\n");
        write_file(&root.join("src/main.rs"), "fn main() {}\n");
        let run = |label: &str| {
            scan_island(
                root,
                "sat-det".to_owned(),
                label,
                CityIslandRole::Satellite,
                Some(Instant::now() + MAX_REPO_SCAN_DURATION),
            )
            .expect("scan succeeds")
        };
        let island_a = run("loan");
        let island_b = run("loan");
        assert_eq!(island_a, island_b);
        assert!(!island_a.structure_hash.is_empty());
        assert!(!island_a.revision_hash.is_empty());
        assert!(island_a.accent_hue <= 360);
        // Root single-package building is always present.
        assert_eq!(island_a.buildings.len(), 1);
        assert_eq!(island_a.buildings[0].id, ".");
        assert_eq!(island_a.buildings[0].file_count, 3);
    }

    #[test]
    fn scans_node_monorepo_with_internal_dep_edges() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path();
        write_file(
            &root.join("package.json"),
            r#"{"name":"monorepo","workspaces":["*"]}"#,
        );
        write_file(&root.join("src/main.ts"), "console.log('hi');\n");
        write_file(
            &root.join("shared/package.json"),
            r#"{"name":"shared-lib"}"#,
        );
        write_file(&root.join("shared/index.js"), "export {};\n");
        // `apps` is the depth-1 package that declares the dependency.
        let apps_dir = root.join("apps");
        fs::create_dir_all(&apps_dir).expect("mkdir");
        write_file(
            &apps_dir.join("package.json"),
            r#"{"name":"client","dependencies":{"shared-lib":"1.0.0"}}"#,
        );

        let island = scan_island(
            root,
            "sat-node".to_owned(),
            "monorepo",
            CityIslandRole::Satellite,
            Some(Instant::now() + MAX_REPO_SCAN_DURATION),
        )
        .expect("scan succeeds");
        assert_eq!(island.ecosystem, CityEcosystem::Node);
        // Depth-1 packages become module buildings.
        let ids: Vec<&str> = island.buildings.iter().map(|b| b.id.as_str()).collect();
        assert!(ids.contains(&"shared"));
        assert!(ids.contains(&"apps"));
        // apps -> shared via the declared dependency name.
        assert_eq!(island.edges.len(), 1);
        assert_eq!(island.edges[0].from_id, "apps");
        assert_eq!(island.edges[0].to_id, "shared");
    }

    #[test]
    fn falls_back_to_top_level_dirs_without_manifests() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path();
        write_file(&root.join("src/util.py"), "print('x')\n");
        write_file(&root.join("app/main.go"), "package main\n");
        let island = scan_island(
            root,
            "sat-gen".to_owned(),
            "generic",
            CityIslandRole::Satellite,
            Some(Instant::now() + MAX_REPO_SCAN_DURATION),
        )
        .expect("scan succeeds");
        assert_eq!(island.ecosystem, CityEcosystem::Generic);
        let ids: Vec<&str> = island.buildings.iter().map(|b| b.id.as_str()).collect();
        assert!(ids.contains(&"src"));
        assert!(ids.contains(&"app"));
    }

    #[test]
    fn embedded_capital_matches_real_crate_names() {
        let capital = embedded_capital_island();
        assert_eq!(capital.id, CAPITAL_ISLAND_ID);
        assert_eq!(capital.role, CityIslandRole::Capital);
        assert_eq!(capital.source, CityIslandSource::EmbeddedManifest);
        assert_eq!(capital.ecosystem, CityEcosystem::Mixed);
        assert!(capital
            .buildings
            .iter()
            .any(|building| building.id == "crates/evohime-core"
                && building.kind == CityBuildingKind::Citadel));
        assert!(capital
            .buildings
            .iter()
            .any(|building| building.id == "desktop/evohime-electron"
                && building.kind == CityBuildingKind::Ui));
        assert!(capital.buildings.len() <= MAX_BUILDINGS_PER_ISLAND);
        assert!(capital
            .edges
            .iter()
            .any(|edge| edge.from_id == "desktop/evohime-electron"
                && edge.to_id == "crates/desktop-ipc"));
        validate_island(&capital).expect("embedded capital valid");
    }

    #[test]
    fn live_capital_falls_back_to_embedded_when_path_is_missing() {
        let inputs = CityMapInputs {
            eva_workspace_path: Some("Z:\\definitely\\missing\\eva-workspace"),
            watched_repositories: &[],
            cached_satellites: &BTreeMap::new(),
        };
        let snapshot = build_archipelago(&inputs, 42);
        assert_eq!(snapshot.capital.source, CityIslandSource::EmbeddedManifest);
        assert_eq!(snapshot.capital.status, CityIslandStatus::Ready);
        assert!(snapshot.satellites.is_empty());
        assert_eq!(snapshot.generated_at_ms, 42);
    }

    #[test]
    fn watched_repository_without_disk_becomes_unavailable_island() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let live = tmp.path().join("live-repo");
        write_file(&live.join("src/lib.rs"), "pub fn x() {}\n");
        let live_path = live.to_string_lossy().into_owned();
        let inputs = CityMapInputs {
            eva_workspace_path: Some(live_path.as_str()),
            watched_repositories: &["C:\\definitely\\missing\\satellite".to_owned()],
            cached_satellites: &BTreeMap::new(),
        };
        let snapshot = build_archipelago(&inputs, 7);
        assert_eq!(snapshot.satellites.len(), 1);
        let satellite = &snapshot.satellites[0];
        assert_eq!(satellite.status, CityIslandStatus::Unavailable);
        assert_eq!(
            satellite.reason,
            Some(CityIslandUnavailableReason::RepoNotFound)
        );
        assert!(satellite.buildings.is_empty());
        assert_eq!(snapshot.capital.source, CityIslandSource::LiveWorkspace);
    }

    #[test]
    fn satellites_are_capped_and_sorted_canonically() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let mut repos: Vec<String> = Vec::new();
        for index in 0..(MAX_SATELLITES + 4) {
            let dir = tmp.path().join(format!("repo_{index:02}"));
            write_file(&dir.join("src/lib.rs"), "pub fn f() {}\n");
            repos.push(dir.to_string_lossy().into_owned());
        }
        repos.reverse();
        let inputs = CityMapInputs {
            eva_workspace_path: None,
            watched_repositories: &repos,
            cached_satellites: &BTreeMap::new(),
        };
        let snapshot = build_archipelago(&inputs, 0);
        assert_eq!(snapshot.satellites.len(), MAX_SATELLITES);
        let mut sorted = snapshot.satellites.clone();
        sorted.sort_by(|a, b| {
            (a.id.as_str(), a.name.as_str()).cmp(&(b.id.as_str(), b.name.as_str()))
        });
        assert_eq!(snapshot.satellites, sorted);
    }

    #[test]
    fn budget_exhaustion_uses_cache_then_reports_timeout() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let repo = tmp.path().join("repo");
        write_file(&repo.join("src/lib.rs"), "pub fn r() {}\n");
        let canonical = resolve_repo_root(&repo.to_string_lossy());
        let cached_id = satellite_id_for(&canonical);
        let mut cached_island = embedded_capital_island();
        cached_island.id = cached_id.clone();
        cached_island.role = CityIslandRole::Satellite;
        cached_island.name = "repo".to_owned();
        let cached_island = prepare_island(cached_island);
        let mut cached = BTreeMap::new();
        cached.insert(cached_id.clone(), cached_island);
        // Total budget exactly zero: the satellite is served from the cache.
        let inputs_cached = CityMapInputs {
            eva_workspace_path: None,
            watched_repositories: &[repo.to_string_lossy().into_owned()],
            cached_satellites: &cached,
        };
        let snapshot = build_archipelago_with_budgets(
            &inputs_cached,
            0,
            MAX_REPO_SCAN_DURATION,
            Duration::ZERO,
        );
        assert_eq!(snapshot.satellites.len(), 1);
        assert_eq!(snapshot.satellites[0].id, cached_id);
        assert_eq!(snapshot.satellites[0].source, CityIslandSource::EmbeddedManifest);
        // Without cache the timeout reason is explicit.
        let inputs_no_cache = CityMapInputs {
            eva_workspace_path: None,
            watched_repositories: &[repo.to_string_lossy().into_owned()],
            cached_satellites: &BTreeMap::new(),
        };
        let snapshot = build_archipelago_with_budgets(
            &inputs_no_cache,
            0,
            MAX_REPO_SCAN_DURATION,
            Duration::ZERO,
        );
        assert_eq!(
            snapshot.satellites[0].reason,
            Some(CityIslandUnavailableReason::ScanTimeout)
        );
    }

    #[test]
    fn worst_case_full_archipelago_fits_payload_budget() {
        // Maximal nominal configuration: 12 satellites x 48 buildings with
        // saturated edge graphs each.
        let mut satellites = Vec::new();
        for satellite_index in 0..MAX_SATELLITES {
            let mut buildings = Vec::new();
            for building_index in 0..MAX_BUILDINGS_PER_ISLAND {
                let building_id = format!(
                    "services/area-{satellite_index:02}/module-{building_index:03}-with-a-long-descriptive-name"
                );
                buildings.push(CityBuilding {
                    id: building_id.clone(),
                    name: format!("service module {building_index:03} display name"),
                    district_id: "core".to_owned(),
                    kind: CityBuildingKind::Module,
                    relative_path: building_id,
                    file_count: u32::try_from(building_index + 1).unwrap_or(1),
                    line_count: u64::try_from(10_000 + building_index).unwrap_or(10_000),
                    height_weight_bp: 500,
                    language: "rust".to_owned(),
                });
            }
            let mut edges = Vec::new();
            for edge_index in 0..MAX_EDGES_PER_ISLAND {
                let from_id = buildings[edge_index % buildings.len()].id.clone();
                let to_id = buildings[(edge_index + 7) % buildings.len()].id.clone();
                edges.push(CityEdge {
                    from_id,
                    to_id,
                    kind: CityEdgeKind::Dependency,
                    weight: 80,
                });
            }
            let island = CityIsland {
                id: format!("sat-{satellite_index:016x}"),
                name: format!("satellite city {satellite_index:02} with a long display name"),
                role: CityIslandRole::Satellite,
                source: CityIslandSource::LiveWorkspace,
                status: CityIslandStatus::Ready,
                reason: None,
                ecosystem: CityEcosystem::Generic,
                structure_hash: String::new(),
                revision_hash: String::new(),
                accent_hue: 0,
                districts: vec![CityDistrict {
                    id: "core".to_owned(),
                    label: "Core".to_owned(),
                    kind: CityDistrictKind::Core,
                }],
                buildings,
                edges,
                total_files: 4_000,
                total_lines: 120_000,
            };
            satellites.push(prepare_island(island));
        }
        let snapshot = assemble_snapshot(embedded_capital_island(), satellites, 0);
        assert!(!snapshot.capital.buildings.is_empty());
        assert!(serialized_len(&snapshot) <= MAX_CITY_MAP_PAYLOAD_BYTES);
    }

    #[test]
    fn truncation_keeps_citadels_and_drops_low_weight_modules_first() {
        let mut satellites = Vec::new();
        for satellite_index in 0..MAX_SATELLITES {
            let mut buildings = Vec::new();
            buildings.push(CityBuilding {
                id: "crates/citadel".to_owned(),
                name: "citadel".to_owned(),
                district_id: "core".to_owned(),
                kind: CityBuildingKind::Citadel,
                relative_path: "crates/citadel".to_owned(),
                file_count: 900,
                line_count: 900_000,
                height_weight_bp: 1000,
                language: "rust".to_owned(),
            });
            for building_index in 0..150 {
                let id = format!("mod/{satellite_index:02}/{building_index:03}-module-row");
                buildings.push(CityBuilding {
                    id: id.clone(),
                    name: format!("module {building_index:03} of satellite {satellite_index:02}"),
                    district_id: "core".to_owned(),
                    kind: CityBuildingKind::Module,
                    relative_path: id,
                    file_count: 1,
                    line_count: u64::try_from(building_index).unwrap_or(0),
                    height_weight_bp: 150,
                    language: "rust".to_owned(),
                });
            }
            let island = CityIsland {
                id: format!("sat-{satellite_index:016x}"),
                name: format!("satellite {satellite_index:02}"),
                role: CityIslandRole::Satellite,
                source: CityIslandSource::LiveWorkspace,
                status: CityIslandStatus::Ready,
                reason: None,
                ecosystem: CityEcosystem::Generic,
                structure_hash: String::new(),
                revision_hash: String::new(),
                accent_hue: 0,
                districts: vec![CityDistrict {
                    id: "core".to_owned(),
                    label: "Core".to_owned(),
                    kind: CityDistrictKind::Core,
                }],
                buildings,
                edges: Vec::new(),
                total_files: 151,
                total_lines: 900_000,
            };
            satellites.push(prepare_island(island));
        }
        let total_before: usize = satellites.iter().map(|island| island.buildings.len()).sum();
        let snapshot = assemble_snapshot(embedded_capital_island(), satellites, 0);
        assert!(serialized_len(&snapshot) <= MAX_CITY_MAP_PAYLOAD_BYTES);
        assert!(snapshot.satellites.iter().all(|island| island
            .buildings
            .iter()
            .any(|building| building.kind == CityBuildingKind::Citadel)));
        let total_after: usize = snapshot
            .satellites
            .iter()
            .map(|island| island.buildings.len())
            .sum();
        assert!(total_after < total_before, "modules were pruned");
    }

    #[test]
    fn service_directories_are_skipped_at_any_depth() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path();
        write_file(&root.join("Cargo.toml"), "[package]\nname = \"r\"\n");
        write_file(&root.join("src/lib.rs"), "pub fn r() {}\n");
        write_file(&root.join("src/bin/daemon.rs"), "fn main() {}\n");
        write_file(&root.join("target/debug/junk.rs"), "junk\n");
        write_file(&root.join("node_modules/pkg/index.js"), "x\n");
        write_file(&root.join(".evohime/state.json"), "{}\n");
        write_file(&root.join(".evohime-cargo/registry/junk.txt"), "junk\n");
        write_file(&root.join(".git/config"), "[core]\n");
        let island = scan_island(
            root,
            "sat-skip".to_owned(),
            "r",
            CityIslandRole::Satellite,
            Some(Instant::now() + MAX_REPO_SCAN_DURATION),
        )
        .expect("scan succeeds");
        // Cargo.toml + src/lib.rs + src/bin/daemon.rs; target, node_modules,
        // dot-directories are skipped; Rust src/bin stays visible.
        assert_eq!(island.total_files, 3);
        assert_eq!(island.buildings[0].file_count, 3);
        validate_island(&island).expect("island valid");
    }

    #[test]
    fn fingerprint_reflects_manifest_changes() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path();
        write_file(&root.join("Cargo.toml"), "[package]\nname = \"r\"\n");
        write_file(&root.join("src/lib.rs"), "pub fn r() {}\n");
        let fingerprint_a = repository_fingerprint(root).expect("fingerprint");
        let fingerprint_b = repository_fingerprint(root).expect("fingerprint");
        assert_eq!(fingerprint_a, fingerprint_b);
        write_file(&root.join("package.json"), "{}\n");
        let fingerprint_c = repository_fingerprint(root).expect("fingerprint");
        assert_ne!(fingerprint_a, fingerprint_c);
    }

    #[test]
    fn snapshot_roundtrips_and_rejects_unknown_fields() {
        let snapshot = offline_snapshot(12_345);
        let bytes = serde_json::to_vec(&snapshot).expect("serialize");
        let back: HiveCityMapSnapshot = serde_json::from_slice(&bytes).expect("roundtrip");
        assert_eq!(back, snapshot);
        assert_eq!(back.schema_version, HIVE_CITY_MAP_SCHEMA_VERSION);
        // deny_unknown_fields on the top-level snapshot.
        let mut enemy = bytes.clone();
        enemy.truncate(enemy.len() - 1);
        enemy.extend_from_slice(b",\"injected\":1}");
        let result: Result<HiveCityMapSnapshot, _> = serde_json::from_slice(&enemy);
        assert!(result.is_err());
    }

    #[test]
    fn validation_rejects_self_edges_and_dangling_references() {
        let mut island = embedded_capital_island();
        let first = island.buildings[0].id.clone();
        island.edges.push(CityEdge {
            from_id: first.clone(),
            to_id: first,
            kind: CityEdgeKind::Dependency,
            weight: 10,
        });
        assert!(validate_island(&island).is_err());
        island.edges.clear();
        island.edges.push(CityEdge {
            from_id: "ghost".to_owned(),
            to_id: island.buildings[0].id.clone(),
            kind: CityEdgeKind::Ipc,
            weight: 10,
        });
        assert!(validate_island(&island).is_err());
    }

    #[test]
    fn schema_validates_ranges() {
        let mut island = embedded_capital_island();
        island.accent_hue = 361;
        assert!(validate_island(&island).is_err());
        island.accent_hue = 0;
        let core = island
            .buildings
            .iter_mut()
            .find(|building| building.id == "crates/evohime-core")
            .expect("core building");
        core.height_weight_bp = 149;
        assert!(validate_island(&island).is_err());
    }

    #[test]
    fn schema_json_field_names_are_snake_case() {
        let snapshot = offline_snapshot(1);
        let json = serde_json::to_value(&snapshot).expect("json value");
        assert!(json.get("schema_version").is_some());
        assert!(json.get("generated_at_ms").is_some());
        assert!(json.get("revision_hash").is_some());
        assert!(json.get("capital").is_some());
        assert!(json.get("satellites").is_some());
        let capital = json.get("capital").expect("capital object");
        assert!(capital.get("structure_hash").is_some());
        assert!(capital.get("accent_hue").is_some());
        assert!(capital.get("reason").is_some());
        assert!(capital.get("total_lines").is_some());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_skipped() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path().join("repo");
        let outside = tmp.path().join("outside");
        write_file(&root.join("Cargo.toml"), "[package]\nname = \"r\"\n");
        write_file(&root.join("src/lib.rs"), "pub fn r() {}\n");
        write_file(&outside.join("secret.rs"), "SECRET\n");
        symlink(&outside, root.join("linked-dir")).expect("symlink");
        symlink(&outside.join("secret.rs"), root.join("linked-file.rs")).expect("symlink");
        let island = scan_island(
            &root,
            "sat-sym".to_owned(),
            "r",
            CityIslandRole::Satellite,
            Some(Instant::now() + MAX_REPO_SCAN_DURATION),
        )
        .expect("scan succeeds");
        assert_eq!(island.total_files, 2);
        // No leaked absolute paths or symlink names.
        assert!(island
            .buildings
            .iter()
            .all(|building| !building.relative_path.contains("linked")));
    }
}
