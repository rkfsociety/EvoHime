//! Queen-owned safety ceiling for dynamic sub-agent fan-out.
//!
//! [`HiveSettings`] is deliberately *not* a plan. The queen decides how many
//! sub-agents a task needs; these values only bound that decision. Exceeding a
//! ceiling is rejected, but a default ceiling never means "spawn this many".
//!
//! Semantics of zero:
//! - `max_subagents_per_worker = 0` means "no per-worker ceiling": the worker
//!   ceiling is bounded by `max_total_subagents_per_workflow`;
//! - `max_parallel_subagents = 0` means "no separate parallel ceiling": the
//!   parallel limit is bounded by the effective worker ceiling;
//! - `max_total_subagents_per_workflow = 0` is invalid. This value is the last
//!   guard and must always stay positive.
//!
//! The settings are persisted through the existing metadata-only
//! `schema_agent_configurations` store under the `hive` scope, so this module
//! does not add a second configuration owner or a new database table.

use serde::{Deserialize, Serialize};

use crate::child_contracts::MAX_CHILD_DEPTH;

/// Version of the persisted [`HiveSettings`] contract.
pub const HIVE_SETTINGS_SCHEMA_VERSION: u32 = 1;
/// Default per-worker sub-agent ceiling.
pub const DEFAULT_MAX_SUBAGENTS_PER_WORKER: u32 = 16;
/// Default number of sub-agents a worker may keep in flight.
pub const DEFAULT_MAX_PARALLEL_SUBAGENTS: u32 = 4;
/// Default workflow-wide sub-agent guard.
pub const DEFAULT_MAX_TOTAL_SUBAGENTS_PER_WORKFLOW: u32 = 64;
/// Hard upper bound accepted for any persisted sub-agent ceiling.
///
/// The value is an implementation guard against corrupt or hostile settings,
/// not a product default. Every hop-2 sub-agent is a bounded read-only task,
/// and the workflow-wide guard is normally much smaller than this limit.
pub const MAX_SUBAGENT_HARD_LIMIT: u32 = 256;
/// Hard upper bound accepted for the parallel sub-agent ceiling.
pub const MAX_PARALLEL_HARD_LIMIT: u32 = 32;
/// Storage scope used by [`crate::EventJournal::hive_settings`].
pub const HIVE_SETTINGS_SCOPE: &str = "hive";
/// Default interval between EvaPulse background cycles, in seconds.
pub const DEFAULT_PULSE_INTERVAL_SECS: u64 = 1_800;
/// Lower bound accepted for the EvaPulse interval.
pub const MIN_PULSE_INTERVAL_SECS: u64 = 30;
/// Upper bound accepted for the EvaPulse interval.
pub const MAX_PULSE_INTERVAL_SECS: u64 = 86_400;
/// Default daily spend ceiling shown and enforced by the hive, in USD.
pub const DEFAULT_DAILY_BUDGET_USD: f64 = 0.50;
/// Hard upper bound for the persisted daily budget, in USD.
pub const MAX_DAILY_BUDGET_USD: f64 = 10_000.0;

/// Bounded inputs the queen uses to pick a delegation ceiling.
///
/// The queen derives these values from the task itself (goal text, number of
/// independently inspectable areas, context items, grants). They are hints for
/// a deterministic ceiling, not a list of planned sub-agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DelegationComplexity {
    /// Character count of the queen's goal/purpose text.
    pub goal_chars: usize,
    /// Number of context items the worker may pass to a sub-agent.
    pub context_items: usize,
    /// Number of read-only grants available for the sub-agent.
    pub grant_count: usize,
    /// Optional count of independent areas the queen identified.
    pub independent_areas: u32,
}

