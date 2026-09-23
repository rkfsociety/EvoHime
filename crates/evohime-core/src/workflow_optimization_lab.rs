//! Offline-only, bounded workflow strategy search (plan 71).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
/// Current schema version for offline workflow optimization runs.
pub const CONTRACT_VERSION: u32 = 1;
/// Maximum number of search rounds in one optimization run.
pub const MAX_ROUNDS: u32 = 32;
/// Maximum number of candidates evaluated in one run.
pub const MAX_CANDIDATES: u32 = 256;
/// Maximum serialized mutation payload size.
pub const MAX_MUTATION_BYTES: usize = 64 * 1024;
/// Maximum cumulative cost units permitted by an evaluation policy.
pub const MAX_COST: u64 = 10_000_000;
/// Maximum token budget permitted by an evaluation policy.
pub const MAX_TOKENS: u64 = 1_000_000;
/// Maximum wall-clock budget permitted by an evaluation policy, in milliseconds.
pub const MAX_WALL_MS: u64 = 30 * 60 * 1000;
/// Dataset split used to separate candidate search from final evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Split {
    /// Data used to generate or tune candidates.
    Train,
    /// Data used to compare candidates during search.
    Validation,
    /// Immutable data reserved for final promotion evidence.
    Holdout,
}
/// Lifecycle state for an optimization run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunState {
    /// Run is defined but has not started searching.
    Draft,
    /// Candidate generation and search are in progress.
    Searching,
    /// Candidates are being evaluated on validation data.
    Validation,
    /// Final holdout evaluation is in progress.
    Holdout,
    /// Evidence is ready for an explicit promotion decision.
    AwaitingPromotion,
    /// A candidate was explicitly promoted.
    Promoted,
    /// Candidate or run was explicitly rejected.
    Rejected,
    /// A blocking condition prevents safe continuation.
    Blocked,
    /// State value is not recognized by this implementation.
    Unknown,
}
/// Relative weighting applied to quality, cost, and latency measurements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Objective {
    /// Weight assigned to benchmark quality.
    pub quality_weight: u32,
    /// Weight assigned to execution cost.
    pub cost_weight: u32,
    /// Weight assigned to execution latency.
    pub latency_weight: u32,
}
/// Candidate strategy revision evaluated against a benchmark suite.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Candidate {
    /// Stable identifier for the candidate.
    pub id: String,
    /// Digest of the candidate from which this candidate was derived.
    pub parent_hash: String,
    /// Bounded declarative changes to the strategy.
    pub mutations: serde_json::Value,
    /// Candidate revision number.
    pub version: u64,
    /// Whether security evidence rejected the candidate.
    pub security_rejected: bool,
    /// Digest of the candidate with this field cleared.
    pub content_hash: String,
}
/// Integrity-bound optimization run definition and current lifecycle state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OptimizationRun {
    /// Stable run identifier.
    pub id: String,
    /// Digest of the baseline strategy.
    pub base_strategy_hash: String,
    /// Digest identifying the benchmark suite.
    pub benchmark_suite_hash: String,
    /// Weights used to compare candidate outcomes.
    pub objective: Objective,
    /// Additional constraints applied to candidate evaluation.
    pub constraints: Vec<String>,
    /// Number of search rounds planned for this run.
    pub rounds: u32,
    /// Current optimization lifecycle state.
    pub state: RunState,
    /// Digest of the policy governing the run.
    pub policy_hash: String,
    /// Digest of the run with this field cleared.
    pub content_hash: String,
}
/// Contract, resource-limit, security, or split-integrity failure.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Required identity, digest, or promotion evidence is invalid.
    #[error("invalid optimization contract: {0}")]
    Invalid(String),
    /// Run or candidate exceeds a configured resource bound.
    #[error("optimization limit exceeded")]
    Limit,
    /// Security benchmark detected a hard regression.
    #[error("security regression is a hard rejection")]
    SecurityRegression,
    /// Candidate mutation attempted to use the immutable holdout split.
    #[error("holdout is immutable and may not drive mutation")]
    HoldoutMutation,
    /// Input contract version is unsupported.
    #[error("unsupported optimization version")]
    UnsupportedVersion,
}
/// Benchmark suite and policy used to evaluate a candidate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkEvaluationRequest {
    /// Benchmark cases used to compare candidates.
    pub suite: crate::agent_benchmark_matrix::BenchmarkSuite,
    /// Resource and scoring policy for the suite.
    pub policy: crate::agent_benchmark_matrix::BenchmarkPolicy,
}
/// Serializes a value and returns its SHA-256 integrity digest.
pub fn hash<T: Serialize>(v: &T) -> Result<String, Error> {
    let b = serde_json::to_vec(v).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(b))))
}
/// Validates run identity, bounds, and the canonical content digest.
pub fn validate_run(r: &OptimizationRun) -> Result<(), Error> {
    if r.id.is_empty()
        || r.base_strategy_hash.is_empty()
        || r.benchmark_suite_hash.is_empty()
        || r.policy_hash.is_empty()
    {
        return Err(Error::Invalid("identity".into()));
    }
    if r.rounds == 0 || r.rounds > MAX_ROUNDS {
        return Err(Error::Limit);
    };
    if r.constraints.len() > 64 {
        return Err(Error::Limit);
    };
    let mut c = r.clone();
    c.content_hash.clear();
    if r.content_hash != hash(&c)? {
        return Err(Error::Invalid("content_hash".into()));
    }
    Ok(())
}
/// Validates a candidate and rejects all mutation against the holdout split.
pub fn validate_candidate(c: &Candidate, split: Split) -> Result<(), Error> {
    if c.id.is_empty() || c.parent_hash.is_empty() {
        return Err(Error::Invalid("identity".into()));
    }
    if serde_json::to_vec(&c.mutations)
        .map_err(|e| Error::Invalid(e.to_string()))?
        .len()
        > MAX_MUTATION_BYTES
    {
        return Err(Error::Limit);
    };
    if c.security_rejected {
        return Err(Error::SecurityRegression);
    };
    if matches!(split, Split::Holdout) {
        return Err(Error::HoldoutMutation);
    };
    let mut x = c.clone();
    x.content_hash.clear();
    if c.content_hash != hash(&x)? {
        return Err(Error::Invalid("content_hash".into()));
    }
    Ok(())
}
/// Requires explicit approval and passing holdout evidence before promotion.
pub fn promotion_allowed(
    run: &OptimizationRun,
    candidate: &Candidate,
    explicit: bool,
    holdout_passed: bool,
) -> Result<(), Error> {
    validate_run(run)?;
    if !explicit || !holdout_passed {
        return Err(Error::Invalid(
            "explicit promotion and holdout pass required".into(),
        ));
    }
    if candidate.security_rejected {
        return Err(Error::SecurityRegression);
    };
    Ok(())
}
/// Evaluates a candidate with the deterministic fixture-backed benchmark executor.
pub fn evaluate_candidate(
    run_id: &str,
    candidate: &Candidate,
    request: &BenchmarkEvaluationRequest,
) -> Result<crate::agent_benchmark_matrix::BenchmarkReport, Error> {
    validate_candidate(candidate, Split::Validation)?;
    let report = crate::agent_benchmark_matrix::run_matrix(
        &request.suite,
        &request.policy,
        run_id,
        &candidate.content_hash,
        &crate::agent_benchmark_matrix::FixtureToolBenchmarkExecutor,
        &BTreeMap::new(),
    )
    .map_err(|e| Error::Invalid(e.to_string()))?;
    if report.comparisons.values().any(|v| v.security_hard_failure) {
        return Err(Error::SecurityRegression);
    }
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn run() -> OptimizationRun {
        let mut r = OptimizationRun {
            id: "r".into(),
            base_strategy_hash: "b".into(),
            benchmark_suite_hash: "s".into(),
            objective: Objective {
                quality_weight: 1,
                cost_weight: 1,
                latency_weight: 1,
            },
            constraints: vec![],
            rounds: 2,
            state: RunState::Draft,
            policy_hash: "p".into(),
            content_hash: String::new(),
        };
        let mut c = r.clone();
        c.content_hash.clear();
        r.content_hash = hash(&c).unwrap();
        r
    }
    #[test]
    fn bounds_and_promotion() {
        assert!(validate_run(&run()).is_ok());
        assert!(promotion_allowed(
            &run(),
            &Candidate {
                id: "c".into(),
                parent_hash: "p".into(),
                mutations: serde_json::json!({}),
                version: 1,
                security_rejected: false,
                content_hash: hash(&Candidate {
                    id: "c".into(),
                    parent_hash: "p".into(),
                    mutations: serde_json::json!({}),
                    version: 1,
                    security_rejected: false,
                    content_hash: String::new()
                })
                .unwrap()
            },
            true,
            true
        )
        .is_ok())
    }
    #[test]
    fn holdout_cannot_mutate() {
        let c = Candidate {
            id: "c".into(),
            parent_hash: "p".into(),
            mutations: serde_json::json!({}),
            version: 1,
            security_rejected: false,
            content_hash: String::new(),
        };
        assert_eq!(
            validate_candidate(&c, Split::Holdout),
            Err(Error::HoldoutMutation)
        )
    }
}
