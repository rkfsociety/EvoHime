//! Bounded-context entry points for storage.
//!
//! Store modules remain available for source compatibility during migration,
//! but new code should depend on a domain facade instead of treating every
//! table as an independent subsystem.
//!
#![allow(ambiguous_glob_reexports)]
#![allow(ambiguous_glob_imports)]

pub mod memory {
    //! Facade for durable memory records, candidate publication, and recall views.
    pub use crate::memory_extraction_store::{
        acquire_source_lease, candidate_basis_for, candidate_slot_for, capture_candidate,
        capture_source, defer_expired_source_lease, finalize_candidate, finish_source, get_source,
        link_extractor_request, link_extractor_response, list_recoverable_sources,
        publish_candidate, CaptureCandidateInput, CaptureCandidateOutcome, CaptureSourceInput,
        CaptureSourceOutcome, FinalizeCandidateInput, MemoryExtractionOrigin,
        MemoryExtractionSourceRecord, MemoryExtractionSourceState, PublishOutcome,
        SourceLeaseOutcome, MAX_EXTRACTION_DEPTH, MAX_RECOVERABLE_SOURCES,
    };
    pub use crate::memory_store::{
        install_schema, InsertSessionNoteInput, MemoryExtractionFields, MemoryPrivacy,
        MemoryRecord, MemoryRecordInput, MemoryScope, MemoryStoreError, MemoryStoreSql,
        MAX_CONTENT_BYTES, MAX_EVIDENCE_REFS, MAX_ID_BYTES, MAX_PROVENANCE_BYTES, MAX_QUERY_BYTES,
        MAX_SCOPE_ID_BYTES, MAX_TIMESTAMP_BYTES, MAX_TITLE_BYTES, MAX_TTL_SECONDS,
    };
    pub use crate::memory_views_and_adaptive_recall_store::{
        install_schema as install_views_schema, load_view, save_recall, save_view, RecallInput,
        RecallRecord, ViewInput, ViewRecord,
    };
}

pub mod runs {
    //! Facade for checkpoints, continuations, and run-scoped persisted state.
    pub use crate::checkpoint_forking_store::*;
    pub use crate::continuation_store::*;
    pub use crate::task_checkpoint::*;
    pub use crate::workspace_state_checkpoint::*;
}

pub mod workflow {
    //! Facade for workflow packages, artifacts, optimization, and worktree isolation.
    pub use crate::artifact_store::*;
    pub use crate::task_worktree_isolation_store::*;
    pub use crate::workflow_optimization_lab_store::*;
    pub use crate::workflow_package_store::*;
    pub use crate::workflow_store::*;
}

pub mod image_generation {
    //! Facade for bounded image-generation job metadata.
    pub use crate::image_generation_store::*;
}

pub mod agents {
    //! Facade for child-agent and persistent-agent registry records.
    pub use crate::child_store::*;
    pub use crate::persistent_agent_registry_store::*;
    pub use crate::retained_child_store::*;
}

pub mod audit {
    //! Facade for conversation, execution-ledger, and reconciliation evidence.
    pub use crate::conversation_event_log_store::*;
    pub use crate::execution_ledger::*;
    pub use crate::reconciliation_verifier::*;
}

pub mod receipts {
    //! Facade for model-provenance receipt persistence.
    pub use crate::model_provenance::*;
}

pub mod evaluation {
    //! Facade for persisted benchmark evaluation runs and reports.
    pub use crate::benchmark_store::{
        get_baseline, get_baseline_approval, get_baseline_approval_by_key, get_run,
        get_run_policy_json, latest_baseline_revision, put_baseline, put_baseline_approval,
        save_report, save_run,
    };
}
