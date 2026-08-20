//! Signed, versioned offline evaluation catalog for the small-route gate.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationRecord {
    pub catalog_version: String,
    pub task_class: String,
    pub dataset_hash: String,
    pub large_route_id: String,
    pub small_route_id: String,
    pub metric: String,
    pub large_score: f64,
    pub small_score: f64,
    pub quality_floor: f64,
    pub generated_at: u64,
    pub expires_at: u64,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    #[error("catalog is empty")]
    Empty,
    #[error("malformed catalog line")]
    Malformed,
    #[error("catalog signature mismatch")]
    Signature,
    #[error("catalog schema is invalid")]
    Schema,
    #[error("catalog is expired")]
    Expired,
    #[error("catalog route mismatch")]
    RouteMismatch,
    #[error("catalog I/O error: {0}")]
    Io(String),
}

#[derive(Debug, Clone, Default)]
pub struct EvaluationCatalog {
    pub records: Vec<EvaluationRecord>,
}

impl EvaluationCatalog {
    pub fn load_jsonl(text: &str, expected_signature: Option<&str>) -> Result<Self, CatalogError> {
        let mut records = Vec::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let record: EvaluationRecord =
                serde_json::from_str(line).map_err(|_| CatalogError::Malformed)?;
            validate_record(&record)?;
            if let Some(signature) = expected_signature {
                if record.signature != signature {
                    return Err(CatalogError::Signature);
                }
            }
            if record.signature != Self::canonical_signature(&record) {
                return Err(CatalogError::Signature);
            }
            records.push(record);
        }
        if records.is_empty() {
            return Err(CatalogError::Empty);
        }
        records.sort_by(|left, right| {
            left.catalog_version
                .cmp(&right.catalog_version)
                .then(left.task_class.cmp(&right.task_class))
        });
        Ok(Self { records })
    }

    pub fn load_file(path: &Path, expected_signature: Option<&str>) -> Result<Self, CatalogError> {
        Self::load_jsonl(
            &fs::read_to_string(path).map_err(|error| CatalogError::Io(error.to_string()))?,
            expected_signature,
        )
    }

    pub fn record(
        &self,
        task_class: &str,
        large_route_id: &str,
        small_route_id: &str,
        now_ms: u64,
    ) -> Option<&EvaluationRecord> {
        self.records.iter().find(|record| {
            record.task_class == task_class
                && record.large_route_id == large_route_id
                && record.small_route_id == small_route_id
                && record.expires_at > now_ms
        })
    }

    pub fn small_route_allowed(
        &self,
        task_class: &str,
        large_route_id: &str,
        small_route_id: &str,
        quality_delta: f64,
        now_ms: u64,
    ) -> bool {
        let Some(record) = self.record(task_class, large_route_id, small_route_id, now_ms) else {
            return false;
        };
        record.small_score >= record.quality_floor
            && record.small_score >= record.large_score - quality_delta
    }

    pub fn canonical_signature(record: &EvaluationRecord) -> String {
        let mut value = serde_json::to_value(record).expect("evaluation record serializes");
        if let Some(object) = value.as_object_mut() {
            object.insert("signature".into(), serde_json::Value::String(String::new()));
        }
        let bytes = serde_json::to_vec(&value).expect("evaluation record serializes");
        hex::encode(Sha256::digest(bytes))
    }

    /// Validates a complete signed catalog before replacing the runtime file.
    /// The caller never observes a partially-written JSONL document.
    pub fn atomic_replace(path: &Path, content: &str) -> Result<Self, CatalogError> {
        let catalog = Self::load_jsonl(content, None)?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| CatalogError::Io(error.to_string()))?;
        let temp = temporary_path(path);
        fs::write(&temp, content).map_err(|error| CatalogError::Io(error.to_string()))?;
        if let Err(error) = fs::rename(&temp, path) {
            let _ = fs::remove_file(&temp);
            return Err(CatalogError::Io(error.to_string()));
        }
        Ok(catalog)
    }
}

/// A runtime catalog location. Loading is fail-closed: an invalid external
/// file is not silently mixed with the embedded fallback.
#[derive(Debug, Clone)]
pub struct CatalogStore {
    path: PathBuf,
    catalog: EvaluationCatalog,
}

impl CatalogStore {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, CatalogError> {
        let path = path.into();
        let catalog = EvaluationCatalog::load_file(&path, None)?;
        Ok(Self { path, catalog })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn catalog(&self) -> &EvaluationCatalog {
        &self.catalog
    }

    pub fn replace(&mut self, content: &str) -> Result<(), CatalogError> {
        let catalog = EvaluationCatalog::atomic_replace(&self.path, content)?;
        self.catalog = catalog;
        Ok(())
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut temp = path.to_path_buf();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("catalog");
    temp.set_extension(format!("{extension}.tmp-{}", std::process::id()));
    temp
}

fn validate_record(record: &EvaluationRecord) -> Result<(), CatalogError> {
    if record.catalog_version.is_empty()
        || record.task_class.is_empty()
        || record.large_route_id.is_empty()
        || record.small_route_id.is_empty()
        || record.metric.is_empty()
        || record.expires_at <= record.generated_at
        || !record.large_score.is_finite()
        || !record.small_score.is_finite()
        || !record.quality_floor.is_finite()
        || record.quality_floor < 0.0
        || record.quality_floor > 1.0
    {
        return Err(CatalogError::Schema);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record() -> EvaluationRecord {
        EvaluationRecord {
            catalog_version: "v1".into(),
            task_class: "simple".into(),
            dataset_hash: "d".into(),
            large_route_id: "cloud".into(),
            small_route_id: "local".into(),
            metric: "accuracy".into(),
            large_score: 0.9,
            small_score: 0.88,
            quality_floor: 0.8,
            generated_at: 1,
            expires_at: 100,
            signature: "sig".into(),
        }
    }
    #[test]
    fn gate_requires_floor_and_delta() {
        let catalog = EvaluationCatalog {
            records: vec![record()],
        };
        assert!(catalog.small_route_allowed("simple", "cloud", "local", 0.03, 2));
        assert!(!catalog.small_route_allowed("simple", "cloud", "local", 0.01, 2));
    }
    #[test]
    fn expired_catalog_is_gate_unavailable() {
        let catalog = EvaluationCatalog {
            records: vec![record()],
        };
        assert!(!catalog.small_route_allowed("simple", "cloud", "local", 0.1, 100));
    }
    #[test]
    fn signature_excludes_signature_field() {
        let value = record();
        assert_eq!(
            EvaluationCatalog::canonical_signature(&value),
            EvaluationCatalog::canonical_signature(&EvaluationRecord {
                signature: "other".into(),
                ..value
            })
        );
    }
    #[test]
    fn atomic_replace_rejects_invalid_content_and_keeps_old_catalog() {
        let dir = std::env::temp_dir().join(format!("evohime-catalog-{}", std::process::id()));
        let path = dir.join("routing.jsonl");
        let value = record();
        let content = serde_json::to_string(&EvaluationRecord {
            signature: EvaluationCatalog::canonical_signature(&value),
            ..value
        })
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = CatalogStore {
            path: path.clone(),
            catalog: EvaluationCatalog::default(),
        };
        store.replace(&content).unwrap();
        assert_eq!(store.catalog().records.len(), 1);
        assert!(EvaluationCatalog::atomic_replace(&path, "not-json").is_err());
        assert_eq!(
            EvaluationCatalog::load_file(&path, None)
                .unwrap()
                .records
                .len(),
            1
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
