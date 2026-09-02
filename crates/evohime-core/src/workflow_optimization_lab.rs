//! Offline-only, bounded workflow strategy search (plan 71).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_ROUNDS: u32 = 32;
pub const MAX_CANDIDATES: u32 = 256;
pub const MAX_MUTATION_BYTES: usize = 64 * 1024;
pub const MAX_COST: u64 = 10_000_000;
pub const MAX_TOKENS: u64 = 1_000_000;
pub const MAX_WALL_MS: u64 = 30 * 60 * 1000;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Split {
    Train,
    Validation,
    Holdout,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunState {
    Draft,
    Searching,
    Validation,
    Holdout,
    AwaitingPromotion,
    Promoted,
    Rejected,
    Blocked,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Objective {
    pub quality_weight: u32,
    pub cost_weight: u32,
    pub latency_weight: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Candidate {
    pub id: String,
    pub parent_hash: String,
    pub mutations: serde_json::Value,
    pub version: u64,
    pub security_rejected: bool,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OptimizationRun {
    pub id: String,
    pub base_strategy_hash: String,
    pub benchmark_suite_hash: String,
    pub objective: Objective,
    pub constraints: Vec<String>,
    pub rounds: u32,
    pub state: RunState,
    pub policy_hash: String,
    pub content_hash: String,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid optimization contract: {0}")]
    Invalid(String),
    #[error("optimization limit exceeded")]
    Limit,
    #[error("security regression is a hard rejection")]
    SecurityRegression,
    #[error("holdout is immutable and may not drive mutation")]
    HoldoutMutation,
    #[error("unsupported optimization version")]
    UnsupportedVersion,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkEvaluationRequest {
    pub suite: crate::agent_benchmark_matrix::BenchmarkSuite,
    pub policy: crate::agent_benchmark_matrix::BenchmarkPolicy,
}
pub fn hash<T: Serialize>(v: &T) -> Result<String, Error> {
    let b = serde_json::to_vec(v).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(b))))
}
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
