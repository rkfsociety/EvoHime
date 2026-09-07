//! Bounded-context entry points for storage.
//!
//! Store modules remain available for source compatibility during migration,
//! but new code should depend on a domain facade instead of treating every
//! table as an independent subsystem.
//!
#![allow(ambiguous_glob_reexports)]
#![allow(ambiguous_glob_imports)]

pub mod memory {
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
    pub use crate::checkpoint_forking_store::*;
    pub use crate::continuation_store::*;
    pub use crate::task_checkpoint::*;
    pub use crate::workspace_state_checkpoint::*;
}

pub mod workflow {
    pub use crate::artifact_store::*;
    pub use crate::task_worktree_isolation_store::*;
    pub use crate::workflow_optimization_lab_store::*;
    pub use crate::workflow_package_store::*;
    pub use crate::workflow_store::*;
}

pub mod agents {
    pub use crate::child_store::*;
    pub use crate::persistent_agent_registry_store::*;
    pub use crate::retained_child_store::*;
}

pub mod audit {
    pub use crate::conversation_event_log_store::*;
    pub use crate::execution_ledger::*;
    pub use crate::reconciliation_verifier::*;
}

pub mod receipts {
    pub use crate::model_provenance::*;
}
