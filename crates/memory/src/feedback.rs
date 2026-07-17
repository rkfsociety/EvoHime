//! Memory feedback loop: helpful/harmful/idle confidence updates (roadmap 6.23).

use evohime_storage::MemoryStatus;

/// Soft bump when a task succeeds after using the item.
pub const HELPFUL_CONFIDENCE_BUMP: f64 = 0.03;
pub const HELPFUL_IMPORTANCE_BUMP: f64 = 0.01;

/// Decay when a task fails after using the item.
pub const HARMFUL_CONFIDENCE_DECAY: f64 = 0.08;
pub const HARMFUL_IMPORTANCE_DECAY: f64 = 0.02;

/// Idle decay for prolonged unused low-confidence items.
pub const IDLE_CONFIDENCE_DECAY: f64 = 0.02;

/// Below this (and not pinned) harmful/idle may archive.
pub const ARCHIVE_CONFIDENCE_THRESHOLD: f64 = 0.25;

/// Tiny bump for mere retrieval attribution.
pub const USED_CONFIDENCE_BUMP: f64 = 0.005;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackSignal {
    Used,
    Helpful,
    Harmful,
    Corrected,
    Rejected,
    IdleDecay,
}

impl FeedbackSignal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Used => "used",
            Self::Helpful => "helpful",
            Self::Harmful => "harmful",
            Self::Corrected => "corrected",
            Self::Rejected => "rejected",
            Self::IdleDecay => "idle_decay",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "used" => Some(Self::Used),
            "helpful" => Some(Self::Helpful),
            "harmful" => Some(Self::Harmful),
            "corrected" => Some(Self::Corrected),
            "rejected" => Some(Self::Rejected),
            "idle_decay" => Some(Self::IdleDecay),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeedbackAdjustment {
    pub confidence: f64,
    pub importance: f64,
    pub delta_confidence: f64,
    pub next_status: Option<MemoryStatus>,
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

/// Compute the next confidence/importance/status for a feedback signal.
pub fn apply_feedback_signal(
    confidence: f64,
    importance: f64,
    pinned: bool,
    status: MemoryStatus,
    signal: FeedbackSignal,
) -> FeedbackAdjustment {
    let before = clamp01(confidence);
    let mut next_importance = clamp01(importance);
    let mut next_status: Option<MemoryStatus> = None;

    let next_confidence = match signal {
        FeedbackSignal::Used => clamp01(before + USED_CONFIDENCE_BUMP),
        FeedbackSignal::Helpful => {
            next_importance = clamp01(next_importance + HELPFUL_IMPORTANCE_BUMP);
            clamp01(before + HELPFUL_CONFIDENCE_BUMP)
        }
        FeedbackSignal::Harmful => {
            next_importance = clamp01(next_importance - HARMFUL_IMPORTANCE_DECAY);
            let conf = clamp01(before - HARMFUL_CONFIDENCE_DECAY);
            if !pinned
                && conf < ARCHIVE_CONFIDENCE_THRESHOLD
                && matches!(status, MemoryStatus::Active | MemoryStatus::Candidate)
            {
                next_status = Some(MemoryStatus::Archived);
            }
            conf
        }
        FeedbackSignal::Corrected => {
            next_importance = clamp01(next_importance + HELPFUL_IMPORTANCE_BUMP);
            if matches!(status, MemoryStatus::Candidate | MemoryStatus::Conflict) {
                next_status = Some(MemoryStatus::Active);
            }
            clamp01(before.max(0.7))
        }
        FeedbackSignal::Rejected => {
            next_status = Some(MemoryStatus::Rejected);
            clamp01(before * 0.5)
        }
        FeedbackSignal::IdleDecay => {
            let conf = clamp01(before - IDLE_CONFIDENCE_DECAY);
            if !pinned
                && conf < ARCHIVE_CONFIDENCE_THRESHOLD
                && matches!(status, MemoryStatus::Active | MemoryStatus::Candidate)
            {
                next_status = Some(MemoryStatus::Archived);
            }
            conf
        }
    };

    FeedbackAdjustment {
        confidence: next_confidence,
        importance: next_importance,
        delta_confidence: next_confidence - before,
        next_status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpful_bumps_confidence() {
        let adj = apply_feedback_signal(
            0.5,
            0.5,
            false,
            MemoryStatus::Active,
            FeedbackSignal::Helpful,
        );
        assert!((adj.confidence - 0.53).abs() < 1e-9);
        assert!(adj.delta_confidence > 0.0);
        assert!(adj.next_status.is_none());
    }

    #[test]
    fn harmful_can_archive_low_confidence() {
        let adj = apply_feedback_signal(
            0.28,
            0.4,
            false,
            MemoryStatus::Active,
            FeedbackSignal::Harmful,
        );
        assert!(adj.confidence < ARCHIVE_CONFIDENCE_THRESHOLD);
        assert_eq!(adj.next_status, Some(MemoryStatus::Archived));
    }

    #[test]
    fn harmful_does_not_archive_pinned() {
        let adj = apply_feedback_signal(
            0.2,
            0.4,
            true,
            MemoryStatus::Active,
            FeedbackSignal::Harmful,
        );
        assert!(adj.next_status.is_none());
    }

    #[test]
    fn rejected_marks_rejected_and_halves_confidence() {
        let adj = apply_feedback_signal(
            0.8,
            0.5,
            false,
            MemoryStatus::Candidate,
            FeedbackSignal::Rejected,
        );
        assert!((adj.confidence - 0.4).abs() < 1e-9);
        assert_eq!(adj.next_status, Some(MemoryStatus::Rejected));
    }

    #[test]
    fn idle_decay_archives_when_low() {
        let adj = apply_feedback_signal(
            0.26,
            0.3,
            false,
            MemoryStatus::Candidate,
            FeedbackSignal::IdleDecay,
        );
        assert_eq!(adj.next_status, Some(MemoryStatus::Archived));
    }
}
