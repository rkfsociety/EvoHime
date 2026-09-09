use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
const MAX_ID: usize = 128;
const MAX_TEXT: usize = 512;
const MAX_ENTRIES: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle { Draft, Active, Superseded, Invalid }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteRef { pub id: String, pub title: String, pub source_ref: String, pub content_hash: String, pub tags: Vec<String> }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Notebook { pub schema_version: u32, pub id: String, pub revision: u64, pub lifecycle: Lifecycle, pub scope: String, pub entries: Vec<NoteRef>, pub content_hash: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotebookPin { pub notebook_id: String, pub revision: u64, pub content_hash: String, pub run_id: String }

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NotebookError { #[error("invalid project knowledge notebook: {0}")] Invalid(String) }

fn bounded(value: &str, max: usize, field: &str) -> Result<(), NotebookError> { if value.trim().is_empty() || value.len() > max { return Err(NotebookError::Invalid(format!("{field}_out_of_bounds"))); } Ok(()) }
pub fn canonical_hash(notebook: &Notebook) -> Result<String, NotebookError> { let mut normalized=notebook.clone(); normalized.content_hash.clear(); let bytes=serde_json::to_vec(&normalized).map_err(|_|NotebookError::Invalid("notebook_not_serializable".into()))?; Ok(format!("{:x}",Sha256::digest(bytes))) }
pub fn validate(notebook: &Notebook) -> Result<(), NotebookError> { if notebook.schema_version != SCHEMA_VERSION {return Err(NotebookError::Invalid("unsupported_schema_version".into()))} bounded(&notebook.id,MAX_ID,"notebook_id")?; bounded(&notebook.scope,MAX_TEXT,"scope")?; if notebook.revision==0||notebook.entries.is_empty()||notebook.entries.len()>MAX_ENTRIES{return Err(NotebookError::Invalid("entry_count_or_revision_invalid".into()))} let mut ids=std::collections::BTreeSet::new(); for entry in &notebook.entries {bounded(&entry.id,MAX_ID,"entry_id")?;bounded(&entry.title,MAX_TEXT,"title")?;bounded(&entry.source_ref,MAX_TEXT,"source_ref")?;bounded(&entry.content_hash,MAX_ID,"content_hash")?;if !ids.insert(entry.id.as_str()){return Err(NotebookError::Invalid("duplicate_entry_id".into()))} if entry.tags.len()>16{return Err(NotebookError::Invalid("tag_count_out_of_bounds".into()))} for tag in &entry.tags{bounded(tag,MAX_ID,"tag")?}} if canonical_hash(notebook)?!=notebook.content_hash{return Err(NotebookError::Invalid("content_hash_mismatch".into()))} Ok(()) }
pub fn pin(notebook: &Notebook, run_id: &str) -> Result<NotebookPin, NotebookError> { bounded(run_id,MAX_ID,"run_id")?; validate(notebook)?; if notebook.lifecycle!=Lifecycle::Active{return Err(NotebookError::Invalid("notebook_is_not_active".into()))} Ok(NotebookPin{notebook_id:notebook.id.clone(),revision:notebook.revision,content_hash:notebook.content_hash.clone(),run_id:run_id.into()}) }

#[cfg(test)]
mod tests { use super::*; fn notebook(lifecycle:Lifecycle)->Notebook{let mut n=Notebook{schema_version:SCHEMA_VERSION,id:"notebook".into(),revision:1,lifecycle,scope:"project".into(),entries:vec![NoteRef{id:"note".into(),title:"Design".into(),source_ref:"ref:1".into(),content_hash:"hash".into(),tags:vec!["design".into()]}],content_hash:String::new()};n.content_hash=canonical_hash(&n).expect("hash");n} #[test] fn active_notebook_pins(){assert!(pin(&notebook(Lifecycle::Active),"run").is_ok());assert!(pin(&notebook(Lifecycle::Draft),"run").is_err())} #[test] fn duplicate_entries_fail(){let mut n=notebook(Lifecycle::Active);n.entries.push(n.entries[0].clone());assert!(validate(&n).is_err())} }
