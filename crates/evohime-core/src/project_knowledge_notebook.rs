use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current serialized schema version for project knowledge notebooks.
pub const SCHEMA_VERSION: u32 = 1;
const MAX_ID: usize = 128;
const MAX_TEXT: usize = 512;
const MAX_ENTRIES: usize = 128;

/// Lifecycle state of a notebook revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Revision is being assembled and cannot be pinned to a run.
    Draft,
    /// Revision passed validation and may be pinned.
    Active,
    /// Revision has been replaced by a newer one.
    Superseded,
    /// Revision is invalid and cannot be used.
    Invalid,
}

/// Bounded reference to one knowledge note included in a notebook.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteRef {
    /// Stable identifier of the note.
    pub id: String,
    /// Human-readable note title.
    pub title: String,
    /// Locator for the source from which the note was derived.
    pub source_ref: String,
    /// Digest identifying the referenced note content.
    pub content_hash: String,
    /// Search and organization tags associated with the note.
    pub tags: Vec<String>,
}

/// Content-addressed collection of project knowledge note references.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Notebook {
    /// Serialized schema version.
    pub schema_version: u32,
    /// Stable notebook identifier.
    pub id: String,
    /// Monotonically increasing notebook revision.
    pub revision: u64,
    /// Lifecycle state controlling whether the notebook can be pinned.
    pub lifecycle: Lifecycle,
    /// Project scope to which the notebook belongs.
    pub scope: String,
    /// Bounded set of referenced notes.
    pub entries: Vec<NoteRef>,
    /// SHA-256 digest of the canonical notebook with this field cleared.
    pub content_hash: String,
}

/// Snapshot reference that binds a run to one notebook revision and digest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotebookPin {
    /// Notebook identifier.
    pub notebook_id: String,
    /// Pinned notebook revision.
    pub revision: u64,
    /// Digest of the pinned notebook content.
    pub content_hash: String,
    /// Run that owns this pin.
    pub run_id: String,
}

/// Validation failure for notebook metadata or pin requests.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NotebookError {
    /// The record violated schema, bounds, uniqueness, lifecycle, or digest requirements.
    #[error("invalid project knowledge notebook: {0}")]
    Invalid(String),
}

fn bounded(value: &str, max: usize, field: &str) -> Result<(), NotebookError> {
    if value.trim().is_empty() || value.len() > max {
        return Err(NotebookError::Invalid(format!("{field}_out_of_bounds")));
    }
    Ok(())
}
/// Computes the canonical SHA-256 digest with `content_hash` cleared.
pub fn canonical_hash(notebook: &Notebook) -> Result<String, NotebookError> {
    let mut normalized = notebook.clone();
    normalized.content_hash.clear();
    let bytes = serde_json::to_vec(&normalized)
        .map_err(|_| NotebookError::Invalid("notebook_not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
/// Validates schema, field bounds, entry uniqueness, and the canonical digest.
pub fn validate(notebook: &Notebook) -> Result<(), NotebookError> {
    if notebook.schema_version != SCHEMA_VERSION {
        return Err(NotebookError::Invalid("unsupported_schema_version".into()));
    }
    bounded(&notebook.id, MAX_ID, "notebook_id")?;
    bounded(&notebook.scope, MAX_TEXT, "scope")?;
    if notebook.revision == 0 || notebook.entries.is_empty() || notebook.entries.len() > MAX_ENTRIES
    {
        return Err(NotebookError::Invalid(
            "entry_count_or_revision_invalid".into(),
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    for entry in &notebook.entries {
        bounded(&entry.id, MAX_ID, "entry_id")?;
        bounded(&entry.title, MAX_TEXT, "title")?;
        bounded(&entry.source_ref, MAX_TEXT, "source_ref")?;
        bounded(&entry.content_hash, MAX_ID, "content_hash")?;
        if !ids.insert(entry.id.as_str()) {
            return Err(NotebookError::Invalid("duplicate_entry_id".into()));
        }
        if entry.tags.len() > 16 {
            return Err(NotebookError::Invalid("tag_count_out_of_bounds".into()));
        }
        for tag in &entry.tags {
            bounded(tag, MAX_ID, "tag")?
        }
    }
    if canonical_hash(notebook)? != notebook.content_hash {
        return Err(NotebookError::Invalid("content_hash_mismatch".into()));
    }
    Ok(())
}
/// Creates a run-specific pin for a valid active notebook revision.
pub fn pin(notebook: &Notebook, run_id: &str) -> Result<NotebookPin, NotebookError> {
    bounded(run_id, MAX_ID, "run_id")?;
    validate(notebook)?;
    if notebook.lifecycle != Lifecycle::Active {
        return Err(NotebookError::Invalid("notebook_is_not_active".into()));
    }
    Ok(NotebookPin {
        notebook_id: notebook.id.clone(),
        revision: notebook.revision,
        content_hash: notebook.content_hash.clone(),
        run_id: run_id.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn notebook(lifecycle: Lifecycle) -> Notebook {
        let mut n = Notebook {
            schema_version: SCHEMA_VERSION,
            id: "notebook".into(),
            revision: 1,
            lifecycle,
            scope: "project".into(),
            entries: vec![NoteRef {
                id: "note".into(),
                title: "Design".into(),
                source_ref: "ref:1".into(),
                content_hash: "hash".into(),
                tags: vec!["design".into()],
            }],
            content_hash: String::new(),
        };
        n.content_hash = canonical_hash(&n).expect("hash");
        n
    }
    #[test]
    fn active_notebook_pins() {
        assert!(pin(&notebook(Lifecycle::Active), "run").is_ok());
        assert!(pin(&notebook(Lifecycle::Draft), "run").is_err())
    }
    #[test]
    fn duplicate_entries_fail() {
        let mut n = notebook(Lifecycle::Active);
        n.entries.push(n.entries[0].clone());
        assert!(validate(&n).is_err())
    }
}
