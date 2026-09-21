const COLUMNS: &str = "id, scope_kind, scope_id, title, content, provenance, privacy,
        created_at, expires_at, archived, forgotten, confirmations, lesson_key,
        kind, canonical_subject, confirmation_state, model_confidence,
        verification_confidence, privacy_class, source_trust, supersedes,
        superseded_by, supersession_reason, extractor_version, policy_version,
        validation_status, validated_at, provenance_source_id, record_version,
        evidence_refs, execution_event_refs, authority, durability, confidence";

const RETRIEVABLE_PREDICATE: &str = "forgotten = 0 AND archived = 0
          AND confirmation_state = 'confirmed'
          AND validation_status IN ('not_required', 'valid')
          AND superseded_by IS NULL";

pub(crate) fn select_by_id() -> String {
    format!("SELECT {COLUMNS} FROM memory_entries WHERE id = ?1")
}

pub(crate) fn search() -> String {
    format!(
        "SELECT {COLUMNS} FROM memory_entries
        WHERE scope_kind = ?1 AND scope_id = ?2
          AND {RETRIEVABLE_PREDICATE}
          AND (expires_at IS NULL OR expires_at > ?3)
          AND (lower(title) LIKE lower(?4) ESCAPE '\\'
               OR lower(content) LIKE lower(?4) ESCAPE '\\')
        ORDER BY id ASC LIMIT ?5"
    )
}

pub(crate) fn search_lessons() -> String {
    format!(
        "SELECT {COLUMNS} FROM memory_entries
        WHERE scope_kind = ?1 AND scope_id = ?2
          AND {RETRIEVABLE_PREDICATE} AND lesson_key IS NOT NULL
          AND (expires_at IS NULL OR expires_at > ?3)
          AND (lower(title) LIKE lower(?4) ESCAPE '\\'
               OR lower(content) LIKE lower(?4) ESCAPE '\\')
        ORDER BY confirmations DESC, created_at DESC, id ASC LIMIT ?5"
    )
}

pub(crate) fn list() -> String {
    format!(
        "SELECT {COLUMNS} FROM memory_entries
        WHERE scope_kind = ?1 AND scope_id = ?2
          AND forgotten = 0
          AND (?3 = 1 OR archived = 0)
        ORDER BY created_at DESC, id ASC LIMIT ?4"
    )
}

pub(crate) fn list_by_state() -> String {
    format!(
        "SELECT {COLUMNS} FROM memory_entries
        WHERE scope_kind = ?1 AND scope_id = ?2
          AND confirmation_state = ?3 AND forgotten = 0
        ORDER BY created_at DESC, id ASC LIMIT ?4"
    )
}

pub(crate) fn conflict_candidates() -> String {
    format!(
        "SELECT {COLUMNS} FROM memory_entries
        WHERE scope_kind = ?1 AND scope_id = ?2 AND kind = ?3
          AND {RETRIEVABLE_PREDICATE}
        ORDER BY created_at DESC, id ASC LIMIT ?4"
    )
}
