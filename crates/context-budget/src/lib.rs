//! Context Budget Manager (план 01).
//!
//! Владелец состояния и политики — Rust Core. Этот crate содержит контракты и
//! детерминированную логику сборки контекста:
//!
//! - [`budget`], [`profile`], [`estimator`], [`item`], [`hash`], [`ladder`],
//!   [`ledger`], [`metrics`], [`planner`] — этап 01.1;
//! - [`scratchpad`], [`artifact`] — этап 01.2;
//! - [`compression`] — этап 01.3;
//! - [`loadout`] — этап 01.4.
//!
//! Наружу (в IPC и UI, этап 01.5) уходит только bounded read-only projection:
//! состав контекста, причины сокращения и `context_ledger_hash`. Сырой prompt,
//! тело памяти и raw tool output не покидают Core.

pub mod artifact;
pub mod budget;
pub mod compression;
pub mod estimator;
pub mod hash;
pub mod item;
pub mod ladder;
pub mod ledger;
pub mod loadout;
pub mod metrics;
pub mod planner;
pub mod profile;
pub mod scratchpad;

pub use budget::{
    BudgetUnavailable, BudgetUnavailableStage, CategoryBudget, ContextBudget, MandatoryPart,
    MinimumViableContext, BUDGET_UNAVAILABLE_CODE, CONTEXT_BUDGET_SCHEMA_VERSION,
};
pub use estimator::{
    EstimateCache, EstimatorDrift, FallbackEstimator, HeuristicEstimator, TokenEstimator,
};
pub use hash::{content_hash, ContentForm, NORMALIZER_VERSION};
pub use item::{
    BudgetCategory, ContextItem, ContextItemBuilder, DropReason, ItemKind, Privacy,
    ScratchpadStatus, Trust, CONTEXT_ITEM_SCHEMA_VERSION,
};
pub use ladder::{
    LadderDiagnostic, LadderLevel, LadderOutcome, OffloadOutcome, OffloadSink, Selection,
    Summarizer, SummaryOutcome,
};
pub use ledger::{
    CompressionRecord, ContextLedgerEntry, ContextLedgerUsage, DroppedItemRecord, LedgerOutcome,
    LoadoutRecord, MandatoryPartRecord, SelectedItemRecord, CONTEXT_LEDGER_SCHEMA_VERSION,
};
pub use loadout::{
    build_loadout, check_tool_call, route_intent, IntentDecision, IntentRules, LoadoutLimits,
    LoadoutMiss, ToolGroup, ToolLoadout, ToolRegistryEntry,
};
pub use metrics::ContextMetrics;
pub use planner::{ContextPlan, ContextPlanner, OwnedContent, PlanInput, PlanRequest};
pub use profile::{
    ModelContextProfile, ProfileCatalog, ProfileError, MODEL_CONTEXT_PROFILE_SCHEMA_VERSION,
    STRATEGY_VERSION,
};
pub use scratchpad::{
    recover_after_restart, wrap_external_output, ConfirmationBasis, RecoveryPolicy,
    ScratchpadCategory, ScratchpadEntry,
};
