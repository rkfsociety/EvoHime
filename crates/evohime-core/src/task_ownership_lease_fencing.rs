//! Shared fail-closed fencing contract for existing Core lease owners.

/// Outcome of validating a lease owner, generation, and deadline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceDecision {
    /// The lease matches and remains valid.
    Allowed,
    /// The lease deadline has passed.
    Expired,
    /// The caller holds an older lease generation.
    StaleGeneration,
    /// The lease belongs to a different owner or has no owner.
    WrongOwner,
    /// Lease state could not be established safely.
    Unavailable,
}

/// Fencing data required to validate an existing lease owner.
///
/// ```
/// use evohime_core::task_ownership_lease_fencing::{FenceDecision, LeaseFence};
/// let fence = LeaseFence { owner_id: "worker-1".into(), generation: 4, deadline_ms: 100 };
/// assert_eq!(fence.check("worker-1", 4, 99), FenceDecision::Allowed);
/// assert_eq!(fence.check("worker-1", 3, 99), FenceDecision::StaleGeneration);
/// assert_eq!(fence.check("worker-1", 4, 100), FenceDecision::Expired);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseFence {
    /// Identifier of the lease owner.
    pub owner_id: String,
    /// Monotonically changing generation that fences stale owners.
    pub generation: u64,
    /// Expiration time in the same millisecond clock domain as `now_ms`.
    pub deadline_ms: u64,
}

impl LeaseFence {
    /// Checks the supplied owner and generation against this lease and current time.
    pub fn check(&self, owner_id: &str, generation: u64, now_ms: u64) -> FenceDecision {
        if self.owner_id.is_empty() || self.owner_id != owner_id {
            return FenceDecision::WrongOwner;
        }
        if generation != self.generation {
            return FenceDecision::StaleGeneration;
        }
        if now_ms >= self.deadline_ms {
            return FenceDecision::Expired;
        }
        FenceDecision::Allowed
    }
}
