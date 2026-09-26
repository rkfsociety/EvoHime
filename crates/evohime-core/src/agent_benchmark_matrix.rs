//! Core-owned contract and bounded aggregation for Agent Benchmark Matrix.
//!
//! The runner deliberately receives an executor instead of a provider or a
//! tool registry. This keeps benchmark orchestration unable to mint
//! capabilities and makes the deterministic test executor reproducible.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Serialized contract version for the benchmark matrix.
pub const CONTRACT_VERSION: u32 = 1;
/// Stable identifier for the benchmark matrix contract.
pub const CONTRACT_ID: &str = "agent-benchmark-matrix-v1";
/// Maximum number of challenges in one suite.
pub const MAX_CHALLENGES: usize = 256;
/// Maximum model or agent profiles in one suite.
pub const MAX_PROFILES: usize = 64;
/// Maximum attempts for each challenge/profile combination.
pub const MAX_ATTEMPTS: usize = 32;
/// Maximum requested benchmark parallelism.
pub const MAX_PARALLELISM: usize = 16;
/// Maximum identifier field length in Unicode scalar values.
pub const MAX_ID_CHARS: usize = 128;
/// Maximum free-text field length in Unicode scalar values.
pub const MAX_TEXT_CHARS: usize = 16_384;

/// Executes one benchmark challenge without granting access to provider or tool registries.
pub trait BenchmarkExecutor {
    /// Runs one seeded attempt for the supplied challenge and profile pair.
    fn execute(
        &self,
        challenge: &BenchmarkChallenge,
        model: &ModelProfile,
        agent: &AgentProfile,
        seed: u64,
    ) -> AttemptResult;
}

/// Deterministic synthetic executor used for reproducible contract checks.
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

/// Executor implementation that marks attempts unavailable without side effects.
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

/// Runs every challenge/model/agent combination with a bounded deterministic seed sequence.
///
/// The caller supplies the executor, so the matrix layer does not create
/// provider credentials or tool capabilities. Output digests and comparison
/// records contain bounded metrics and redaction status.
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

