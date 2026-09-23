use crate::memory_store::{MemoryPrivacy, MemoryScope};

/// Input fields for adding a session-scoped note to memory.
pub struct InsertSessionNoteInput<'a> {
    /// Stable memory-note identifier.
    pub id: &'a str,
    /// Session that produced the note.
    pub session_id: &'a str,
    /// Scope in which the note may be recalled.
    pub scope: MemoryScope,
    /// Identifier of the selected scope.
    pub scope_id: &'a str,
    /// Category assigned to the note.
    pub kind: &'a str,
    /// Note text to retain.
    pub statement: &'a str,
    /// Creation timestamp.
    pub created_at: &'a str,
    /// Optional retention expiration timestamp.
    pub expires_at: &'a str,
}

/// Owned input for inserting a memory record.
pub struct MemoryRecordInput {
    /// Stable memory-record identifier.
    pub id: String,
    /// Scope in which the record may be recalled.
    pub scope: MemoryScope,
    /// Identifier of the selected scope.
    pub scope_id: String,
    /// Short display title.
    pub title: String,
    /// Memory content.
    pub content: String,
    /// Source or origin description for the memory.
    pub provenance: String,
    /// Privacy classification that governs retention and access.
    pub privacy: MemoryPrivacy,
    /// Creation timestamp.
    pub created_at: String,
    /// Optional retention expiration timestamp.
    pub expires_at: Option<String>,
}
