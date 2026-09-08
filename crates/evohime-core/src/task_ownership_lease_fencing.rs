//! Shared fail-closed fencing contract for existing Core lease owners.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceDecision {
    Allowed,
    Expired,
    StaleGeneration,
    WrongOwner,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseFence {
    pub owner_id: String,
    pub generation: u64,
    pub deadline_ms: u64,
}

impl LeaseFence {
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
