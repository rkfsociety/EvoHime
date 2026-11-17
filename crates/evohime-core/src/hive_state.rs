//! Read-only aggregated snapshot of the hive for the operations panel.
//!
//! The snapshot is a *projection*, never an authority: it is derived from the
//! bounded child projections Core already journals and from the validated
//! [`crate::hive_settings::HiveSettings`]. It never starts, stops, or retargets
//! an agent, and it never exposes prompts, reports, or tool output.
//!
//! Safe defaults are part of the contract. An empty registry or a journal with
//! no child events produces zero/empty metrics rather than an error, so the UI
//! can render before the first task starts.

use serde::{Deserialize, Serialize};

use crate::child_workflow::{ChildProjection, CoordinatorState};
use crate::hive_settings::HiveSettings;

/// Version of the serialized [`HiveStateSnapshot`] contract.
pub const HIVE_STATE_SCHEMA_VERSION: u32 = 1;
/// Name shown for the single queen in the local hive.
pub const DEFAULT_QUEEN_NAME: &str = "EvoHime";
/// Maximum number of departments rendered at once.
pub const MAX_HIVE_DEPARTMENTS: usize = 16;
/// Maximum number of active agents rendered at once.
pub const MAX_HIVE_ACTIVE_AGENTS: usize = 64;

/// Queen status line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveQueenState {
    /// `active` while at least one child task is running, `idle` otherwise.
    pub status: String,
    /// Display name of the queen.
    pub name: String,
}

/// Aggregate counters shown above the department grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveMetrics {
    /// Number of active agents in the snapshot.
    pub total_agents_online: u16,
    /// Number of child tasks currently in flight.
    pub active_tasks: u16,
    /// Mean wall-clock time of a finished child task.
    ///
    /// Child projections do not carry start/end timestamps yet, so the value is
    /// currently always zero rather than an estimate.
    pub average_task_time_sec: u64,
    /// Share of the queen-authored ceilings already used by spawned sub-agents.
    pub efficiency_percent: u8,
}

/// One role department with its current load and ceiling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveDepartment {
    /// Role identifier aggregated across child projections.
    pub role: String,
    /// Agents of this role currently active.
    pub active_count: u16,
    /// Per-role ceiling derived from [`HiveSettings`].
    pub max_capacity: u16,
}

/// One active agent rendered in the inspector list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveActiveAgent {
    /// Stable child-task identifier.
    pub id: String,
    /// Delegated role.
    pub role: String,
    /// Bounded description of the current lifecycle step.
    pub current_action: String,
    /// Nesting depth: 1 for a worker, 2 for a delegated sub-agent.
    pub depth: u8,
    /// Sub-agents this agent already spawned.
    pub spawned_subagents: u16,
    /// Queen-authored ceiling in force for this agent.
    pub ceiling: u16,
}

/// Task-flow counters rendered as progress bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveTaskFlow {
    /// Active tasks overall.
    pub processing: u8,
    /// Tasks in analysis-oriented roles.
    pub analyzing: u8,
    /// Tasks in planning-oriented roles.
    pub generating_plan: u8,
}

/// Active fan-out limits from [`HiveSettings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveLimits {
    /// Per-worker sub-agent ceiling.
    pub max_subagents_per_worker: u16,
    /// Maximum sub-agents a worker keeps in flight.
    pub max_parallel_subagents: u16,
}

/// Aggregated, serializable hive state for the operations panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiveStateSnapshot {
    /// Version of this projection contract.
    pub schema_version: u32,
    /// Queen status line.
    pub queen: HiveQueenState,
    /// Aggregate counters.
    pub metrics: HiveMetrics,
    /// Departments aggregated by role.
    pub departments: Vec<HiveDepartment>,
    /// Active agents with their fan-out progress.
    pub active_agents: Vec<HiveActiveAgent>,
    /// Task-flow counters.
    pub task_flow: HiveTaskFlow,
    /// Active fan-out limits.
    pub limits: HiveLimits,
}

impl Default for HiveStateSnapshot {
    fn default() -> Self {
        Self::empty(&HiveSettings::default())
    }
}

impl HiveStateSnapshot {
    /// Builds the safe empty snapshot for the given settings.
    ///
    /// Limits are still reported: the panel shows the configured ceilings even
    /// before the first task starts.
    pub fn empty(settings: &HiveSettings) -> Self {
        Self {
            schema_version: HIVE_STATE_SCHEMA_VERSION,
            queen: HiveQueenState {
                status: "idle".into(),
                name: DEFAULT_QUEEN_NAME.into(),
            },
            metrics: HiveMetrics::default(),
            departments: Vec::new(),
            active_agents: Vec::new(),
            task_flow: HiveTaskFlow::default(),
            limits: limits_for(settings),
        }
    }