/// Safety ceiling for queen-driven sub-agent fan-out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveSettings {
    /// Version of the persisted settings contract.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Per-worker ceiling. `0` means "no separate per-worker ceiling"; the
    /// workflow-wide guard still applies.
    pub max_subagents_per_worker: u32,
    /// Maximum sub-agents one worker keeps in flight. `0` means "no separate
    /// parallel ceiling"; the effective worker ceiling still applies.
    pub max_parallel_subagents: u32,
    /// Hard workflow-wide guard. Must be positive.
    pub max_total_subagents_per_workflow: u32,
    /// Interval between EvaPulse background cycles, in seconds.
    #[serde(default = "default_pulse_interval_secs")]
    pub pulse_interval_secs: u64,
    /// Daily spend ceiling for background work, in USD.
    ///
    /// `0` means no budget at all: a background cycle must not start a task.
    #[serde(default = "default_daily_budget_usd")]
    pub daily_budget_usd: f64,
    /// Optional isolated Git checkouts watched by Queen / EvaPulse.
    #[serde(default)]
    pub watched_repositories: Vec<String>,
    /// Optional isolated provider profile for Hive (e.g. `"literouter"`, `"rout"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hive_provider_profile: Option<String>,
    /// Optional model override used specifically by Hive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hive_model: Option<String>,
    /// Optional isolated fallback provider profile for Hive (e.g. `"rout"`, `"literouter"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hive_fallback_provider_profile: Option<String>,
    /// Optional fallback model override used specifically by Hive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hive_fallback_model: Option<String>,
    /// Whether EvaPulse may proactively pick uncompleted roadmap items when idle.
    #[serde(default)]
    pub proactive_evolution_enabled: bool,
}

fn default_schema_version() -> u32 {
    HIVE_SETTINGS_SCHEMA_VERSION
}

fn default_pulse_interval_secs() -> u64 {
    DEFAULT_PULSE_INTERVAL_SECS
}

fn default_daily_budget_usd() -> f64 {
    DEFAULT_DAILY_BUDGET_USD
}

impl Default for HiveSettings {
    fn default() -> Self {
        Self {
            schema_version: HIVE_SETTINGS_SCHEMA_VERSION,
            max_subagents_per_worker: DEFAULT_MAX_SUBAGENTS_PER_WORKER,
            max_parallel_subagents: DEFAULT_MAX_PARALLEL_SUBAGENTS,
            max_total_subagents_per_workflow: DEFAULT_MAX_TOTAL_SUBAGENTS_PER_WORKFLOW,
            pulse_interval_secs: DEFAULT_PULSE_INTERVAL_SECS,
            daily_budget_usd: DEFAULT_DAILY_BUDGET_USD,
            watched_repositories: Vec::new(),
            hive_provider_profile: None,
            hive_model: None,
            hive_fallback_provider_profile: None,
            hive_fallback_model: None,
            proactive_evolution_enabled: false,
        }
    }
}

impl HiveSettings {
    /// Validates persisted or freshly submitted settings.
    pub fn validate(&self) -> Result<(), HiveSettingsError> {
        if self.schema_version != HIVE_SETTINGS_SCHEMA_VERSION {
            return Err(HiveSettingsError::UnsupportedVersion(self.schema_version));
        }
        if self.max_subagents_per_worker > MAX_SUBAGENT_HARD_LIMIT {
            return Err(HiveSettingsError::PerWorkerAboveHardLimit {
                value: self.max_subagents_per_worker,
                limit: MAX_SUBAGENT_HARD_LIMIT,
            });
        }
        if self.max_parallel_subagents > MAX_PARALLEL_HARD_LIMIT {
            return Err(HiveSettingsError::ParallelAboveHardLimit {
                value: self.max_parallel_subagents,
                limit: MAX_PARALLEL_HARD_LIMIT,
            });
        }
        if self.max_total_subagents_per_workflow == 0 {
            return Err(HiveSettingsError::MissingWorkflowGuard);
        }
        if self.max_total_subagents_per_workflow > MAX_SUBAGENT_HARD_LIMIT {
            return Err(HiveSettingsError::TotalAboveHardLimit {
                value: self.max_total_subagents_per_workflow,
                limit: MAX_SUBAGENT_HARD_LIMIT,
            });
        }
        if !(MIN_PULSE_INTERVAL_SECS..=MAX_PULSE_INTERVAL_SECS).contains(&self.pulse_interval_secs)
        {
            return Err(HiveSettingsError::InvalidPulseInterval {
                value: self.pulse_interval_secs,
                min: MIN_PULSE_INTERVAL_SECS,
                max: MAX_PULSE_INTERVAL_SECS,
            });
        }
        if !self.daily_budget_usd.is_finite()
            || self.daily_budget_usd < 0.0
            || self.daily_budget_usd > MAX_DAILY_BUDGET_USD
        {
            return Err(HiveSettingsError::InvalidDailyBudget {
                value: self.daily_budget_usd,
                max: MAX_DAILY_BUDGET_USD,
            });
        }
        for path in &self.watched_repositories {
            validate_eva_workspace_path(path)?;
        }
        Ok(())
    }

