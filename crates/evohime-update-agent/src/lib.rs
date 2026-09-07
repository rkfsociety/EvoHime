use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::cmp::Ordering;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ModuleRecord {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
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

/// Validate the immutable package manifest before starting any product process.
/// A mismatch is fatal: running a partially replaced installation would make
/// dependency and rollback guarantees impossible.
pub fn validate_component_manifest(
    manifest: &ComponentManifest,
    install_dir: &Path,
) -> Result<(), String> {
    for component in &manifest.components {
        if component.path.contains("..") || Path::new(&component.path).is_absolute() {
            return Err(format!("invalid path for {}", component.id));
        }
        let path = install_dir.join(&component.path);
        let metadata =
            std::fs::metadata(&path).map_err(|error| format!("{}: {error}", component.id))?;
        if metadata.len() != component.size {
            return Err(format!("size mismatch for {}", component.id));
        }
        let bytes = std::fs::read(&path).map_err(|error| format!("{}: {error}", component.id))?;
        let digest = sha2::Sha256::digest(&bytes);
        let actual = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if actual != component.sha256.to_ascii_lowercase() {
            return Err(format!("sha256 mismatch for {}", component.id));
        }
    }
    Ok(())
}

pub fn select_outdated(
    installed: &InstalledManifest,
    available: &[ModuleRecord],
) -> Result<UpdatePlan, String> {
    let current = installed
        .components
        .iter()
        .map(|item| (item.id.as_str(), item.version.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut selected = std::collections::BTreeSet::new();
    for item in available {
        if !is_semver(&item.version) {
            return Err(format!("invalid version for {}", item.id));
        }
        if current
            .get(item.id.as_str())
            .is_none_or(|version| compare_semver(version, &item.version) == Ordering::Less)
        {
            selected.insert(item.id.clone());
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for item in available {
            if !selected.contains(&item.id)
                && item
                    .dependencies
                    .iter()
                    .any(|dependency| selected.contains(dependency))
            {
                selected.insert(item.id.clone());
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

fn is_semver(value: &str) -> bool {
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
}
