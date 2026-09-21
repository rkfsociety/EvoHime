//! SQLite row decoding for the typed memory contract.

use crate::memory_store::{
    default_authority, default_durability, MemoryExtractionFields, MemoryPrivacy, MemoryRecord,
    MemoryScope, MemoryStoreError,
};

pub(crate) fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRecord> {
    let archived: i64 = row.get(9)?;
    let forgotten: i64 = row.get(10)?;
    Ok(MemoryRecord {
        id: row.get(0)?,
        scope: MemoryScope::parse(&row.get::<_, String>(1)?).map_err(to_sql_error)?,
        scope_id: row.get(2)?,
        title: row.get(3)?,
        content: row.get(4)?,
        provenance: row.get(5)?,
        privacy: MemoryPrivacy::parse(&row.get::<_, String>(6)?).map_err(to_sql_error)?,
        created_at: row.get(7)?,
        expires_at: row.get(8)?,
        archived: archived != 0,
        forgotten: forgotten != 0,
        confirmations: row.get(11).unwrap_or(1),
        lesson_key: row.get(12).unwrap_or(None),
        extraction: MemoryExtractionFields {
            kind: row.get(13)?,
            canonical_subject: row.get(14)?,
            confirmation_state: row.get(15)?,
            model_confidence: row.get(16)?,
            verification_confidence: row.get(17)?,
            privacy_class: row.get(18)?,
            source_trust: row.get(19)?,
            supersedes: row.get(20)?,
            superseded_by: row.get(21)?,
            supersession_reason: row.get(22)?,
            extractor_version: row.get(23)?,
            policy_version: row.get(24)?,
            validation_status: row.get(25)?,
            validated_at: row.get(26)?,
            provenance_source_id: row.get(27)?,
            record_version: row.get(28).unwrap_or(1),
            evidence_refs: row
                .get::<_, Option<String>>(29)
                .unwrap_or(None)
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_default(),
            execution_event_refs: row
                .get::<_, Option<String>>(30)
                .unwrap_or(None)
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_default(),
            authority: row.get(31).unwrap_or_else(|_| default_authority()),
            durability: row.get(32).unwrap_or_else(|_| default_durability()),
            confidence: row.get(33).unwrap_or(1.0),
        },
    })
}

pub(crate) fn to_sql_error(error: MemoryStoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
