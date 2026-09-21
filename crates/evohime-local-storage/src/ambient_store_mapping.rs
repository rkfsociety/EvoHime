//! SQLite row decoding for the ambient storage contract.

use evohime_listener_contract::{ExtractionState, ProposalKind, ProposalState};

use crate::ambient_store::{
    AmbientEpisodeRecord, AmbientProposalRecord, AmbientStoreError, AmbientTombstoneRecord,
    AmbientUtteranceRecord,
};

pub(crate) fn map_episode(row: &rusqlite::Row<'_>) -> rusqlite::Result<AmbientEpisodeRecord> {
    let stored: String = row.get(7)?;
    let extraction_state = ExtractionState::parse(&stored).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            Box::new(AmbientStoreError::Empty {
                field: "extraction_state",
            }),
        )
    })?;
    Ok(AmbientEpisodeRecord {
        episode_id: row.get(0)?,
        started_at: row.get(1)?,
        ended_at: row.get(2)?,
        utterance_count: row.get(3)?,
        speech_ms: row.get(4)?,
        engine_version: row.get(5)?,
        model_id: row.get(6)?,
        extraction_state,
        expires_at: row.get(8)?,
    })
}

pub(crate) fn map_utterance(row: &rusqlite::Row<'_>) -> rusqlite::Result<AmbientUtteranceRecord> {
    Ok(AmbientUtteranceRecord {
        utterance_id: row.get(0)?,
        episode_id: row.get(1)?,
        sequence: row.get(2)?,
        started_at: row.get(3)?,
        duration_ms: row.get(4)?,
        text: row.get(5)?,
        text_hash: row.get(6)?,
        language: row.get(7)?,
        avg_logprob: row.get(8)?,
        speaker: row.get(9)?,
        redacted: row.get::<_, i64>(10)? != 0,
        expires_at: row.get(11)?,
    })
}

pub(crate) fn map_proposal(row: &rusqlite::Row<'_>) -> rusqlite::Result<AmbientProposalRecord> {
    let stored_kind: String = row.get(3)?;
    let kind = ProposalKind::parse(&stored_kind).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(AmbientStoreError::Empty { field: "kind" }),
        )
    })?;
    let stored_state: String = row.get(14)?;
    let state = ProposalState::parse(&stored_state).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            14,
            rusqlite::types::Type::Text,
            Box::new(AmbientStoreError::Empty { field: "state" }),
        )
    })?;
    Ok(AmbientProposalRecord {
        proposal_id: row.get(0)?,
        proposal_key: row.get(1)?,
        mute_key: row.get(2)?,
        kind,
        subject_key: row.get(4)?,
        subject: row.get(5)?,
        title: row.get(6)?,
        source_episode_id: row.get(7)?,
        source_deleted_at: row.get(8)?,
        source_deleted_reason: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        expires_at: row.get(12)?,
        occurrences: row.get(13)?,
        state,
        accepted_task_id: row.get(15)?,
        idempotency_key: row.get(16)?,
    })
}

pub(crate) fn map_tombstone(row: &rusqlite::Row<'_>) -> rusqlite::Result<AmbientTombstoneRecord> {
    Ok(AmbientTombstoneRecord {
        tombstone_id: row.get(0)?,
        episode_id: row.get(1)?,
        removed_at: row.get(2)?,
        reason: row.get(3)?,
        utterance_count: row.get(4)?,
        expires_at: row.get(5)?,
    })
}
