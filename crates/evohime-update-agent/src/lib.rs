use serde::{
    de::{Deserializer, Error as DeError},
    Deserialize, Serialize,
};
use sha2::Digest;
use std::cmp::Ordering;
use std::io::Read;
use std::path::Path;

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
    pub modules: Vec<String>,
    pub available: Vec<UpdaterModuleStatus>,
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
            || component.sha256.len() != 64
            || !component
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!("invalid path for {}", component.id));
        }
        let path = install_dir.join(&component.path);
        let metadata =
            std::fs::metadata(&path).map_err(|error| format!("{}: {error}", component.id))?;
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
}
