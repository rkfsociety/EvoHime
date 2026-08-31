//! Core-owned contract and bounded aggregation for Agent Benchmark Matrix.
//!
//! The runner deliberately receives an executor instead of a provider or a
//! tool registry. This keeps benchmark orchestration unable to mint
//! capabilities and makes the deterministic test executor reproducible.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const CONTRACT_VERSION: u32 = 1;
pub const CONTRACT_ID: &str = "agent-benchmark-matrix-v1";
pub const MAX_CHALLENGES: usize = 256;
pub const MAX_PROFILES: usize = 64;
pub const MAX_ATTEMPTS: usize = 32;
pub const MAX_PARALLELISM: usize = 16;
pub const MAX_ID_CHARS: usize = 128;
pub const MAX_TEXT_CHARS: usize = 16_384;

pub trait BenchmarkExecutor {
    fn execute(
        &self,
        challenge: &BenchmarkChallenge,
        model: &ModelProfile,
        agent: &AgentProfile,
        seed: u64,
    ) -> AttemptResult;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DeterministicBenchmarkExecutor;

/// Executor used by the simulation runtime. It consumes only a fixture
/// reference and produces bounded synthetic metrics; it never reaches a
/// provider or ToolRegistry.
#[derive(Debug, Default, Clone, Copy)]
pub struct FixtureToolBenchmarkExecutor;

impl BenchmarkExecutor for FixtureToolBenchmarkExecutor {
    fn execute(
        &self,
        challenge: &BenchmarkChallenge,
        _model: &ModelProfile,
        _agent: &AgentProfile,
        seed: u64,
    ) -> AttemptResult {
        let available = challenge.fixture_ref.starts_with("fixture:");
        let digest = hex::encode(Sha256::digest(
            format!("{}:{seed}", challenge.fixture_ref).as_bytes(),
        ));
        AttemptResult {
            outcome: if available {
                AttemptOutcome::Passed
            } else {
                AttemptOutcome::Unavailable
            },
            failure_class: (!available).then_some(FailureClass::Infrastructure),
            security_violation: false,
            latency_ms: 1,
            steps: u32::from(available),
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_micros: 0,
            output_digest: if available {
                digest.clone()
            } else {
                String::new()
            },
            tool_trace_digest: if available { digest } else { String::new() },
        }
    }
}

impl BenchmarkExecutor for DeterministicBenchmarkExecutor {
    fn execute(
        &self,
        challenge: &BenchmarkChallenge,
        _model: &ModelProfile,
        _agent: &AgentProfile,
        seed: u64,
    ) -> AttemptResult {
        let digest = hex::encode(Sha256::digest(
            format!("{}:{seed}", challenge.id).as_bytes(),
        ));
        AttemptResult {
            outcome: AttemptOutcome::Passed,
            failure_class: None,
            security_violation: false,
            latency_ms: 1,
            steps: 1,
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_micros: 0,
            output_digest: digest.clone(),
            tool_trace_digest: digest,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableBenchmarkExecutor;

impl BenchmarkExecutor for UnavailableBenchmarkExecutor {
    fn execute(
        &self,
        _challenge: &BenchmarkChallenge,
        _model: &ModelProfile,
        _agent: &AgentProfile,
        _seed: u64,
    ) -> AttemptResult {
        AttemptResult {
            outcome: AttemptOutcome::Unavailable,
            failure_class: Some(FailureClass::Infrastructure),
            security_violation: false,
            latency_ms: 0,
            steps: 0,
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_micros: 0,
            output_digest: String::new(),
            tool_trace_digest: String::new(),
        }
    }
}

pub fn run_matrix<E: BenchmarkExecutor>(
    suite: &BenchmarkSuite,
    policy: &BenchmarkPolicy,
    run_id: &str,
    source_commit: &str,
    executor: &E,
    baselines: &BTreeMap<String, Baseline>,
) -> Result<BenchmarkReport, BenchmarkValidationError> {
    suite.validate()?;
    policy.validate()?;
    bounded("run_id", run_id)?;
    bounded("source_commit", source_commit)?;
    let combinations = suite.challenges.len() as u64
        * suite.model_profiles.len() as u64
        * suite.agent_profiles.len() as u64
        * policy.attempts as u64;
    if combinations > 16_384 {
        return Err(BenchmarkValidationError::Limit("matrix_size".into()));
    }
    let mut metrics = BTreeMap::new();
    let mut comparisons = BTreeMap::new();
    for challenge in &suite.challenges {
        for model in &suite.model_profiles {
            for agent in &suite.agent_profiles {
                let key = format!("{}:{}:{}", challenge.id, model.id, agent.id);
                let attempts = (0..policy.attempts as u64)
                    .map(|attempt| {
                        executor.execute(challenge, model, agent, policy.seed.wrapping_add(attempt))
                    })
                    .collect::<Vec<_>>();
                let result = aggregate_attempts(&attempts);
                let baseline = baselines.get(&key);
                let comparison = if result.completed == 0 && baseline.is_none() {
                    BenchmarkComparison {
                        verdict: ComparisonVerdict::Blocked,
                        security_hard_failure: false,
                        reason: "no completed attempts".into(),
                    }
                } else {
                    compare_metrics(&result, baseline, suite.thresholds)
                };
                metrics.insert(key.clone(), result);
                comparisons.insert(key, comparison);
            }
        }
    }
    Ok(BenchmarkReport {
        contract_id: CONTRACT_ID.into(),
        contract_hash: hex::encode(Sha256::digest(CONTRACT_ID.as_bytes())),
        run_id: run_id.into(),
        source_commit: source_commit.into(),
        suite_id: suite.id.clone(),
        suite_version: suite.version.clone(),
        model_profile_ids: suite.model_profiles.iter().map(|v| v.id.clone()).collect(),
        agent_profile_ids: suite.agent_profiles.iter().map(|v| v.id.clone()).collect(),
        metrics,
        comparisons,
        redaction_status: "redacted".into(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkChallenge {
    pub id: String,
    pub version: String,
    pub category: String,
    pub objective: String,
    pub fixture_ref: String,
    pub success_evaluator: String,
    pub setup_profile: String,
    pub dependencies: Vec<String>,
    pub tags: Vec<String>,
    pub synthetic_only: bool,
    pub max_steps: u32,
    pub max_tokens: Option<u32>,
    pub max_cost_micros: Option<u64>,
    pub timeout_ms: u64,
    pub set: BenchmarkSet,
    pub security: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkSet {
    Maintain,
    Improve,
    Explore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelProfile {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub temperature_millis: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub routing_profile: Option<String>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentProfile {
    pub id: String,
    pub prompt_version: String,
    pub memory_policy_version: String,
    pub context_policy_version: String,
    pub tool_routing_version: String,
    pub child_policy_version: Option<String>,
    pub continuation_policy_version: Option<String>,
    pub skills_set_hash: Option<String>,
    pub refinement_state_hash: Option<String>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkSuite {
    pub id: String,
    pub version: String,
    pub challenges: Vec<BenchmarkChallenge>,
    pub model_profiles: Vec<ModelProfile>,
    pub agent_profiles: Vec<AgentProfile>,
    pub thresholds: Thresholds,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Thresholds {
    pub min_pass_rate_millis: u32,
    pub max_latency_p95_ms: Option<u64>,
    pub max_cost_p95_micros: Option<u64>,
    pub max_security_failures: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkPolicy {
    pub attempts: u16,
    pub max_parallelism: u16,
    pub seed: u64,
    pub global_token_budget: Option<u64>,
    pub global_cost_budget_micros: Option<u64>,
    pub mode: BenchmarkMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkMode {
    Deterministic,
    Real,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttemptOutcome {
    Passed,
    Failed,
    PrerequisiteFailed,
    Skipped,
    Unavailable,
    Unknown,
    Blocked,
    Invalid,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    Reasoning,
    WrongTool,
    InvalidArguments,
    Permission,
    ApprovalViolation,
    HallucinatedCapability,
    Timeout,
    BudgetExceeded,
    Provider,
    Infrastructure,
    Recovery,
    Evaluator,
    Security,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttemptResult {
    pub outcome: AttemptOutcome,
    pub failure_class: Option<FailureClass>,
    pub security_violation: bool,
    pub latency_ms: u64,
    pub steps: u32,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub cost_micros: u64,
    pub output_digest: String,
    pub tool_trace_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Metrics {
    pub attempts: u32,
    pub completed: u32,
    pub passed: u32,
    pub pass_rate_millis: u32,
    pub timeout_count: u32,
    pub security_failures: u32,
    pub p50_latency_ms: Option<u64>,
    pub p95_latency_ms: Option<u64>,
    pub p99_latency_ms: Option<u64>,
    pub p50_cost_micros: Option<u64>,
    pub p95_cost_micros: Option<u64>,
    pub p99_cost_micros: Option<u64>,
    pub failure_classes: BTreeMap<FailureClass, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Baseline {
    pub id: String,
    pub suite_version: String,
    pub challenge_id: String,
    pub model_profile_hash: String,
    pub agent_profile_hash: String,
    pub metrics: Metrics,
    pub source_commit: String,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonVerdict {
    Improved,
    Stable,
    Regressed,
    Inconclusive,
    New,
    Blocked,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkComparison {
    pub verdict: ComparisonVerdict,
    pub security_hard_failure: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkReport {
    pub contract_id: String,
    pub contract_hash: String,
    pub run_id: String,
    pub source_commit: String,
    pub suite_id: String,
    pub suite_version: String,
    pub model_profile_ids: Vec<String>,
    pub agent_profile_ids: Vec<String>,
    pub metrics: BTreeMap<String, Metrics>,
    pub comparisons: BTreeMap<String, BenchmarkComparison>,
    pub redaction_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BenchmarkValidationError {
    InvalidField(String),
    UnsupportedVersion(u32),
    Limit(String),
    SensitiveField(String),
    Duplicate(String),
}

impl std::fmt::Display for BenchmarkValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidField(v) => write!(f, "invalid field: {v}"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported version: {v}"),
            Self::Limit(v) => write!(f, "limit exceeded: {v}"),
            Self::SensitiveField(v) => write!(f, "sensitive field: {v}"),
            Self::Duplicate(v) => write!(f, "duplicate: {v}"),
        }
    }
}
impl std::error::Error for BenchmarkValidationError {}

fn bounded(name: &str, value: &str) -> Result<(), BenchmarkValidationError> {
    if value.is_empty() || value.chars().count() > MAX_ID_CHARS {
        return Err(BenchmarkValidationError::InvalidField(name.into()));
    }
    Ok(())
}
fn bounded_text(name: &str, value: &str) -> Result<(), BenchmarkValidationError> {
    if value.is_empty() || value.chars().count() > MAX_TEXT_CHARS {
        return Err(BenchmarkValidationError::InvalidField(name.into()));
    }
    Ok(())
}
fn valid_hash(name: &str, value: &str) -> Result<(), BenchmarkValidationError> {
    bounded(name, value)?;
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(BenchmarkValidationError::InvalidField(name.into()));
    }
    Ok(())
}

impl BenchmarkChallenge {
    pub fn validate(&self) -> Result<(), BenchmarkValidationError> {
        bounded("challenge.id", &self.id)?;
        bounded("challenge.version", &self.version)?;
        bounded("challenge.category", &self.category)?;
        bounded_text("challenge.objective", &self.objective)?;
        bounded("challenge.fixture_ref", &self.fixture_ref)?;
        bounded("challenge.success_evaluator", &self.success_evaluator)?;
        bounded("challenge.setup_profile", &self.setup_profile)?;
        if self.max_steps == 0 || self.timeout_ms == 0 {
            return Err(BenchmarkValidationError::Limit(self.id.clone()));
        }
        if self.dependencies.len() > MAX_CHALLENGES || self.tags.len() > MAX_CHALLENGES {
            return Err(BenchmarkValidationError::Limit(self.id.clone()));
        }
        Ok(())
    }
}

impl ModelProfile {
    pub fn validate(&self) -> Result<(), BenchmarkValidationError> {
        bounded("model.id", &self.id)?;
        bounded("model.provider", &self.provider)?;
        bounded("model.model", &self.model)?;
        valid_hash("model.content_hash", &self.content_hash)
    }
}
impl AgentProfile {
    pub fn validate(&self) -> Result<(), BenchmarkValidationError> {
        bounded("agent.id", &self.id)?;
        bounded("agent.prompt_version", &self.prompt_version)?;
        bounded("agent.memory_policy_version", &self.memory_policy_version)?;
        bounded("agent.context_policy_version", &self.context_policy_version)?;
        bounded("agent.tool_routing_version", &self.tool_routing_version)?;
        valid_hash("agent.content_hash", &self.content_hash)
    }
}

impl BenchmarkSuite {
    pub fn validate(&self) -> Result<(), BenchmarkValidationError> {
        bounded("suite.id", &self.id)?;
        bounded("suite.version", &self.version)?;
        if self.challenges.is_empty() || self.challenges.len() > MAX_CHALLENGES {
            return Err(BenchmarkValidationError::Limit("challenges".into()));
        }
        if self.model_profiles.is_empty() || self.model_profiles.len() > MAX_PROFILES {
            return Err(BenchmarkValidationError::Limit("model_profiles".into()));
        }
        if self.agent_profiles.is_empty() || self.agent_profiles.len() > MAX_PROFILES {
            return Err(BenchmarkValidationError::Limit("agent_profiles".into()));
        }
        let mut ids = BTreeSet::new();
        for challenge in &self.challenges {
            challenge.validate()?;
            if !ids.insert(challenge.id.clone()) {
                return Err(BenchmarkValidationError::Duplicate(challenge.id.clone()));
            }
        }
        for profile in &self.model_profiles {
            profile.validate()?;
        }
        for profile in &self.agent_profiles {
            profile.validate()?;
        }
        if self.thresholds.min_pass_rate_millis > 1000 {
            return Err(BenchmarkValidationError::Limit(
                "min_pass_rate_millis".into(),
            ));
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("benchmark suite serializes");
        hex::encode(Sha256::digest(bytes))
    }
}

impl BenchmarkPolicy {
    pub fn validate(&self) -> Result<(), BenchmarkValidationError> {
        if self.attempts == 0 || self.attempts as usize > MAX_ATTEMPTS {
            return Err(BenchmarkValidationError::Limit("attempts".into()));
        }
        if self.max_parallelism == 0 || self.max_parallelism as usize > MAX_PARALLELISM {
            return Err(BenchmarkValidationError::Limit("max_parallelism".into()));
        }
        Ok(())
    }
}

pub fn aggregate_attempts(attempts: &[AttemptResult]) -> Metrics {
    let mut metrics = Metrics {
        attempts: attempts.len() as u32,
        ..Metrics::default()
    };
    let mut latencies = Vec::new();
    let mut costs = Vec::new();
    for result in attempts {
        if result.outcome == AttemptOutcome::Passed {
            metrics.passed += 1;
        }
        if matches!(
            result.outcome,
            AttemptOutcome::Passed | AttemptOutcome::Failed
        ) {
            metrics.completed += 1;
            latencies.push(result.latency_ms);
            costs.push(result.cost_micros);
        }
        if result.outcome == AttemptOutcome::Unknown
            || result.outcome == AttemptOutcome::Unavailable
        {
            // Non-completed attempts are deliberately excluded from pass-rate.
        }
        if result.failure_class == Some(FailureClass::Timeout) {
            metrics.timeout_count += 1;
        }
        if result.security_violation || result.failure_class == Some(FailureClass::Security) {
            metrics.security_failures += 1;
        }
        if let Some(class) = result.failure_class {
            *metrics.failure_classes.entry(class).or_default() += 1;
        }
    }
    if metrics.completed > 0 {
        metrics.pass_rate_millis = metrics
            .passed
            .saturating_mul(1000)
            .checked_div(metrics.completed)
            .unwrap_or(0);
    }
    metrics.p50_latency_ms = percentile(&mut latencies.clone(), 50);
    metrics.p95_latency_ms = percentile(&mut latencies, 95);
    metrics.p99_latency_ms = percentile(&mut latencies, 99);
    metrics.p50_cost_micros = percentile(&mut costs.clone(), 50);
    metrics.p95_cost_micros = percentile(&mut costs, 95);
    metrics.p99_cost_micros = percentile(&mut costs, 99);
    metrics
}

fn percentile(values: &mut [u64], percentile: u64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let index = (values.len() * percentile as usize).div_ceil(100) - 1;
    values.get(index.min(values.len() - 1)).copied()
}

pub fn compare_metrics(
    current: &Metrics,
    baseline: Option<&Baseline>,
    thresholds: Thresholds,
) -> BenchmarkComparison {
    if current.security_failures > thresholds.max_security_failures {
        return BenchmarkComparison {
            verdict: ComparisonVerdict::Regressed,
            security_hard_failure: true,
            reason: "security regression".into(),
        };
    }
    let Some(baseline) = baseline else {
        return BenchmarkComparison {
            verdict: ComparisonVerdict::New,
            security_hard_failure: false,
            reason: "no compatible baseline".into(),
        };
    };
    if current.completed == 0 {
        return BenchmarkComparison {
            verdict: ComparisonVerdict::Inconclusive,
            security_hard_failure: false,
            reason: "no completed attempts".into(),
        };
    }
    if current.pass_rate_millis < thresholds.min_pass_rate_millis {
        return BenchmarkComparison {
            verdict: ComparisonVerdict::Regressed,
            security_hard_failure: false,
            reason: "pass rate below threshold".into(),
        };
    }
    if thresholds
        .max_latency_p95_ms
        .is_some_and(|max| current.p95_latency_ms.unwrap_or(u64::MAX) > max)
        || thresholds
            .max_cost_p95_micros
            .is_some_and(|max| current.p95_cost_micros.unwrap_or(u64::MAX) > max)
    {
        return BenchmarkComparison {
            verdict: ComparisonVerdict::Regressed,
            security_hard_failure: false,
            reason: "cost or latency threshold exceeded".into(),
        };
    }
    let improved = current.pass_rate_millis > baseline.metrics.pass_rate_millis
        && current.p95_latency_ms <= baseline.metrics.p95_latency_ms;
    BenchmarkComparison {
        verdict: if improved {
            ComparisonVerdict::Improved
        } else {
            ComparisonVerdict::Stable
        },
        security_hard_failure: false,
        reason: "compatible baseline comparison".into(),
    }
}

pub fn redact_report(report: &BenchmarkReport) -> Result<Value, BenchmarkValidationError> {
    if report.redaction_status != "redacted" {
        return Err(BenchmarkValidationError::SensitiveField(
            "redaction_status".into(),
        ));
    }
    serde_json::to_value(report)
        .map_err(|_| BenchmarkValidationError::InvalidField("report".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(outcome: AttemptOutcome, latency_ms: u64, cost_micros: u64) -> AttemptResult {
        AttemptResult {
            outcome,
            failure_class: None,
            security_violation: false,
            latency_ms,
            steps: 1,
            prompt_tokens: 2,
            completion_tokens: 3,
            cost_micros,
            output_digest: "a".into(),
            tool_trace_digest: "b".into(),
        }
    }

    #[test]
    fn aggregates_completed_only_and_uses_percentiles() {
        let values = vec![
            result(AttemptOutcome::Passed, 10, 100),
            result(AttemptOutcome::Failed, 20, 200),
            result(AttemptOutcome::Unknown, 999, 999),
        ];
        let metrics = aggregate_attempts(&values);
        assert_eq!(metrics.completed, 2);
        assert_eq!(metrics.pass_rate_millis, 500);
        assert_eq!(metrics.p50_latency_ms, Some(10));
        assert_eq!(metrics.p95_latency_ms, Some(20));
    }

    #[test]
    fn security_regression_is_hard_failure() {
        let current = Metrics {
            security_failures: 1,
            ..Metrics::default()
        };
        let comparison = compare_metrics(
            &current,
            None,
            Thresholds {
                min_pass_rate_millis: 0,
                max_latency_p95_ms: None,
                max_cost_p95_micros: None,
                max_security_failures: 0,
            },
        );
        assert_eq!(comparison.verdict, ComparisonVerdict::Regressed);
        assert!(comparison.security_hard_failure);
    }

    #[test]
    fn unknown_is_inconclusive_and_not_success() {
        let metrics = aggregate_attempts(&[result(AttemptOutcome::Unknown, 10, 1)]);
        let comparison = compare_metrics(
            &metrics,
            Some(&Baseline {
                id: "b".into(),
                suite_version: "1".into(),
                challenge_id: "c".into(),
                model_profile_hash: "m".into(),
                agent_profile_hash: "a".into(),
                metrics: Metrics::default(),
                source_commit: "c".into(),
                revision: 1,
            }),
            Thresholds {
                min_pass_rate_millis: 0,
                max_latency_p95_ms: None,
                max_cost_p95_micros: None,
                max_security_failures: 0,
            },
        );
        assert_eq!(comparison.verdict, ComparisonVerdict::Inconclusive);
    }

    fn suite() -> BenchmarkSuite {
        let model = ModelProfile {
            id: "m".into(),
            provider: "mock".into(),
            model: "m".into(),
            reasoning_effort: None,
            temperature_millis: Some(0),
            max_output_tokens: Some(32),
            routing_profile: None,
            content_hash: "a".repeat(64),
        };
        let agent = AgentProfile {
            id: "a".into(),
            prompt_version: "1".into(),
            memory_policy_version: "1".into(),
            context_policy_version: "1".into(),
            tool_routing_version: "1".into(),
            child_policy_version: None,
            continuation_policy_version: None,
            skills_set_hash: None,
            refinement_state_hash: None,
            content_hash: "b".repeat(64),
        };
        BenchmarkSuite {
            id: "s".into(),
            version: "1".into(),
            challenges: vec![BenchmarkChallenge {
                id: "c".into(),
                version: "1".into(),
                category: "tool_selection".into(),
                objective: "synthetic".into(),
                fixture_ref: "synthetic://c".into(),
                success_evaluator: "structured_rule".into(),
                setup_profile: "empty".into(),
                dependencies: vec![],
                tags: vec!["maintain".into()],
                synthetic_only: true,
                max_steps: 1,
                max_tokens: None,
                max_cost_micros: None,
                timeout_ms: 100,
                set: BenchmarkSet::Maintain,
                security: false,
            }],
            model_profiles: vec![model],
            agent_profiles: vec![agent],
            thresholds: Thresholds {
                min_pass_rate_millis: 1000,
                max_latency_p95_ms: None,
                max_cost_p95_micros: None,
                max_security_failures: 0,
            },
        }
    }

    #[test]
    fn matrix_runs_profiles_and_attempts_with_bounded_policy() {
        let report = run_matrix(
            &suite(),
            &BenchmarkPolicy {
                attempts: 3,
                max_parallelism: 2,
                seed: 7,
                global_token_budget: None,
                global_cost_budget_micros: None,
                mode: BenchmarkMode::Deterministic,
            },
            "run",
            "commit",
            &DeterministicBenchmarkExecutor,
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(report.metrics["c:m:a"].attempts, 3);
        assert_eq!(report.metrics["c:m:a"].pass_rate_millis, 1000);
        assert_eq!(report.comparisons["c:m:a"].verdict, ComparisonVerdict::New);
    }

    #[test]
    fn fixture_executor_is_available_only_for_fixture_refs() {
        let mut benchmark = suite();
        benchmark.challenges[0].fixture_ref = "fixture:echo-v1".into();
        let report = run_matrix(
            &benchmark,
            &BenchmarkPolicy {
                attempts: 1,
                max_parallelism: 1,
                seed: 9,
                global_token_budget: None,
                global_cost_budget_micros: None,
                mode: BenchmarkMode::Deterministic,
            },
            "run",
            "commit",
            &FixtureToolBenchmarkExecutor,
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(report.metrics["c:m:a"].pass_rate_millis, 1000);
    }

    #[test]
    fn unavailable_matrix_is_blocked_and_never_passes() {
        let report = run_matrix(
            &suite(),
            &BenchmarkPolicy {
                attempts: 3,
                max_parallelism: 1,
                seed: 0,
                global_token_budget: None,
                global_cost_budget_micros: None,
                mode: BenchmarkMode::Real,
            },
            "run",
            "commit",
            &UnavailableBenchmarkExecutor,
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(
            report.comparisons["c:m:a"].verdict,
            ComparisonVerdict::Blocked
        );
    }
}
