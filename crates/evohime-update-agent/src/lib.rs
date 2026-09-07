use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

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
}
