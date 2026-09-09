use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};
pub const SCHEMA_VERSION:u32=1;
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]#[serde(rename_all="snake_case")]pub enum Lifecycle{Draft,Active,Superseded,Invalid}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]pub struct TemporalMemoryFactsRecord{pub schema_version:u32,pub id:String,pub revision:u64,pub lifecycle:Lifecycle,pub scope:String,pub content_hash:String}
#[derive(Debug,thiserror::Error,PartialEq,Eq)]pub enum Error{#[error("invalid temporal_memory_facts: {0}")]Invalid(String)}
pub fn canonical_hash(v:&TemporalMemoryFactsRecord)->Result<String,Error>{let mut c=v.clone();c.content_hash.clear();let b=serde_json::to_vec(&c).map_err(|_|Error::Invalid("not_serializable".into()))?;Ok(format!("{:x}",Sha256::digest(b)))}
pub fn validate(v:&TemporalMemoryFactsRecord)->Result<(),Error>{if v.schema_version!=SCHEMA_VERSION||v.id.trim().is_empty()||v.id.len()>256||v.scope.len()>256||v.revision==0||matches!(v.lifecycle,Lifecycle::Invalid){return Err(Error::Invalid("bounds_or_lifecycle".into()))}if canonical_hash(v)?!=v.content_hash{return Err(Error::Invalid("content_hash_mismatch".into()))}Ok(())}
pub fn projection(v:&TemporalMemoryFactsRecord)->Result<serde_json::Value,Error>{validate(v)?;Ok(serde_json::json!({"status":"metadata_only","capability":"temporal_memory_facts","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}))}