    /// Aggregates the latest child projections into one snapshot.
    ///
    /// `registry_roles` lists roles known to the persistent agent registry. They
    /// appear with `active_count = 0` so an idle department stays visible.
    /// Terminal projections are ignored, which keeps the snapshot an
    /// "active hive" view and lets the caller pass every replay window.
    pub fn from_projection(
        settings: &HiveSettings,
        projections: &[ChildProjection],
        registry_roles: &[String],
    ) -> Self {
        let capacity = bounded_u16(settings.effective_worker_ceiling() as usize).max(1);
        let mut departments: std::collections::BTreeMap<&str, u16> =
            std::collections::BTreeMap::new();
        for role in registry_roles {
            departments.entry(role.as_str()).or_insert(0);
        }
        let mut active_agents = Vec::new();
        let mut processing = 0_u16;
        let mut analyzing = 0_u16;
        let mut generating_plan = 0_u16;
        let mut spawned = 0_u64;
        let mut ceiling = 0_u64;
        for projection in projections {
            if !is_active(projection) {
                continue;
            }
            let role = projection.role.as_str();
            *departments.entry(role).or_insert(0) += 1;
            processing = processing.saturating_add(1);
            if role_is_analysis(role) {
                analyzing = analyzing.saturating_add(1);
            }
            if role_is_planning(role) {
                generating_plan = generating_plan.saturating_add(1);
            }
            spawned = spawned.saturating_add(u64::from(projection.spawned_subagents));
            ceiling = ceiling.saturating_add(u64::from(projection.planned_ceiling));
            if active_agents.len() < MAX_HIVE_ACTIVE_AGENTS {
                active_agents.push(HiveActiveAgent {
                    id: bounded_id(&projection.child_task_id),
                    role: bounded_id(role),
                    current_action: current_action(projection.state),
                    depth: projection.depth,
                    spawned_subagents: bounded_u16(projection.spawned_subagents as usize),
                    ceiling: bounded_u16(projection.planned_ceiling as usize),
                });
            }
        }
        active_agents.sort_by(|left, right| left.id.cmp(&right.id));
        let mut departments: Vec<HiveDepartment> = departments
            .into_iter()
            .take(MAX_HIVE_DEPARTMENTS)
            .map(|(role, active_count)| HiveDepartment {
                role: bounded_id(role),
                active_count,
                max_capacity: capacity,
            })
            .collect();
        departments.sort_by(|left, right| left.role.cmp(&right.role));
        let total = active_agents.len();
        Self {
            schema_version: HIVE_STATE_SCHEMA_VERSION,
            queen: HiveQueenState {
                status: if total > 0 { "active" } else { "idle" }.into(),
                name: DEFAULT_QUEEN_NAME.into(),
            },
            metrics: HiveMetrics {
                total_agents_online: bounded_u16(total),
                active_tasks: bounded_u16(processing as usize),
                average_task_time_sec: 0,
                efficiency_percent: efficiency_percent(spawned, ceiling),
            },
            departments,
            active_agents,
            task_flow: HiveTaskFlow {
                processing: bounded_u8(processing as usize),
                analyzing: bounded_u8(analyzing as usize),
                generating_plan: bounded_u8(generating_plan as usize),
            },
            limits: limits_for(settings),
        }
    }
}

fn limits_for(settings: &HiveSettings) -> HiveLimits {
    HiveLimits {
        max_subagents_per_worker: bounded_u16(settings.max_subagents_per_worker as usize),
        max_parallel_subagents: bounded_u16(settings.max_parallel_subagents as usize),
    }
}

/// Returns true while the projection still represents working state.
fn is_active(projection: &ChildProjection) -> bool {
    if projection.dead_letter {
        return false;
    }
    !matches!(
        projection.state,
        CoordinatorState::Accepted
            | CoordinatorState::Rejected
            | CoordinatorState::Failed
            | CoordinatorState::Cancelled
            | CoordinatorState::TimedOut
            | CoordinatorState::Aborted
    )
}

fn current_action(state: CoordinatorState) -> String {
    match state {
        CoordinatorState::Created | CoordinatorState::Queued => "queued",
        CoordinatorState::Running => "executing",
        CoordinatorState::Validating => "validating report",
        CoordinatorState::WaitingParentAcceptance => "waiting for parent",
        CoordinatorState::RevisePlan => "revising plan",
        CoordinatorState::Accepted => "completed",
        CoordinatorState::Rejected => "rejected",
        CoordinatorState::Failed => "failed",
        CoordinatorState::Cancelled => "cancelled",
        CoordinatorState::TimedOut => "timed out",
        CoordinatorState::Aborted => "aborted",
    }
    .to_owned()
}

fn role_is_analysis(role: &str) -> bool {
    let role = role.to_ascii_lowercase();
    role.contains("research") || role.contains("analys") || role.contains("review")
}

fn role_is_planning(role: &str) -> bool {
    let role = role.to_ascii_lowercase();
    role.contains("planner") || role.contains("architect")
}

fn efficiency_percent(spawned: u64, ceiling: u64) -> u8 {
    if ceiling == 0 {
        return 0;
    }
    ((spawned.saturating_mul(100) / ceiling).min(100)) as u8
}

