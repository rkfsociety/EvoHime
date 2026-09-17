use serde::{
    de::{Deserializer, Error as DeError},
    Deserialize, Serialize,
};
use sha2::Digest;
use std::cmp::Ordering;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub const RECOVERY_SCHEMA: u32 = 1;
pub const MAX_RECOVERY_BYTES: usize = 32 * 1024;
pub const MAX_RECOVERY_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryJournal {
    pub schema: u32,
    pub operation_id: String,
    pub phase: String,
    pub active_slot: String,
    pub active_version: String,
    pub active_sha256: String,
    pub fallback_available: bool,
    pub retry_count: u32,
    pub reason_code: Option<String>,
}

impl RecoveryJournal {
    pub fn new(operation_id: impl Into<String>, phase: impl Into<String>) -> Self {
        Self {
            schema: RECOVERY_SCHEMA,
            operation_id: operation_id.into(),
            phase: phase.into(),
            active_slot: "active".into(),
            active_version: String::new(),
            active_sha256: String::new(),
            fallback_available: false,
            retry_count: 0,
            reason_code: None,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != RECOVERY_SCHEMA
            || self.operation_id.is_empty()
            || self.operation_id.len() > 128
            || !matches!(
                self.phase.as_str(),
                "prepared"
                    | "downloaded"
                    | "verified"
                    | "replaced"
                    | "self-tested"
                    | "committed"
                    | "rolled-back"
                    | "manual-recovery"
            )
            || !matches!(self.active_slot.as_str(), "active" | "fallback")
            || self.active_version.len() > 64
            || self.active_sha256.len() > 64
            || self.retry_count > MAX_RECOVERY_ATTEMPTS
        {
            return Err("invalid recovery journal".into());
        }
        if !self.active_sha256.is_empty()
            && (self.active_sha256.len() != 64
                || !self.active_sha256.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("invalid recovery hash".into());
        }
        Ok(())
    }
}

pub fn write_recovery_journal(path: &Path, journal: &RecoveryJournal) -> Result<(), String> {
    journal.validate()?;
    let bytes = serde_json::to_vec(journal).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_RECOVERY_BYTES {
        return Err("recovery journal exceeds bounds".into());
    }
    let temporary = path.with_extension("json.tmp");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let result = (|| {
        use std::io::Write;
        let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub fn read_recovery_journal(path: &Path) -> Result<Option<RecoveryJournal>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_RECOVERY_BYTES {
        return Err("recovery journal exceeds bounds".into());
    }
    let journal: RecoveryJournal = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    journal.validate()?;
    Ok(Some(journal))
}

pub fn validate_pe_artifact(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), String> {
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file()
        || metadata.len() != expected_size
        || expected_size == 0
        || expected_size > 1024 * 1024 * 1024
    {
        return Err("artifact size mismatch".into());
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    validate_pe_headers(&mut file, metadata.len())?;
    let mut digest = sha2::Sha256::new();
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let actual = format!("{:x}", digest.finalize());
    if actual != expected_sha256.to_ascii_lowercase() {
        return Err("artifact hash mismatch".into());
    }
    Ok(())
}

/// Validate the executable format before a downloaded worker can be launched.
/// Checking only the DOS `MZ` marker lets a truncated or malformed PE reach
/// Windows, which reports it as an unsupported 16-bit application.
pub fn validate_pe_image(path: &Path) -> Result<(), String> {
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 1024 * 1024 * 1024 {
        return Err("artifact size is outside bounds".into());
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    validate_pe_headers(&mut file, metadata.len())
}

fn validate_pe_headers(file: &mut std::fs::File, file_len: u64) -> Result<(), String> {
    const DOS_HEADER_SIZE: u64 = 64;
    const COFF_HEADER_SIZE: u64 = 24;
    if file_len < DOS_HEADER_SIZE {
        return Err("artifact is not a PE executable".into());
    }
    let mut dos_header = [0u8; DOS_HEADER_SIZE as usize];
    file.read_exact(&mut dos_header)
        .map_err(|e| e.to_string())?;
    if dos_header[..2] != *b"MZ" {
        return Err("artifact is not a PE executable".into());
    }
    let pe_offset = u32::from_le_bytes(
        dos_header[0x3c..0x40]
            .try_into()
            .expect("DOS header slice has fixed size"),
    ) as u64;
    let pe_end = pe_offset
        .checked_add(COFF_HEADER_SIZE)
        .ok_or_else(|| "artifact PE header offset overflowed".to_owned())?;
    if pe_offset < DOS_HEADER_SIZE || pe_end > file_len {
        return Err("artifact PE header is truncated".into());
    }
    file.seek(SeekFrom::Start(pe_offset))
        .map_err(|e| e.to_string())?;
    let mut coff = [0u8; COFF_HEADER_SIZE as usize];
    file.read_exact(&mut coff).map_err(|e| e.to_string())?;
    if coff[..4] != *b"PE\0\0" {
        return Err("artifact PE signature is invalid".into());
    }
    let machine = u16::from_le_bytes([coff[4], coff[5]]);
    if machine != 0x8664 {
        return Err("artifact is not an x64 PE executable".into());
    }
    let optional_header_size = u16::from_le_bytes([coff[20], coff[21]]) as u64;
    let optional_end = pe_end
        .checked_add(optional_header_size)
        .ok_or_else(|| "artifact optional header offset overflowed".to_owned())?;
    if optional_header_size < 2 || optional_end > file_len {
        return Err("artifact PE optional header is truncated".into());
    }
    let mut optional_magic = [0u8; 2];
    file.read_exact(&mut optional_magic)
        .map_err(|e| e.to_string())?;
    if u16::from_le_bytes(optional_magic) != 0x20b {
        return Err("artifact is not a PE32+ executable".into());
    }
    Ok(())
}

#[cfg(test)]
mod recovery_tests {
    use super::*;

    #[test]
    fn journal_round_trips_atomically_and_rejects_unknown_phase() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("recovery.json");
        let journal = RecoveryJournal::new("op-1", "prepared");
        write_recovery_journal(&path, &journal).unwrap();
        assert_eq!(read_recovery_journal(&path).unwrap(), Some(journal));
        std::fs::write(&path, br#"{"schema":1,"operation_id":"x","phase":"unsafe","active_slot":"active","active_version":"","active_sha256":"","fallback_available":false,"retry_count":0,"reason_code":null}"#).unwrap();
        assert!(read_recovery_journal(&path).is_err());
    }

    #[test]
    fn pe_validation_checks_header_size_and_hash() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("updater.exe");
        let mut payload = vec![0u8; 0x5a];
        payload[..2].copy_from_slice(b"MZ");
        payload[0x3c..0x40].copy_from_slice(&(0x40u32).to_le_bytes());
        payload[0x40..0x44].copy_from_slice(b"PE\0\0");
        payload[0x44..0x46].copy_from_slice(&0x8664u16.to_le_bytes());
        payload[0x54..0x56].copy_from_slice(&2u16.to_le_bytes());
        payload[0x58..0x5a].copy_from_slice(&0x20bu16.to_le_bytes());
        std::fs::write(&path, &payload).unwrap();
        let hash = format!("{:x}", sha2::Sha256::digest(&payload));
        validate_pe_artifact(&path, payload.len() as u64, &hash).unwrap();
        assert!(validate_pe_artifact(&path, payload.len() as u64 - 1, &hash).is_err());
        assert!(validate_pe_artifact(&path, 9, &"00".repeat(32)).is_err());
    }

    #[test]
    fn pe_validation_rejects_dos_only_payload() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("updater.exe");
        std::fs::write(&path, b"MZpayload").unwrap();
        assert!(validate_pe_image(&path).is_err());
    }

    #[test]
    fn journal_size_and_retry_limits_are_fail_closed() {
        let mut journal = RecoveryJournal::new("x", "prepared");
        journal.retry_count = MAX_RECOVERY_ATTEMPTS + 1;
        assert!(journal.validate().is_err());
        journal.retry_count = 0;
        journal.operation_id = "x".repeat(129);
        assert!(journal.validate().is_err());
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ModuleRecord {
    pub id: String,
    pub version: String,
    #[serde(default, deserialize_with = "deserialize_nullable_vec")]
    pub dependencies: Vec<String>,
}

pub fn deserialize_nullable_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::String(value) => Ok(vec![value]),
        serde_json::Value::Array(value) => serde_json::from_value(serde_json::Value::Array(value))
            .map_err(|error| D::Error::custom(format!("expected string array: {error}"))),
        value => Err(D::Error::custom(format!(
            "expected null, string, or string array, got {value}"
        ))),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct InstalledManifest {
    pub components: Vec<ModuleRecord>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct UpdatePlan {
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdaterStatus {
    pub schema: &'static str,
    pub phase: &'static str,
    pub message: String,
    pub error: Option<String>,
    pub modules: Vec<String>,
    pub available: Vec<UpdaterModuleStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery: Option<RecoveryStatus>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecoveryStatus {
    pub phase: String,
    pub active_slot: String,
    pub active_version: String,
    pub fallback_available: bool,
    pub retry_count: u32,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdaterModuleStatus {
    pub module: String,
    pub installed: String,
    pub available: String,
    pub summary: String,
    pub changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateCandidate {
    pub module: String,
    pub installed: String,
    pub available: String,
    pub summary: String,
    pub changes: Vec<String>,
    pub dependencies: Vec<String>,
    pub restart: String,
    pub artifact: String,
    pub size: u64,
    pub sha256: String,
    pub download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstalledComponent {
    pub id: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComponentManifest {
    pub components: Vec<InstalledComponent>,
}

const MAX_AVAILABLE_MODULES: usize = 64;
const MAX_MODULE_DEPENDENCIES: usize = 64;
const MAX_COMPONENT_MANIFEST_FIELD_BYTES: usize = 260;
const MAX_COMPONENT_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;

/// Validate the immutable package manifest before starting any product process.
/// A mismatch is fatal: running a partially replaced installation would make
/// dependency and rollback guarantees impossible.
pub fn validate_component_manifest(
    manifest: &ComponentManifest,
    install_dir: &Path,
) -> Result<(), String> {
    if manifest.components.is_empty() || manifest.components.len() > MAX_AVAILABLE_MODULES {
        return Err("component manifest count is outside bounds".into());
    }
    let mut ids = std::collections::HashSet::with_capacity(manifest.components.len());
    let mut paths = std::collections::HashSet::with_capacity(manifest.components.len());
    for component in &manifest.components {
        if component.id.is_empty()
            || component.id.len() > 64
            || !ids.insert(component.id.as_str())
            || component.path.is_empty()
            || component.path.len() > MAX_COMPONENT_MANIFEST_FIELD_BYTES
            || !paths.insert(component.path.as_str())
            || component.path.contains("..")
            || component.path.contains('\\')
            || component.path.contains(':')
            || component.path.starts_with('/')
            || Path::new(&component.path).is_absolute()
            || component.size == 0
            || component.size > MAX_COMPONENT_ARTIFACT_BYTES
            || component.sha256.len() != 64
            || !component
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!("invalid path for {}", component.id));
        }
        let path = install_dir.join(&component.path);
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("{}: {error}", component.id))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(format!(
                "component is not a regular file for {}",
                component.id
            ));
        }
        if metadata.len() != component.size {
            return Err(format!("size mismatch for {}", component.id));
        }
        let file =
            std::fs::File::open(&path).map_err(|error| format!("{}: {error}", component.id))?;
        if !hash_reader_matches(file, component.size, &component.sha256)
            .map_err(|error| format!("{}: {error}", component.id))?
        {
            return Err(format!("sha256 mismatch for {}", component.id));
        }
    }
    Ok(())
}

fn hash_reader_matches<R: Read>(
    mut reader: R,
    expected_size: u64,
    expected_sha256: &str,
) -> std::io::Result<bool> {
    let mut digest = sha2::Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > expected_size {
            return Ok(false);
        }
        digest.update(&buffer[..read]);
    }
    Ok(total == expected_size
        && digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
            == expected_sha256.to_ascii_lowercase())
}

pub fn select_outdated(
    installed: &InstalledManifest,
    available: &[ModuleRecord],
) -> Result<UpdatePlan, String> {
    if installed.components.len() > MAX_AVAILABLE_MODULES {
        return Err("installed module list is too large".into());
    }
    let mut installed_ids = std::collections::HashSet::with_capacity(installed.components.len());
    for item in &installed.components {
        if item.id.is_empty() || item.id.len() > 64 || !installed_ids.insert(item.id.as_str()) {
            return Err(format!(
                "duplicate or invalid installed module id: {}",
                item.id
            ));
        }
        if !is_valid_semver(&item.version) {
            return Err(format!("invalid installed version for {}", item.id));
        }
    }
    if available.len() > MAX_AVAILABLE_MODULES {
        return Err("available module list is too large".into());
    }
    let mut available_ids = std::collections::HashSet::with_capacity(available.len());
    for item in available {
        if item.id.is_empty() || item.id.len() > 64 || !available_ids.insert(item.id.as_str()) {
            return Err(format!("duplicate or invalid module id: {}", item.id));
        }
        if !is_valid_semver(&item.version) {
            return Err(format!("invalid version for {}", item.id));
        }
        let mut dependencies = std::collections::HashSet::with_capacity(item.dependencies.len());
        if item.dependencies.len() > MAX_MODULE_DEPENDENCIES
            || item.dependencies.iter().any(|dependency| {
                dependency.is_empty()
                    || dependency.len() > 64
                    || !dependencies.insert(dependency.as_str())
                    || dependency == &item.id
            })
        {
            return Err(format!("invalid dependencies for {}", item.id));
        }
    }
    for item in available {
        for dependency in &item.dependencies {
            if !available_ids.contains(dependency.as_str()) {
                return Err(format!("dependency {dependency} is not published"));
            }
        }
    }
    let mut unresolved = available_ids.clone();
    while !unresolved.is_empty() {
        let resolved = available
            .iter()
            .filter(|item| {
                unresolved.contains(item.id.as_str())
                    && item
                        .dependencies
                        .iter()
                        .all(|dependency| !unresolved.contains(dependency.as_str()))
            })
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        if resolved.is_empty() {
            return Err("cycle in available module dependencies".into());
        }
        for id in resolved {
            unresolved.remove(id);
        }
    }
    let current = installed
        .components
        .iter()
        .map(|item| (item.id.as_str(), item.version.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut selected = std::collections::BTreeSet::new();
    for item in available {
        if current
            .get(item.id.as_str())
            .is_none_or(|version| compare_semver(version, &item.version) == Ordering::Less)
        {
            selected.insert(item.id.clone());
        }
    }
    // Подтягиваем зависимости самого обновляемого модуля. Нельзя выбирать
    // все модули, которые зависят от него: это вызовет лишние перезапуски.
    let mut changed = true;
    while changed {
        changed = false;
        let dependencies = available
            .iter()
            .filter(|item| selected.contains(&item.id))
            .flat_map(|item| item.dependencies.iter())
            .cloned()
            .collect::<Vec<_>>();
        for dependency in dependencies {
            let Some(item) = available.iter().find(|item| item.id == dependency) else {
                return Err(format!("dependency {dependency} is not published"));
            };
            let installed_version = current.get(item.id.as_str()).copied().unwrap_or("0.0.0");
            if compare_semver(installed_version, &item.version) == Ordering::Less
                && selected.insert(item.id.clone())
            {
                changed = true;
            }
        }
    }
    Ok(UpdatePlan {
        modules: available
            .iter()
            .filter(|item| selected.contains(&item.id))
            .map(|item| item.id.clone())
            .collect(),
    })
}

pub fn compare_semver(left: &str, right: &str) -> Ordering {
    let a = parse_semver(left).expect("validated semver");
    let b = parse_semver(right).expect("validated semver");
    a.cmp(&b)
}

pub fn is_valid_semver(value: &str) -> bool {
    parse_semver(value).is_some()
}

fn parse_semver(value: &str) -> Option<[u64; 3]> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return None;
    }
    Some([
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn compares_versions_numerically() {
        assert_eq!(compare_semver("1.10.0", "1.9.0"), Ordering::Greater);
    }

    #[test]
    fn selects_only_outdated_and_dependents() {
        let installed = InstalledManifest {
            components: vec![
                ModuleRecord {
                    id: "core".into(),
                    version: "1.0.0".into(),
                    dependencies: vec![],
                },
                ModuleRecord {
                    id: "ui".into(),
                    version: "2.0.0".into(),
                    dependencies: vec![],
                },
            ],
        };
        let available = vec![
            ModuleRecord {
                id: "core".into(),
                version: "1.1.0".into(),
                dependencies: vec![],
            },
            ModuleRecord {
                id: "ui".into(),
                version: "2.0.0".into(),
                dependencies: vec![],
            },
            ModuleRecord {
                id: "shell".into(),
                version: "1.0.0".into(),
                dependencies: vec!["core".into()],
            },
        ];
        assert_eq!(
            select_outdated(&installed, &available).unwrap().modules,
            vec!["core", "shell"]
        );
    }

    #[test]
    fn selects_outdated_dependencies_without_selecting_dependents() {
        let installed = InstalledManifest {
            components: vec![
                ModuleRecord {
                    id: "core".into(),
                    version: "1.0.0".into(),
                    dependencies: vec![],
                },
                ModuleRecord {
                    id: "shell".into(),
                    version: "1.0.0".into(),
                    dependencies: vec!["core".into()],
                },
            ],
        };
        let available = vec![
            ModuleRecord {
                id: "core".into(),
                version: "1.1.0".into(),
                dependencies: vec![],
            },
            ModuleRecord {
                id: "shell".into(),
                version: "1.1.0".into(),
                dependencies: vec!["core".into()],
            },
        ];
        assert_eq!(
            select_outdated(&installed, &available).unwrap().modules,
            vec!["core", "shell"]
        );
    }

    #[test]
    fn rejects_duplicate_available_module_ids() {
        let available = vec![
            ModuleRecord {
                id: "core".into(),
                version: "1.0.0".into(),
                dependencies: vec![],
            },
            ModuleRecord {
                id: "core".into(),
                version: "1.1.0".into(),
                dependencies: vec![],
            },
        ];

        let error = select_outdated(&InstalledManifest { components: vec![] }, &available)
            .expect_err("duplicate module ids must be rejected");
        assert!(error.contains("duplicate or invalid module id"));
    }

    #[test]
    fn rejects_ambiguous_available_dependencies() {
        let available = vec![ModuleRecord {
            id: "core".into(),
            version: "1.0.0".into(),
            dependencies: vec!["supervisor".into(), "supervisor".into()],
        }];

        let error = select_outdated(&InstalledManifest { components: vec![] }, &available)
            .expect_err("duplicate dependencies must be rejected");
        assert!(error.contains("invalid dependencies"));
    }

    #[test]
    fn rejects_cycles_in_available_dependencies() {
        let available = vec![
            ModuleRecord {
                id: "core".into(),
                version: "1.0.0".into(),
                dependencies: vec!["supervisor".into()],
            },
            ModuleRecord {
                id: "supervisor".into(),
                version: "1.0.0".into(),
                dependencies: vec!["core".into()],
            },
        ];

        let error = select_outdated(&InstalledManifest { components: vec![] }, &available)
            .expect_err("dependency cycles must be rejected");
        assert!(error.contains("cycle in available module dependencies"));
    }

    #[test]
    fn rejects_invalid_installed_versions_before_comparison() {
        let installed = InstalledManifest {
            components: vec![ModuleRecord {
                id: "core".into(),
                version: "broken".into(),
                dependencies: vec![],
            }],
        };
        let available = vec![ModuleRecord {
            id: "core".into(),
            version: "1.0.0".into(),
            dependencies: vec![],
        }];

        let error = select_outdated(&installed, &available)
            .expect_err("invalid installed versions must fail closed");
        assert!(error.contains("invalid installed version"));
    }

    #[test]
    fn accepts_null_dependencies_in_installed_manifest() {
        let installed: InstalledManifest = serde_json::from_value(serde_json::json!({
            "components": [{"id": "core", "version": "1.0.0", "dependencies": null}]
        }))
        .expect("manifest with nullable dependencies");

        assert!(installed.components[0].dependencies.is_empty());

        let installed: InstalledManifest = serde_json::from_value(serde_json::json!({
            "components": [{"id": "core", "version": "1.0.0", "dependencies": "supervisor"}]
        }))
        .expect("manifest with a single dependency");

        assert_eq!(installed.components[0].dependencies, vec!["supervisor"]);
    }

    #[test]
    fn validates_manifest_hash_and_size() {
        let directory = tempfile::tempdir().unwrap();
        let mut file = std::fs::File::create(directory.path().join("core.exe")).unwrap();
        file.write_all(b"core").unwrap();
        let digest = sha2::Sha256::digest(b"core");
        let hash = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let manifest = ComponentManifest {
            components: vec![InstalledComponent {
                id: "core".into(),
                path: "core.exe".into(),
                size: 4,
                sha256: hash,
            }],
        };
        validate_component_manifest(&manifest, directory.path()).unwrap();
    }

    #[test]
    fn rejects_modified_component() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("core.exe"), b"tampered").unwrap();
        let manifest = ComponentManifest {
            components: vec![InstalledComponent {
                id: "core".into(),
                path: "core.exe".into(),
                size: 4,
                sha256: "00".repeat(32),
            }],
        };
        let error = validate_component_manifest(&manifest, directory.path()).unwrap_err();
        assert!(error.contains("size mismatch") || error.contains("sha256 mismatch"));
    }

    #[test]
    fn rejects_oversized_component_before_opening_file() {
        let directory = tempfile::tempdir().unwrap();
        let manifest = ComponentManifest {
            components: vec![InstalledComponent {
                id: "core".into(),
                path: "core.exe".into(),
                size: MAX_COMPONENT_ARTIFACT_BYTES + 1,
                sha256: "00".repeat(32),
            }],
        };

        let error = validate_component_manifest(&manifest, directory.path()).unwrap_err();
        assert!(error.contains("invalid path"));
    }

    #[test]
    fn streamed_hash_rejects_truncated_and_oversized_content() {
        let hash = sha2::Sha256::digest(b"core")
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert!(hash_reader_matches(std::io::Cursor::new(b"core"), 4, &hash).unwrap());
        assert!(!hash_reader_matches(std::io::Cursor::new(b"cor"), 4, &hash).unwrap());
        assert!(!hash_reader_matches(std::io::Cursor::new(b"core!"), 4, &hash).unwrap());
    }

    #[test]
    fn rejects_duplicate_component_paths_and_windows_streams() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("core.exe"), b"core").unwrap();
        let hash = sha2::Sha256::digest(b"core")
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let duplicate = ComponentManifest {
            components: vec![
                InstalledComponent {
                    id: "core".into(),
                    path: "core.exe".into(),
                    size: 4,
                    sha256: hash.clone(),
                },
                InstalledComponent {
                    id: "supervisor".into(),
                    path: "core.exe".into(),
                    size: 4,
                    sha256: hash.clone(),
                },
            ],
        };
        assert!(validate_component_manifest(&duplicate, directory.path()).is_err());

        let stream = ComponentManifest {
            components: vec![InstalledComponent {
                id: "core".into(),
                path: "core.exe:secret".into(),
                size: 4,
                sha256: hash,
            }],
        };
        assert!(validate_component_manifest(&stream, directory.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_component_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let target = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(target.path(), b"core").unwrap();
        symlink(target.path(), directory.path().join("core.exe")).unwrap();
        let hash = sha2::Sha256::digest(b"core")
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let manifest = ComponentManifest {
            components: vec![InstalledComponent {
                id: "core".into(),
                path: "core.exe".into(),
                size: 4,
                sha256: hash,
            }],
        };

        let error = validate_component_manifest(&manifest, directory.path()).unwrap_err();
        assert!(error.contains("regular file"));
    }
}
