use crate::memory_store::{MemoryPrivacy, MemoryScope};

pub struct InsertSessionNoteInput<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub scope: MemoryScope,
    pub scope_id: &'a str,
    pub kind: &'a str,
    pub statement: &'a str,
    pub created_at: &'a str,
    pub expires_at: &'a str,
}

pub struct MemoryRecordInput {
    pub id: String,
    pub scope: MemoryScope,
    pub scope_id: String,
    pub title: String,
    pub content: String,
    pub provenance: String,
    pub privacy: MemoryPrivacy,
    pub created_at: String,
    pub expires_at: Option<String>,
}