    /// Returns the primary watched repository path, if any is configured.
    pub fn primary_workspace_path(&self) -> Option<&str> {
        self.watched_repositories
            .first()
            .map(String::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
    }

    /// Returns the effective per-worker ceiling.
    ///
    /// `0` means "no separate per-worker ceiling", so the workflow guard is
    /// used instead. The result is always positive.
    pub fn effective_worker_ceiling(&self) -> u32 {
        if self.max_subagents_per_worker == 0 {
            self.max_total_subagents_per_workflow
        } else {
            self.max_subagents_per_worker
        }
        .min(self.max_total_subagents_per_workflow)
    }

    /// Returns the effective number of sub-agents kept in flight.
    ///
    /// `0` means "no separate parallel ceiling"; the effective worker ceiling
    /// is used instead. The result is always at least one so sequential work
    /// remains possible even for a one-slot ceiling.
    pub fn effective_parallel_limit(&self) -> u32 {
        let parallel = if self.max_parallel_subagents == 0 {
            self.effective_worker_ceiling()
        } else {
            self.max_parallel_subagents
        };
        parallel.max(1).min(self.effective_worker_ceiling())
    }

    /// Rejects a queen-authored ceiling that exceeds the safety guard.
    pub fn ensure_worker_ceiling(&self, requested: u32) -> Result<u32, HiveSettingsError> {
        if requested == 0 || requested > self.effective_worker_ceiling() {
            return Err(HiveSettingsError::WorkerCeilingAboveLimit {
                requested,
                limit: self.effective_worker_ceiling(),
            });
        }
        Ok(requested)
    }

    /// Rejects an extension that would exceed the workflow-wide guard.
    pub fn ensure_workflow_capacity(
        &self,
        requested_ceiling: u32,
        already_spawned: u32,
    ) -> Result<u32, HiveSettingsError> {
        self.ensure_worker_ceiling(requested_ceiling)?;
        if already_spawned > self.max_total_subagents_per_workflow {
            return Err(HiveSettingsError::WorkflowGuarded {
                spawned: already_spawned,
                limit: self.max_total_subagents_per_workflow,
            });
        }
        let remaining = self
            .max_total_subagents_per_workflow
            .saturating_sub(already_spawned);
        if requested_ceiling > remaining {
            return Err(HiveSettingsError::WorkflowCeilingAboveRemaining {
                requested: requested_ceiling,
                remaining,
            });
        }
        Ok(requested_ceiling)
    }

    /// Picks a deterministic ceiling for one task.
    ///
    /// This is the queen's default heuristic when no explicit decision is
    /// available. It only shapes a *ceiling*: a worker is free to stop earlier
    /// and never has to use the returned number. The value is derived from
    /// bounded task hints:
    ///
    /// `1 + independent_areas/2 + context_items/8 + goal_chars/400`, clamped
    /// to `[1, effective_worker_ceiling()]`. A task with no independent areas
    /// and a short goal therefore gets a ceiling of one sub-agent, not the
    /// configured default.
    pub fn choose_delegation_ceiling(&self, complexity: DelegationComplexity) -> u32 {
        let independent = complexity.independent_areas.min(MAX_SUBAGENT_HARD_LIMIT);
        let derived = 1_u32
            .saturating_add(independent / 2)
            .saturating_add((complexity.context_items as u32) / 8)
            .saturating_add((complexity.goal_chars as u32) / 400);
        derived.clamp(1, self.effective_worker_ceiling())
    }

    /// Serializes settings after validating them.
    pub fn to_json(&self) -> Result<Vec<u8>, HiveSettingsError> {
        self.validate()?;
        serde_json::to_vec(self)
            .map_err(|error| HiveSettingsError::Serialization(error.to_string()))
    }

    /// Parses and validates settings loaded from durable storage, migrating
    /// legacy `eva_workspace_path` into `watched_repositories` when present.
    pub fn from_json(bytes: &[u8]) -> Result<Self, HiveSettingsError> {
        let mut value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|error| HiveSettingsError::Serialization(error.to_string()))?;
        if let Some(map) = value.as_object_mut() {
            if let Some(legacy_path) = map.remove("eva_workspace_path") {
                if let Some(path) = legacy_path
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    let needs_migration = match map.get("watched_repositories") {
                        None => true,
                        Some(serde_json::Value::Array(items)) => items.is_empty(),
                        Some(serde_json::Value::Null) => true,
                        _ => false,
                    };
                    if needs_migration {
                        map.insert(
                            "watched_repositories".to_owned(),
                            serde_json::Value::Array(vec![serde_json::Value::String(
                                path.to_owned(),
                            )]),
                        );
                    }
                }
            }
        }
        let settings: Self = serde_json::from_value(value)
            .map_err(|error| HiveSettingsError::Serialization(error.to_string()))?;
        settings.validate()?;
        Ok(settings)
    }

    /// Schema document stored next to the snapshot for future migrations.
    pub fn schema_json() -> Vec<u8> {
        serde_json::json!({
            "schema_version": HIVE_SETTINGS_SCHEMA_VERSION,
            "fields": [
                {"name": "max_subagents_per_worker", "type": "u32", "zero": "unbounded_up_to_total"},
                {"name": "max_parallel_subagents", "type": "u32", "zero": "unbounded_up_to_worker_ceiling"},
                {"name": "max_total_subagents_per_workflow", "type": "u32", "zero": "invalid"},
                {"name": "pulse_interval_secs", "type": "u64", "zero": "invalid"},
                {"name": "daily_budget_usd", "type": "f64", "zero": "no_background_task"},
                {"name": "watched_repositories", "type": "list_of_existing_git_directories", "default": []},
                {"name": "hive_provider_profile", "type": "optional_string", "default": null},
                {"name": "hive_model", "type": "optional_string", "default": null},
                {"name": "hive_fallback_provider_profile", "type": "optional_string", "default": null},
                {"name": "hive_fallback_model", "type": "optional_string", "default": null},
                {"name": "proactive_evolution_enabled", "type": "bool", "default": false}
            ],
            "note": "ceilings are safety guards, not a spawn plan",
            "max_child_depth": MAX_CHILD_DEPTH
        })
        .to_string()
        .into_bytes()
    }
}

