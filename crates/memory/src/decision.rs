//! Decision gate for ask-on-uncertainty (roadmap 6.20).

use evohime_storage::{MemoryKind, MemoryScope, MemoryStatus};

/// Minimum confidence for automatic promotion.
pub const AUTO_PROMOTE_CONFIDENCE: f64 = 0.7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    AutoPromote,
    Ask { reason: String },
    Drop { reason: String },
}

#[derive(Debug, Clone)]
pub struct GateInput {
    pub scope: MemoryScope,
    pub kind: MemoryKind,
    pub status: MemoryStatus,
    pub content: String,
    pub confidence: f64,
    pub pinned: bool,
    /// True when admit detected a conflict with an active item.
    pub had_conflict: bool,
    /// True when admit found a duplicate (no new row / skip ask).
    pub was_duplicate: bool,
    /// True when admit rejected (secrets / empty).
    pub was_rejected: bool,
}

pub fn decide_gate(input: &GateInput) -> GateDecision {
    if input.was_rejected {
        return GateDecision::Drop {
            reason: "rejected by admit (empty or secrets)".into(),
        };
    }
    if input.was_duplicate {
        return GateDecision::Drop {
            reason: "duplicate of existing memory".into(),
        };
    }
    if input.had_conflict || input.status == MemoryStatus::Conflict {
        return GateDecision::Ask {
            reason: "conflicts with an existing memory item".into(),
        };
    }
    if input.pinned {
        return GateDecision::Ask {
            reason: "pinned memory requires operator confirmation".into(),
        };
    }
    if input.scope == MemoryScope::Global {
        return GateDecision::Ask {
            reason: "global memory requires operator confirmation".into(),
        };
    }
    if input.kind == MemoryKind::Constraint {
        return GateDecision::Ask {
            reason: "constraint memory requires operator confirmation".into(),
        };
    }
    if input.confidence < AUTO_PROMOTE_CONFIDENCE {
        return GateDecision::Ask {
            reason: format!(
                "confidence {:.2} below auto-promote threshold {:.2}",
                input.confidence, AUTO_PROMOTE_CONFIDENCE
            ),
        };
    }
    if looks_high_impact(&input.content) {
        return GateDecision::Ask {
            reason: "high-impact policy language detected".into(),
        };
    }

    match (input.scope, input.kind) {
        (
            MemoryScope::Session | MemoryScope::Workspace | MemoryScope::Project,
            MemoryKind::Fact | MemoryKind::Preference,
        ) if input.confidence >= AUTO_PROMOTE_CONFIDENCE => GateDecision::AutoPromote,
        _ => GateDecision::Ask {
            reason: "scope/kind not eligible for auto-promote".into(),
        },
    }
}

fn looks_high_impact(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    let has_policy = lower.contains("always ")
        || lower.contains("never ")
        || lower.starts_with("always")
        || lower.starts_with("never")
        || lower.contains("must not")
        || lower.contains("forbid");
    let has_security = lower.contains("security")
        || lower.contains("password")
        || lower.contains("credential")
        || lower.contains("private key")
        || lower.contains("api key");
    has_policy || has_security
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> GateInput {
        GateInput {
            scope: MemoryScope::Workspace,
            kind: MemoryKind::Fact,
            status: MemoryStatus::Candidate,
            content: "prefer worktrees for parallel agents".into(),
            confidence: 0.8,
            pinned: false,
            had_conflict: false,
            was_duplicate: false,
            was_rejected: false,
        }
    }

    #[test]
    fn auto_promotes_high_confidence_workspace_fact() {
        assert_eq!(decide_gate(&base()), GateDecision::AutoPromote);
    }

    #[test]
    fn asks_on_low_confidence() {
        let mut input = base();
        input.confidence = 0.55;
        assert!(matches!(decide_gate(&input), GateDecision::Ask { .. }));
    }

    #[test]
    fn asks_on_global_scope() {
        let mut input = base();
        input.scope = MemoryScope::Global;
        assert!(matches!(decide_gate(&input), GateDecision::Ask { .. }));
    }

    #[test]
    fn asks_on_pinned() {
        let mut input = base();
        input.pinned = true;
        assert!(matches!(decide_gate(&input), GateDecision::Ask { .. }));
    }

    #[test]
    fn asks_on_constraint() {
        let mut input = base();
        input.kind = MemoryKind::Constraint;
        assert!(matches!(decide_gate(&input), GateDecision::Ask { .. }));
    }

    #[test]
    fn asks_on_conflict() {
        let mut input = base();
        input.had_conflict = true;
        input.status = MemoryStatus::Conflict;
        assert!(matches!(decide_gate(&input), GateDecision::Ask { .. }));
    }

    #[test]
    fn drops_duplicates_and_rejects() {
        let mut dup = base();
        dup.was_duplicate = true;
        assert!(matches!(decide_gate(&dup), GateDecision::Drop { .. }));

        let mut rej = base();
        rej.was_rejected = true;
        assert!(matches!(decide_gate(&rej), GateDecision::Drop { .. }));
    }

    #[test]
    fn asks_on_high_impact_language() {
        let mut input = base();
        input.content = "never commit secrets to the repo".into();
        assert!(matches!(decide_gate(&input), GateDecision::Ask { .. }));
    }

    #[test]
    fn asks_on_experience_even_with_high_confidence() {
        let mut input = base();
        input.scope = MemoryScope::Experience;
        input.kind = MemoryKind::FailurePattern;
        input.confidence = 0.9;
        assert!(matches!(decide_gate(&input), GateDecision::Ask { .. }));
    }
}