/// Runs a bounded challenge matrix against the currently supervised local model.
///
/// The only evaluator accepted by this adapter is `sha256:<digest>`, which
/// compares a registered expected-output digest with bytes returned by real
/// inference. Synthetic suites, setup dependencies, and security challenges
/// are rejected because this executor does not provide their required owners.
pub async fn run_local_model_matrix(
    suite: &BenchmarkSuite,
    policy: &BenchmarkPolicy,
    run_id: &str,
    source_commit: &str,
    port: u16,
    model_alias: &str,
    baselines: &BTreeMap<String, Baseline>,
) -> Result<BenchmarkReport, BenchmarkValidationError> {
    suite.validate()?;
    policy.validate()?;
    bounded("run_id", run_id)?;
    bounded("source_commit", source_commit)?;
    if policy.mode != BenchmarkMode::Real
        || port == 0
        || policy.global_token_budget.is_some()
        || policy.global_cost_budget_micros.is_some()
        || suite.thresholds.max_cost_p95_micros.is_some()
    {
        return Err(BenchmarkValidationError::InvalidField("real_executor".into()));
    }
    let combinations = suite.challenges.len() as u64
        * suite.model_profiles.len() as u64
        * suite.agent_profiles.len() as u64
        * policy.attempts as u64;
    if combinations > 16_384 {
        return Err(BenchmarkValidationError::Limit("matrix_size".into()));
    }
    if suite.challenges.iter().any(|challenge| {
        challenge.synthetic_only
            || challenge.security
            || !challenge.dependencies.is_empty()
            || challenge.max_cost_micros.is_some()
            || challenge.timeout_ms < 300_000
            || challenge.max_tokens.is_some_and(|value| value == 0 || value > 4096)
    }) {
        return Err(BenchmarkValidationError::InvalidField("unsupported_challenge".into()));
    }
    if suite.model_profiles.iter().any(|profile| {
        profile.max_output_tokens.is_some_and(|value| value == 0 || value > 4096)
    }) {
        return Err(BenchmarkValidationError::InvalidField("unsupported_model_budget".into()));
    }
    // This adapter supervises exactly one local model. Running a multi-model
    // suite here would attribute the same runtime's output to profiles that
    // were never actually executed. The stable profile identity describes the
    // benchmark configuration; the supervised alias is intentionally derived
    // from this run's verified artifact and is not part of the frozen suite.
    if suite.model_profiles.len() != 1 {
        return Err(BenchmarkValidationError::InvalidField("local_model_profile_count".into()));
    }
    let mut metrics = BTreeMap::new();
    let mut comparisons = BTreeMap::new();
    for challenge in &suite.challenges {
        let expected_digest = challenge
            .success_evaluator
            .strip_prefix("sha256:")
            .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| BenchmarkValidationError::InvalidField("unsupported_evaluator".into()))?;
        for model in &suite.model_profiles {
            for agent in &suite.agent_profiles {
                let key = format!("{}:{}:{}", challenge.id, model.id, agent.id);
                let mut attempts = Vec::with_capacity(policy.attempts as usize);
                for _ in 0..policy.attempts {
                    let max_tokens = challenge
                        .max_tokens
                        .into_iter()
                        .chain(model.max_output_tokens)
                        .min()
                        .unwrap_or(256);
                    let inference = crate::local_model_adaptation::local_inference_stream(
                        port,
                        model_alias,
                        &challenge.objective,
                        max_tokens,
                        None,
                    )
                    .await;
                    let result = match inference {
                        Ok(evidence) => {
                            let timed_out = evidence.latency_ms > challenge.timeout_ms;
                            let passed = !timed_out
                                && evidence.completion_sha256.eq_ignore_ascii_case(expected_digest);
                            AttemptResult {
                            outcome: if passed { AttemptOutcome::Passed } else { AttemptOutcome::Failed },
                            failure_class: if timed_out {
                                Some(FailureClass::Timeout)
                            } else if passed {
                                None
                            } else {
                                Some(FailureClass::Evaluator)
                            },
                            security_violation: false,
                            latency_ms: evidence.latency_ms,
                            steps: 1,
                            prompt_tokens: evidence.prompt_tokens.unwrap_or(0).min(u32::MAX as u64) as u32,
                            completion_tokens: evidence.completion_tokens.unwrap_or(0).min(u32::MAX as u64) as u32,
                            cost_micros: 0,
                            output_digest: evidence.completion_sha256,
                            tool_trace_digest: String::new(),
                        }
                        },
                        Err(_) => AttemptResult {
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
                        },
                    };
                    attempts.push(result);
                }
                let result = aggregate_attempts(&attempts);
                let baseline = baselines.get(&key);
                if baseline.is_some_and(|baseline| {
                    baseline.suite_version != suite.version
                        || baseline.challenge_id != challenge.id
                        || baseline.model_profile_hash != model.content_hash
                        || baseline.agent_profile_hash != agent.content_hash
                        || baseline.revision == 0
                        || baseline.metrics.attempts == 0
                }) {
                    return Err(BenchmarkValidationError::InvalidField("incompatible_baseline".into()));
                }
                // A real run with no approved baseline is durable evidence for
                // the explicit approveBaseline flow, but remains New and can
                // never by itself unlock adaptation promotion.
                let comparison = compare_metrics(&result, baseline, suite.thresholds);
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
        model_profile_ids: suite.model_profiles.iter().map(|profile| profile.id.clone()).collect(),
        agent_profile_ids: suite.agent_profiles.iter().map(|profile| profile.id.clone()).collect(),
        metrics,
        comparisons,
        redaction_status: "redacted".into(),
    })
}

/// Versioned benchmark task definition with synthetic or external fixture binding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkChallenge {
    /// Stable challenge identifier.
    pub id: String,
    /// Challenge definition version.
    pub version: String,
    /// Challenge category used for reporting.
    pub category: String,
    /// Objective supplied to the benchmarked agent.
    pub objective: String,
    /// Fixture reference consumed by the injected executor.
    pub fixture_ref: String,
    /// Registered success-evaluator reference.
    pub success_evaluator: String,
    /// Setup profile reference needed before execution.
    pub setup_profile: String,
    /// Other challenge identifiers that must be satisfied first.
    pub dependencies: Vec<String>,
    /// Labels used to group or filter challenges.
    pub tags: Vec<String>,
    /// Whether evaluation must use synthetic fixtures only.
    pub synthetic_only: bool,
    /// Maximum action steps for this challenge.
    pub max_steps: u32,
    /// Optional per-attempt token budget.
    pub max_tokens: Option<u32>,
    /// Optional per-attempt cost budget in micro-units.
    pub max_cost_micros: Option<u64>,
    /// Maximum execution time in milliseconds.
    pub timeout_ms: u64,
    /// Maintenance, improvement, or exploratory suite partition.
    pub set: BenchmarkSet,
    /// Whether the challenge is security-sensitive.
    pub security: bool,
}