/// Validates that an Eva self-development workspace path is absolute, exists,
/// and points to a Git repository root.
pub fn validate_eva_workspace_path(path: &str) -> Result<(), HiveSettingsError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(HiveSettingsError::InvalidEvaWorkspace(
            "path must not be empty".into(),
        ));
    }
    if !std::path::Path::new(trimmed).is_absolute() {
        return Err(HiveSettingsError::InvalidEvaWorkspace(
            "path must be absolute".into(),
        ));
    }
    let workspace = evohime_tool_runtime::self_development::EvaWorkspace::open(trimmed)
        .map_err(|error| HiveSettingsError::InvalidEvaWorkspace(error.to_string()))?;
    if !workspace.root().is_absolute() {
        return Err(HiveSettingsError::InvalidEvaWorkspace(
            "path must resolve to an absolute directory".into(),
        ));
    }
    Ok(())
}

/// Invalid hive safety settings.
#[derive(Debug, Clone, PartialEq)]
pub enum HiveSettingsError {
    /// The configured isolated self-development checkout is invalid.
    InvalidEvaWorkspace(String),
    /// The persisted contract version is not supported.
    UnsupportedVersion(u32),
    /// The per-worker ceiling exceeds the hard implementation limit.
    PerWorkerAboveHardLimit {
        /// Submitted value.
        value: u32,
        /// Hard implementation limit.
        limit: u32,
    },
    /// The parallel ceiling exceeds the hard implementation limit.
    ParallelAboveHardLimit {
        /// Submitted value.
        value: u32,
        /// Hard implementation limit.
        limit: u32,
    },
    /// The workflow-wide guard was set to zero.
    MissingWorkflowGuard,
    /// The workflow-wide guard exceeds the hard implementation limit.
    TotalAboveHardLimit {
        /// Submitted value.
        value: u32,
        /// Hard implementation limit.
        limit: u32,
    },
    /// A queen-authored ceiling exceeds the effective safety ceiling.
    WorkerCeilingAboveLimit {
        /// Requested ceiling.
        requested: u32,
        /// Effective safety ceiling.
        limit: u32,
    },
    /// The workflow-wide guard was already reached.
    WorkflowGuarded {
        /// Sub-agents already spawned in the workflow.
        spawned: u32,
        /// Workflow-wide guard.
        limit: u32,
    },
    /// The requested ceiling exceeds the remaining workflow capacity.
    WorkflowCeilingAboveRemaining {
        /// Requested ceiling.
        requested: u32,
        /// Remaining workflow capacity.
        remaining: u32,
    },
    /// The pulse interval is outside the accepted range.
    InvalidPulseInterval {
        /// Submitted interval in seconds.
        value: u64,
        /// Minimum accepted interval.
        min: u64,
        /// Maximum accepted interval.
        max: u64,
    },
    /// The daily budget is negative, non-finite, or above the hard bound.
    InvalidDailyBudget {
        /// Submitted budget in USD.
        value: f64,
        /// Hard upper bound.
        max: f64,
    },
    /// The settings could not be serialized or parsed.
    Serialization(String),
}

