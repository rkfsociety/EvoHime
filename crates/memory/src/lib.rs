//! Agent memory domain service (roadmap 6.18–6.25+).
//!
//! Redaction, normalization, deduplication, conflict detection, extraction,
//! ask-on-uncertainty gate, experience/playbooks, feedback/decay, and hybrid
//! embedding retrieval on `memory_items` (feature-hash default, optional remote neural).

mod conflict;
mod decision;
mod dedupe;
mod embed;
mod experience;
mod extract;
mod feedback;
mod feedback_service;
mod normalize;
mod redact;
mod retrieve;
mod service;

pub use conflict::{detect_conflict, ConflictHit};
pub use decision::{decide_gate, GateDecision, GateInput, AUTO_PROMOTE_CONFIDENCE};
pub use dedupe::{
    content_fingerprint, detect_duplicate, detect_duplicate_with_embedding,
    SEMANTIC_DEDUPE_MIN_COSINE,
};
pub use embed::{
    cosine_similarity, embed_text, embed_text_hash, embedding_version, needs_reembed,
    semantic_score, EmbeddingResult, EncoderConfig, EncoderMode, EMBEDDING_DIM, EMBEDDING_VERSION,
    HASH_EMBEDDING_VERSION, REMOTE_EMBEDDING_BASE_VERSION, SEMANTIC_MIN_COSINE,
    SEMANTIC_SCORE_WEIGHT,
};
pub use experience::{
    format_experience_line, is_experience_kind, parse_playbook_payload, PlaybookPayload,
};
pub use extract::{
    experience_patterns, extract_candidates, extract_failure_candidates, heuristic_extract,
    is_ephemeral_task_dump, is_transient_infra_failure, parse_extraction_json, ExtractedCandidate,
    FAILURE_CANDIDATE_LIMIT, FAILURE_CONFIDENCE_CAP, MAX_CANDIDATES_PER_TASK,
};
pub use feedback::{
    apply_feedback_signal, FeedbackAdjustment, FeedbackSignal, ARCHIVE_CONFIDENCE_THRESHOLD,
    HARMFUL_CONFIDENCE_DECAY, HELPFUL_CONFIDENCE_BUMP, IDLE_CONFIDENCE_DECAY,
};
pub use feedback_service::{
    decay_unused_memory, record_memory_corrected, record_memory_corrected_for_operator,
    record_memory_harmful, record_memory_helpful, record_memory_rejected,
    record_memory_rejected_for_operator, record_memory_repeated, record_memory_used,
    FeedbackApplyResult, DEFAULT_IDLE_BATCH, DEFAULT_IDLE_DAYS,
};
pub use normalize::normalize_content;
pub use redact::{redact_secrets, RedactionOutcome};
pub use retrieve::{
    format_planner_suggestions, format_untrusted_memory_block, format_untrusted_memory_notes,
    rank_for_query, retrieve_for_prompt, search_memory, select_within_budget, MemoryPromptEntry,
    RankedMemory, RetrieveRequest, SelectedMemory,
};
pub use service::{
    accept_memory_item, accept_memory_item_for_operator, admit_memory_item, gate_after_admit,
    promote_memory_item, promote_memory_item_for_operator, reject_memory_item,
    reject_memory_item_for_operator, AdmitOutcome, ExistingMemory, MemoryError, MemoryService,
    PreparedMemoryItem,
};

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_storage::{
        MemoryKind, MemoryScope, MemoryStatus, NewMemoryItem, LOCAL_OPERATOR_SCOPE_KEY,
    };

    #[test]
    fn redacts_common_secrets() {
        let raw = "token=ghp_abcdefghijklmnopqrstuvwx and key sk-abc123SECRETKEY999";
        let outcome = redact_secrets(raw);
        assert!(outcome.redacted);
        assert!(!outcome.text.contains("ghp_"));
        assert!(!outcome.text.contains("sk-abc"));
        assert!(outcome.text.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_pem_private_keys() {
        let raw = "-----BEGIN PRIVATE KEY-----\nMIIE\n-----END PRIVATE KEY-----";
        let outcome = redact_secrets(raw);
        assert!(outcome.redacted);
        assert!(!outcome.text.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn drops_content_that_is_only_secrets() {
        let outcome = redact_secrets("password=hunter2");
        assert!(outcome.redacted);
        let normalized = normalize_content(&outcome.text);
        assert!(normalized.is_empty() || normalized == "[REDACTED]");
    }

    #[test]
    fn normalizes_whitespace_and_case_for_fingerprint() {
        assert_eq!(normalize_content("  Hello   WORLD \n"), "Hello WORLD");
        assert_eq!(
            content_fingerprint("Hello WORLD"),
            content_fingerprint("  hello   world  ")
        );
    }

    #[test]
    fn detects_exact_duplicates() {
        let existing = [existing("prefer typed api over raw fetch")];
        let hit = detect_duplicate("Prefer  typed API over raw fetch", &existing);
        assert_eq!(hit.map(|h| h.existing_id), Some(existing[0].id));
    }

    #[test]
    fn detects_semantic_duplicate_for_paraphrased_memory() {
        let existing_content = "prefer git worktrees for parallel agents";
        let existing_embedding = embed_text_hash(existing_content);
        let existing = [existing_with_embedding(
            existing_content,
            existing_embedding,
        )];
        let candidate = "use worktrees when running parallel agents";
        let candidate_embedding = embed_text_hash(candidate);
        let hit = detect_duplicate_with_embedding(
            candidate,
            Some(MemoryKind::Fact),
            Some(&candidate_embedding),
            &existing,
        );

        assert_eq!(hit.map(|h| h.existing_id), Some(existing[0].id));
    }

    #[test]
    fn keeps_unrelated_memory_when_embedding_is_not_similar() {
        let existing_content = "postgres connection pool size is 16";
        let existing_embedding = embed_text_hash(existing_content);
        let existing = [existing_with_embedding(
            existing_content,
            existing_embedding,
        )];
        let candidate = "use worktrees when running parallel agents";
        let candidate_embedding = embed_text_hash(candidate);

        let hit = detect_duplicate_with_embedding(
            candidate,
            Some(MemoryKind::Fact),
            Some(&candidate_embedding),
            &existing,
        );

        assert!(hit.is_none());
    }

    #[test]
    fn detects_opposing_constraint_conflict() {
        let existing = [existing_kind(
            MemoryKind::Constraint,
            "always run cargo fmt before commit",
        )];
        let hit = detect_conflict(
            MemoryKind::Constraint,
            "never run cargo fmt before commit",
            &existing,
        );
        assert!(hit.is_some());
    }

    #[test]
    fn non_opposing_facts_are_not_conflicts() {
        let existing = [existing("workspace uses postgres")];
        let hit = detect_conflict(MemoryKind::Fact, "frontend uses vite", &existing);
        assert!(hit.is_none());
    }

    fn existing(content: &str) -> ExistingMemory {
        existing_kind(MemoryKind::Fact, content)
    }

    fn existing_kind(kind: MemoryKind, content: &str) -> ExistingMemory {
        ExistingMemory {
            id: uuid::Uuid::new_v4(),
            kind,
            status: MemoryStatus::Active,
            content: content.to_string(),
            pinned: false,
            embedding: None,
            embedding_version: 0,
        }
    }

    fn existing_with_embedding(content: &str, embedding: Vec<f32>) -> ExistingMemory {
        ExistingMemory {
            embedding: Some(embedding),
            embedding_version: HASH_EMBEDDING_VERSION,
            ..existing(content)
        }
    }

    #[tokio::test]
    async fn prepare_rejects_empty_after_redaction() {
        let item = NewMemoryItem::candidate_fact(
            MemoryScope::Global,
            LOCAL_OPERATOR_SCOPE_KEY,
            "api_key=lr_supersecretvalue123456",
        );
        let prepared = MemoryService::prepare(&item).await;
        assert!(matches!(prepared, Err(MemoryError::Rejected { .. })));
    }

    #[tokio::test]
    async fn prepare_accepts_clean_fact() {
        let item = NewMemoryItem::candidate_fact(
            MemoryScope::Workspace,
            "repo-a",
            "  use  worktrees  for  parallel  agents  ",
        );
        let prepared = MemoryService::prepare(&item).await.expect("ok");
        assert_eq!(prepared.content, "use worktrees for parallel agents");
        assert!(!prepared.redacted);
    }

    #[tokio::test]
    async fn admit_inserts_and_dedupes_against_database() {
        let Some(pool) = evohime_storage::connect_integration_pool().await else {
            eprintln!("skipping admit integration test: database unavailable");
            return;
        };

        let scope_key = format!("mem-svc-{}", uuid::Uuid::new_v4());
        let item = NewMemoryItem::candidate_fact(
            MemoryScope::Workspace,
            &scope_key,
            "prefer worktrees for parallel agents",
        );

        let first = admit_memory_item(&pool, item.clone())
            .await
            .expect("admit 1");
        assert!(matches!(first, AdmitOutcome::Inserted(_)));

        let second = admit_memory_item(&pool, item).await.expect("admit 2");
        assert!(matches!(second, AdmitOutcome::Duplicate { .. }));

        let _ = evohime_storage::delete_memory_items_by_scope_key(&pool, &scope_key)
            .await
            .expect("cleanup");
    }
}