/// Lifecycle lane used to group benchmark challenges.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkSet {
    /// Detects regressions in maintained behavior.
    Maintain,
    /// Measures progress on targeted improvements.
    Improve,
    /// Explores behavior beyond the current required baseline.
    Explore,
}

/// Immutable model configuration referenced by a benchmark suite.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelProfile {
    /// Stable profile identifier.
    pub id: String,
    /// Provider name or registry identity.
    pub provider: String,
    /// Model identifier within the provider.
    pub model: String,
    /// Optional reasoning effort setting.
    pub reasoning_effort: Option<String>,
    /// Optional temperature in thousandths.
    pub temperature_millis: Option<u32>,
    /// Optional maximum generated token count.
    pub max_output_tokens: Option<u32>,
    /// Optional routing profile reference.
    pub routing_profile: Option<String>,
    /// Digest of the canonical model profile.
    pub content_hash: String,
}

/// Snapshot of agent prompt, memory, context, and tool-routing configurations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentProfile {
    /// Stable profile identifier.
    pub id: String,
    /// Prompt configuration version.
    pub prompt_version: String,
    /// Memory policy version.
    pub memory_policy_version: String,
    /// Context policy version.
    pub context_policy_version: String,
    /// Tool-routing configuration version.
    pub tool_routing_version: String,
    /// Optional child-agent policy version.
    pub child_policy_version: Option<String>,
    /// Optional continuation policy version.
    pub continuation_policy_version: Option<String>,
    /// Optional digest of the installed skill set.
    pub skills_set_hash: Option<String>,
    /// Optional digest of the refinement state.
    pub refinement_state_hash: Option<String>,
    /// Digest of the canonical agent profile.
    pub content_hash: String,
}

/// One benchmark suite containing challenges and profile combinations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkSuite {
    /// Stable suite identifier.
    pub id: String,
    /// Suite version used to scope comparisons.
    pub version: String,
    /// Challenges included in the matrix.
    pub challenges: Vec<BenchmarkChallenge>,
    /// Model configurations included in the matrix.
    pub model_profiles: Vec<ModelProfile>,
    /// Agent configurations included in the matrix.
    pub agent_profiles: Vec<AgentProfile>,
    /// Pass-rate, latency, cost, and security thresholds.
    pub thresholds: Thresholds,
}

/// Acceptance thresholds applied to benchmark aggregates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Thresholds {
    /// Minimum pass rate in thousandths, from 0 to 1000.
    pub min_pass_rate_millis: u32,
    /// Optional maximum 95th-percentile latency in milliseconds.
    pub max_latency_p95_ms: Option<u64>,
    /// Optional maximum 95th-percentile cost in micro-units.
    pub max_cost_p95_micros: Option<u64>,
    /// Maximum tolerated security failures.
    pub max_security_failures: u32,
}

/// Deterministic attempt count, resource budgets, seed, and execution mode for a matrix run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkPolicy {
    /// Attempts executed for each challenge/model/agent combination.
    pub attempts: u16,
    /// Maximum requested parallel attempt count.
    pub max_parallelism: u16,
    /// Seed used to derive repeatable attempt seeds.
    pub seed: u64,
    /// Optional aggregate token budget for the full matrix.
    pub global_token_budget: Option<u64>,
    /// Optional aggregate cost budget in micro-units.
    pub global_cost_budget_micros: Option<u64>,
    /// Whether the run uses deterministic or real execution.
    pub mode: BenchmarkMode,
}

/// Execution environment requested for a benchmark run.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkMode {
    /// Use a deterministic or fixture-backed executor.
    Deterministic,
    /// Use an externally supplied real executor.
    Real,
}

/// Outcome category returned by one benchmark attempt.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttemptOutcome {
    /// Attempt satisfied the challenge's success evaluator.
    Passed,
    /// Attempt completed but did not satisfy the success evaluator.
    Failed,
    /// A prerequisite prevented the attempt from proceeding.
    PrerequisiteFailed,
    /// Policy or suite configuration skipped the attempt.
    Skipped,
    /// Required executor capability was unavailable.
    Unavailable,
    /// Outcome could not be determined.
    Unknown,
    /// Attempt was blocked before execution.
    Blocked,
    /// Attempt result violated the benchmark contract.
    Invalid,
}