impl std::fmt::Display for HiveSettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEvaWorkspace(message) => write!(f, "invalid eva workspace: {message}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported hive settings version: {version}")
            }
            Self::PerWorkerAboveHardLimit { value, limit } => {
                write!(f, "max_subagents_per_worker {value} exceeds {limit}")
            }
            Self::ParallelAboveHardLimit { value, limit } => {
                write!(f, "max_parallel_subagents {value} exceeds {limit}")
            }
            Self::MissingWorkflowGuard => {
                write!(f, "max_total_subagents_per_workflow must be positive")
            }
            Self::TotalAboveHardLimit { value, limit } => {
                write!(
                    f,
                    "max_total_subagents_per_workflow {value} exceeds {limit}"
                )
            }
            Self::WorkerCeilingAboveLimit { requested, limit } => {
                write!(
                    f,
                    "delegation ceiling {requested} exceeds hive limit {limit}"
                )
            }
            Self::WorkflowGuarded { spawned, limit } => {
                write!(
                    f,
                    "workflow already spawned {spawned} sub-agents, guard is {limit}"
                )
            }
            Self::WorkflowCeilingAboveRemaining {
                requested,
                remaining,
            } => {
                write!(
                    f,
                    "delegation ceiling {requested} exceeds remaining workflow capacity {remaining}"
                )
            }
            Self::InvalidPulseInterval { value, min, max } => {
                write!(f, "pulse_interval_secs {value} is outside {min}..={max}")
            }
            Self::InvalidDailyBudget { value, max } => {
                write!(f, "daily_budget_usd {value} is outside 0..={max}")
            }
            Self::Serialization(message) => {
                write!(f, "hive settings serialization failed: {message}")
            }
        }
    }
}

impl std::error::Error for HiveSettingsError {}