fn bounded_id(value: &str) -> String {
    value.chars().take(128).collect()
}

fn bounded_u16(value: usize) -> u16 {
    value.min(u16::MAX as usize) as u16
}

fn bounded_u8(value: usize) -> u8 {
    value.min(u8::MAX as usize) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::child_contracts::ChildBudget;

    fn projection(id: &str, role: &str, state: CoordinatorState) -> ChildProjection {
        ChildProjection {
            event_id: format!("{id}:event"),
            parent_task_id: "queen".into(),
            child_task_id: id.into(),
            role: role.into(),
            revision: 0,
            state,
            reason_code: None,
            parent_sequence: 1,
            budget: Some(ChildBudget {
                max_tokens: Some(1_000),
                max_time_seconds: Some(60),
                max_tool_calls: Some(4),
            }),
            lease_live: state == CoordinatorState::Running,
            dead_letter: false,
            depth: 1,
            report_status: None,
            report_summary: None,
            spawned_subagents: 2,
            planned_ceiling: 4,
            extensions_granted: 0,
            lazy_waiting: 0,
            in_flight_subagents: 1,
            extension_requested: false,
        }
    }

    #[test]
    fn hive_state_snapshot_aggregates_active_agents_by_role() {
        let settings = HiveSettings::default();
        let projections = vec![
            projection("a", "researcher", CoordinatorState::Running),
            projection("b", "researcher", CoordinatorState::Running),
            projection("c", "reviewer", CoordinatorState::Validating),
            projection("d", "researcher", CoordinatorState::Accepted),
        ];
        let snapshot = HiveStateSnapshot::from_projection(&settings, &projections, &[]);
        assert_eq!(snapshot.metrics.total_agents_online, 3);
        assert_eq!(snapshot.metrics.active_tasks, 3);
        assert_eq!(snapshot.active_agents.len(), 3);
        assert_eq!(snapshot.departments.len(), 2);
        let researcher = snapshot
            .departments
            .iter()
            .find(|department| department.role == "researcher")
            .expect("researcher department");
        assert_eq!(researcher.active_count, 2);
        assert_eq!(researcher.max_capacity, 16);
        let reviewer = snapshot
            .departments
            .iter()
            .find(|department| department.role == "reviewer")
            .expect("reviewer department");
        assert_eq!(reviewer.active_count, 1);
        assert_eq!(snapshot.task_flow.processing, 3);
        assert_eq!(snapshot.task_flow.analyzing, 3);
        assert_eq!(snapshot.task_flow.generating_plan, 0);
    }

    #[test]
    fn hive_state_reflects_queen_and_limits() {
        let settings = HiveSettings {
            max_subagents_per_worker: 6,
            max_parallel_subagents: 3,
            ..HiveSettings::default()
        };
        let projections = vec![projection("worker", "planner", CoordinatorState::Running)];
        let snapshot = HiveStateSnapshot::from_projection(&settings, &projections, &[]);
        assert_eq!(snapshot.schema_version, HIVE_STATE_SCHEMA_VERSION);
        assert_eq!(snapshot.queen.name, DEFAULT_QUEEN_NAME);
        assert_eq!(snapshot.queen.status, "active");
        assert_eq!(snapshot.limits.max_subagents_per_worker, 6);
        assert_eq!(snapshot.limits.max_parallel_subagents, 3);
        assert_eq!(snapshot.task_flow.generating_plan, 1);
        assert_eq!(snapshot.active_agents[0].ceiling, 4);
        assert_eq!(snapshot.active_agents[0].spawned_subagents, 2);
    }

    #[test]
    fn empty_registry_yields_empty_collections_without_panicking() {
        let snapshot = HiveStateSnapshot::from_projection(&HiveSettings::default(), &[], &[]);
        assert_eq!(snapshot.queen.status, "idle");
        assert_eq!(snapshot.metrics.total_agents_online, 0);
        assert_eq!(snapshot.metrics.active_tasks, 0);
        assert_eq!(snapshot.metrics.efficiency_percent, 0);
        assert!(snapshot.departments.is_empty());
        assert!(snapshot.active_agents.is_empty());
        assert_eq!(snapshot.task_flow, HiveTaskFlow::default());
        assert_eq!(snapshot.limits.max_subagents_per_worker, 16);
        assert_eq!(snapshot.limits.max_parallel_subagents, 4);
    }

    #[test]
    fn idle_registry_roles_are_visible_with_zero_load() {
        let snapshot = HiveStateSnapshot::from_projection(
            &HiveSettings::default(),
            &[],
            &["researcher".to_owned()],
        );
        assert_eq!(snapshot.departments.len(), 1);
        assert_eq!(snapshot.departments[0].role, "researcher");
        assert_eq!(snapshot.departments[0].active_count, 0);
        assert!(snapshot.active_agents.is_empty());
    }
}