/// Normalized failure classification for a benchmark attempt.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// Agent reasoning produced an incorrect result.
    Reasoning,
    /// Agent selected an inappropriate tool.
    WrongTool,
    /// Tool arguments did not satisfy the tool contract.
    InvalidArguments,
    /// Permission policy denied the operation.
    Permission,
    /// Agent violated an approval requirement.
    ApprovalViolation,
    /// Agent claimed a capability that was not available.
    HallucinatedCapability,
    /// Attempt exceeded its time limit.
    Timeout,
    /// Attempt or run exceeded its resource budget.
    BudgetExceeded,
    /// Model provider returned an error.
    Provider,
    /// Infrastructure or executor failed.
    Infrastructure,
    /// Recovery after a failure did not succeed.
    Recovery,
    /// Success evaluator failed or rejected invalidly.
    Evaluator,
    /// Attempt violated a security requirement.
    Security,
}

/// Bounded metrics and outcome for one attempt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttemptResult {
    /// Final outcome category.
    pub outcome: AttemptOutcome,
    /// Optional normalized failure classification.
    pub failure_class: Option<FailureClass>,
    /// Whether the attempt triggered a security violation.
    pub security_violation: bool,
    /// End-to-end latency in milliseconds.
    pub latency_ms: u64,
    /// Number of execution steps used.
    pub steps: u32,
    /// Prompt token count.
    pub prompt_tokens: u32,
    /// Completion token count.
    pub completion_tokens: u32,
    /// Attempt cost in micro-units.
    pub cost_micros: u64,
    /// Digest of the bounded output, not raw output text.
    pub output_digest: String,
    /// Digest of the tool trace, not raw tool contents.
    pub tool_trace_digest: String,
}

/// Aggregated counts, percentiles, and failure-class totals for attempts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Metrics {
    /// Total number of attempts.
    pub attempts: u32,
    /// Attempts with a completed pass or failure outcome.
    pub completed: u32,
    /// Attempts that passed.
    pub passed: u32,
    /// Pass rate in thousandths, from 0 to 1000.
    pub pass_rate_millis: u32,
    /// Number of attempts classified as timeouts.
    pub timeout_count: u32,
    /// Number of security failures.
    pub security_failures: u32,
    /// 50th-percentile completed-attempt latency in milliseconds.
    pub p50_latency_ms: Option<u64>,
    /// 95th-percentile completed-attempt latency in milliseconds.
    pub p95_latency_ms: Option<u64>,
    /// 99th-percentile completed-attempt latency in milliseconds.
    pub p99_latency_ms: Option<u64>,
    /// 50th-percentile completed-attempt cost in micro-units.
    pub p50_cost_micros: Option<u64>,
    /// 95th-percentile completed-attempt cost in micro-units.
    pub p95_cost_micros: Option<u64>,
    /// 99th-percentile completed-attempt cost in micro-units.
    pub p99_cost_micros: Option<u64>,
    /// Count by normalized failure class.
    pub failure_classes: BTreeMap<FailureClass, u32>,
}

/// Compatible prior metrics used as a regression comparison baseline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Baseline {
    /// Stable baseline identifier.
    pub id: String,
    /// Suite version associated with the baseline.
    pub suite_version: String,
    /// Challenge represented by the baseline.
    pub challenge_id: String,
    /// Digest of the model profile used to produce the baseline.
    pub model_profile_hash: String,
    /// Digest of the agent profile used to produce the baseline.
    pub agent_profile_hash: String,
    /// Aggregated metrics from the baseline run.
    pub metrics: Metrics,
    /// Source commit used to generate the baseline.
    pub source_commit: String,
    /// Baseline revision.
    pub revision: u64,
}

/// Result of comparing current benchmark metrics with a baseline and thresholds.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonVerdict {
    /// Current metrics improved against the compatible baseline.
    Improved,
    /// Metrics remain within accepted bounds.
    Stable,
    /// Current results violate a threshold or regress from baseline.
    Regressed,
    /// Completed samples are insufficient for comparison.
    Inconclusive,
    /// No compatible prior baseline exists.
    New,
    /// Run could not proceed.
    Blocked,
    /// Configuration or data failed validation.
    Invalid,
}

/// Per-combination comparison result and security hard-failure flag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkComparison {
    /// Verdict derived from metrics and thresholds.
    pub verdict: ComparisonVerdict,
    /// Whether the result exceeds a configured security-failure threshold.
    pub security_hard_failure: bool,
    /// Bounded explanation for the comparison verdict.
    pub reason: String,
}