impl From<HiveSettingsError> for crate::child_contracts::ContractError {
    fn from(error: HiveSettingsError) -> Self {
        use crate::child_contracts::ContractError;
        match error {
            HiveSettingsError::WorkerCeilingAboveLimit { requested, limit } => {
                ContractError::DelegationCeilingAboveHiveLimit { requested, limit }
            }
            HiveSettingsError::WorkflowGuarded { .. }
            | HiveSettingsError::WorkflowCeilingAboveRemaining { .. } => {
                ContractError::DelegationWorkflowGuardExceeded
            }
            other => ContractError::HiveSettingsRejected(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_ceilings_and_validate() {
        let settings = HiveSettings::default();
        assert_eq!(settings.max_subagents_per_worker, 16);
        assert_eq!(settings.max_parallel_subagents, 4);
        assert_eq!(settings.max_total_subagents_per_workflow, 64);
        assert!(settings.validate().is_ok());
        assert_eq!(settings.effective_worker_ceiling(), 16);
        assert_eq!(settings.effective_parallel_limit(), 4);
    }

    #[test]
    fn zero_semantics_are_explicit() {
        let mut settings = HiveSettings {
            max_subagents_per_worker: 0,
            max_parallel_subagents: 0,
            ..HiveSettings::default()
        };
        assert!(settings.validate().is_ok());
        assert_eq!(settings.effective_worker_ceiling(), 64);
        assert_eq!(settings.effective_parallel_limit(), 64);

        settings.max_total_subagents_per_workflow = 0;
        assert_eq!(
            settings.validate(),
            Err(HiveSettingsError::MissingWorkflowGuard)
        );
    }

    #[test]
    fn hard_limits_reject_hostile_values() {
        let settings = HiveSettings {
            max_subagents_per_worker: MAX_SUBAGENT_HARD_LIMIT + 1,
            ..HiveSettings::default()
        };
        assert!(settings.validate().is_err());
        let settings = HiveSettings {
            max_parallel_subagents: MAX_PARALLEL_HARD_LIMIT + 1,
            ..HiveSettings::default()
        };
        assert!(settings.validate().is_err());
        let settings = HiveSettings {
            max_total_subagents_per_workflow: MAX_SUBAGENT_HARD_LIMIT + 1,
            ..HiveSettings::default()
        };
        assert!(settings.validate().is_err());
    }

    #[test]
    fn heuristic_grows_with_task_complexity_but_stays_a_ceiling() {
        let settings = HiveSettings {
            max_subagents_per_worker: 8,
            ..HiveSettings::default()
        };
        assert_eq!(
            settings.choose_delegation_ceiling(DelegationComplexity::default()),
            1
        );
        assert_eq!(
            settings.choose_delegation_ceiling(DelegationComplexity {
                goal_chars: 0,
                context_items: 0,
                grant_count: 0,
                independent_areas: 4,
            }),
            3
        );
        assert!(
            settings.choose_delegation_ceiling(DelegationComplexity {
                goal_chars: 100_000,
                context_items: 1_000,
                grant_count: 4,
                independent_areas: 100,
            }) <= 8
        );
    }

    #[test]
    fn pulse_budget_and_interval_validate_and_keep_compatible_defaults() {
        let settings = HiveSettings::default();
        assert_eq!(settings.pulse_interval_secs, DEFAULT_PULSE_INTERVAL_SECS);
        assert_eq!(settings.daily_budget_usd, DEFAULT_DAILY_BUDGET_USD);

        let legacy = br#"{"schema_version":1,"max_subagents_per_worker":1,"max_parallel_subagents":1,"max_total_subagents_per_workflow":2}"#;
        let loaded = HiveSettings::from_json(legacy).expect("legacy settings load");
        assert_eq!(loaded.pulse_interval_secs, DEFAULT_PULSE_INTERVAL_SECS);
        assert_eq!(loaded.daily_budget_usd, DEFAULT_DAILY_BUDGET_USD);

        let zero_budget = HiveSettings {
            daily_budget_usd: 0.0,
            ..HiveSettings::default()
        };
        assert!(zero_budget.validate().is_ok());

        let too_fast = HiveSettings {
            pulse_interval_secs: MIN_PULSE_INTERVAL_SECS - 1,
            ..HiveSettings::default()
        };
        assert!(matches!(
            too_fast.validate(),
            Err(HiveSettingsError::InvalidPulseInterval { .. })
        ));

        let negative_budget = HiveSettings {
            daily_budget_usd: -0.1,
            ..HiveSettings::default()
        };
        assert!(matches!(
            negative_budget.validate(),
            Err(HiveSettingsError::InvalidDailyBudget { .. })
        ));
    }

    #[test]
    fn json_roundtrip_revalidates_on_load() {
        let settings = HiveSettings {
            max_subagents_per_worker: 3,
            max_parallel_subagents: 2,
            max_total_subagents_per_workflow: 9,
            schema_version: HIVE_SETTINGS_SCHEMA_VERSION,
            ..HiveSettings::default()
        };
        let json = settings.to_json().expect("serialize");
        assert_eq!(HiveSettings::from_json(&json).expect("load"), settings);

        let invalid = br#"{"schema_version":1,"max_subagents_per_worker":1,"max_parallel_subagents":1,"max_total_subagents_per_workflow":0}"#;
        assert!(matches!(
            HiveSettings::from_json(invalid),
            Err(HiveSettingsError::MissingWorkflowGuard)
        ));
    }

    #[test]
    fn eva_workspace_path_validation() {
        let missing = HiveSettings {
            watched_repositories: vec!["C:\\definitely\\missing\\eva-workspace".into()],
            ..HiveSettings::default()
        };
        assert!(matches!(
            missing.validate(),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));
        let plain = tempfile::tempdir().expect("directory");
        let not_git = HiveSettings {
            watched_repositories: vec![plain.path().to_string_lossy().into_owned()],
            ..HiveSettings::default()
        };
        assert!(matches!(
            not_git.validate(),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));
        assert!(HiveSettings::default().validate().is_ok());
    }

    #[test]
    fn migrates_legacy_eva_workspace_path_to_watched_repositories() {
        let git_repo = tempfile::tempdir().expect("git directory");
        let status = std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(git_repo.path())
            .status()
            .expect("run git init");
        assert!(status.success());

        let repo_path = git_repo.path().to_string_lossy().into_owned();
        let legacy_json = serde_json::json!({
            "schema_version": HIVE_SETTINGS_SCHEMA_VERSION,
            "max_subagents_per_worker": 16,
            "max_parallel_subagents": 4,
            "max_total_subagents_per_workflow": 64,
            "eva_workspace_path": repo_path,
            "proactive_evolution_enabled": true
        })
        .to_string()
        .into_bytes();

        let loaded = HiveSettings::from_json(&legacy_json).expect("legacy settings migrate");
        assert_eq!(loaded.watched_repositories, vec![repo_path]);
        assert!(loaded.proactive_evolution_enabled);
    }

    #[test]
    fn core_validates_eva_workspace_path() {
        assert!(matches!(
            validate_eva_workspace_path(""),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));
        assert!(matches!(
            validate_eva_workspace_path("."),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));
        assert!(matches!(
            validate_eva_workspace_path("relative/eva-workspace"),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));
        assert!(matches!(
            validate_eva_workspace_path("C:\\definitely\\missing\\eva-workspace"),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));

        let plain = tempfile::tempdir().expect("directory");
        assert!(matches!(
            validate_eva_workspace_path(&plain.path().to_string_lossy()),
            Err(HiveSettingsError::InvalidEvaWorkspace(_))
        ));

        let git_repo = tempfile::tempdir().expect("git directory");
        let status = std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(git_repo.path())
            .status()
            .expect("run git init");
        assert!(status.success());
        assert!(validate_eva_workspace_path(&git_repo.path().to_string_lossy()).is_ok());
    }

    #[test]
    fn workflow_capacity_counts_spawned_subagents() {
        let settings = HiveSettings {
            max_subagents_per_worker: 8,
            max_parallel_subagents: 4,
            max_total_subagents_per_workflow: 10,
            schema_version: HIVE_SETTINGS_SCHEMA_VERSION,
            ..HiveSettings::default()
        };
        assert_eq!(settings.ensure_workflow_capacity(8, 0), Ok(8));
        assert_eq!(settings.ensure_workflow_capacity(6, 4), Ok(6));
        assert_eq!(
            settings.ensure_workflow_capacity(8, 4),
            Err(HiveSettingsError::WorkflowCeilingAboveRemaining {
                requested: 8,
                remaining: 6
            })
        );
        assert_eq!(
            settings.ensure_workflow_capacity(9, 0),
            Err(HiveSettingsError::WorkerCeilingAboveLimit {
                requested: 9,
                limit: 8
            })
        );
    }
}