/// Redacted outcome report for one benchmark matrix run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkReport {
    /// Contract identifier used to produce the report.
    pub contract_id: String,
    /// Digest of the benchmark contract identifier.
    pub contract_hash: String,
    /// Stable run identifier.
    pub run_id: String,
    /// Source commit identifier used for the run.
    pub source_commit: String,
    /// Suite identifier.
    pub suite_id: String,
    /// Suite version.
    pub suite_version: String,
    /// Model profile identifiers included in the run.
    pub model_profile_ids: Vec<String>,
    /// Agent profile identifiers included in the run.
    pub agent_profile_ids: Vec<String>,
    /// Aggregated metrics keyed by challenge/model/agent combination.
    pub metrics: BTreeMap<String, Metrics>,
    /// Threshold and baseline comparison keyed by combination.
    pub comparisons: BTreeMap<String, BenchmarkComparison>,
    /// Must be `redacted` before the report is exposed.
    pub redaction_status: String,
}

/// Invalid benchmark data, exceeded limits, sensitive output, or duplicate identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BenchmarkValidationError {
    /// A field does not satisfy the contract.
    InvalidField(String),
    /// Input contract version is not supported.
    UnsupportedVersion(u32),
    /// A collection or run exceeds a configured limit.
    Limit(String),
    /// A sensitive field was not safely redacted.
    SensitiveField(String),
    /// An identifier was duplicated.
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
    /// Validates challenge identifiers, references, bounds, and runtime limits.
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
    /// Validates model identity fields and the canonical profile digest shape.
    pub fn validate(&self) -> Result<(), BenchmarkValidationError> {
        bounded("model.id", &self.id)?;
        bounded("model.provider", &self.provider)?;
        bounded("model.model", &self.model)?;
        valid_hash("model.content_hash", &self.content_hash)
    }
}
impl AgentProfile {
    /// Validates version references and the canonical profile digest shape.
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
    /// Validates suite contents, uniqueness, profile bounds, and pass-rate threshold.
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

    /// Computes the canonical SHA-256 digest of the serialized suite.
    pub fn canonical_hash(&self) -> Result<String, serde_json::Error> {
        let bytes = serde_json::to_vec(self)?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
}

impl BenchmarkPolicy {
    /// Checks that attempt count and requested parallelism are within bounds.
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

/// Aggregates pass rate, security failures, failure classes, latency, and cost percentiles.
///
/// Only passed and failed outcomes count as completed attempts; unavailable
/// and unknown outcomes are excluded from the pass-rate denominator.
///
/// # Example
///
/// ```
/// use evohime_core::agent_benchmark_matrix::{aggregate_attempts, AttemptResult};
///
/// let metrics = aggregate_attempts(&[AttemptResult {
///     outcome: evohime_core::agent_benchmark_matrix::AttemptOutcome::Passed,
///     failure_class: None,
///     security_violation: false,
///     latency_ms: 12,
///     steps: 1,
///     prompt_tokens: 10,
///     completion_tokens: 5,
///     cost_micros: 2,
///     output_digest: "a".repeat(64),
///     tool_trace_digest: "b".repeat(64),
/// }]);
/// assert_eq!(metrics.pass_rate_millis, 1000);
/// assert_eq!(metrics.completed, 1);
/// ```
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

/// Compares current metrics against security and performance thresholds and an optional baseline.
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

/// Returns a serialized report only when its redaction marker confirms safe projection.
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

    #[tokio::test]
    async fn local_executor_rejects_multiple_model_profiles_before_inference() {
        let mut benchmark = suite();
        benchmark.challenges[0].synthetic_only = false;
        benchmark.challenges[0].timeout_ms = 300_000;
        benchmark.challenges[0].success_evaluator = format!("sha256:{}", "0".repeat(64));
        let mut second_model = benchmark.model_profiles[0].clone();
        second_model.id = "m2".into();
        second_model.content_hash = "c".repeat(64);
        benchmark.model_profiles.push(second_model);
        let error = run_local_model_matrix(
            &benchmark,
            &BenchmarkPolicy {
                attempts: 1,
                max_parallelism: 1,
                seed: 0,
                global_token_budget: None,
                global_cost_budget_micros: None,
                mode: BenchmarkMode::Real,
            },
            "run",
            "commit",
            1,
            "managed-local-model",
            &BTreeMap::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            BenchmarkValidationError::InvalidField("local_model_profile_count".into())
        );
    }
}
