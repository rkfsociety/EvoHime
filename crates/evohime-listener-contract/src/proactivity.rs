use crate::{error::ContractError, policy::AmbientPolicy};
use serde::{Deserialize, Serialize};

/// Default maximum number of proposals shown per hour.
pub const MAX_PROPOSALS_PER_HOUR: u32 = 3;
/// Default maximum number of proposals shown per day.
pub const MAX_PROPOSALS_PER_DAY: u32 = 10;
/// Default minimum elapsed time between shown proposals.
pub const MIN_PROPOSAL_INTERVAL_MS: u64 = 10 * 60 * 1000;

const HARD_MAX_PER_HOUR: u32 = 12;
const HARD_MAX_PER_DAY: u32 = 48;

/// Immutable snapshot of the proactivity ceiling, by the same rule as
/// `evohime_core::run_policy::RunPolicy`: the renderer may display it, nothing
/// may raise it at runtime.
///
/// Current counters are deliberately **not** part of the snapshot — they live
/// in `AmbientProactivityRegistry` inside Core and are passed in as
/// [`ProactivityCounters`], so this type stays a pure bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProactivityBudget {
    /// Maximum proposals allowed in one hour.
    pub max_per_hour: u32,
    /// Maximum proposals allowed in one day.
    pub max_per_day: u32,
    /// Minimum interval between proposals, in milliseconds.
    pub min_interval_ms: u64,
}

/// Counters owned by Core and handed to the budget for a decision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProactivityCounters {
    /// Proposals already shown in the current hour window.
    pub hour_count: u32,
    /// Proposals already shown in the current day window.
    pub day_count: u32,
    /// Unix millis of the last proposal shown, if any.
    pub last_proposed_at_ms: Option<u64>,
}

/// Why a proposal was dropped.  A proposal over the ceiling is discarded with
/// a counter, never queued: an hour of silence must not turn into ten cards.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProactivityDenial {
    /// User or policy has paused proactive suggestions.
    Paused,
    /// Current local time is within a quiet-hours window.
    QuietHours,
    /// Hourly proposal ceiling has been reached.
    HourlyCapReached,
    /// Daily proposal ceiling has been reached.
    DailyCapReached,
    /// Minimum spacing has not elapsed; includes time remaining.
    TooSoon {
        /// Milliseconds remaining before another proposal may be shown.
        retry_after_ms: u64,
    },
}

impl ProactivityBudget {
    /// Default immutable proactivity ceilings.
    pub const DEFAULT: ProactivityBudget = ProactivityBudget {
        max_per_hour: MAX_PROPOSALS_PER_HOUR,
        max_per_day: MAX_PROPOSALS_PER_DAY,
        min_interval_ms: MIN_PROPOSAL_INTERVAL_MS,
    };

    /// Validates budget values against hard limits and cross-field rules.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.max_per_hour == 0 || self.max_per_hour > HARD_MAX_PER_HOUR {
            return Err(ContractError::BudgetOutOfBounds("max_per_hour"));
        }
        if self.max_per_day < self.max_per_hour || self.max_per_day > HARD_MAX_PER_DAY {
            return Err(ContractError::BudgetOutOfBounds("max_per_day"));
        }
        if self.min_interval_ms == 0 {
            return Err(ContractError::BudgetOutOfBounds("min_interval_ms"));
        }
        Ok(())
    }

    /// Budget half of the decision: caps and spacing only.
    pub fn check(
        &self,
        counters: ProactivityCounters,
        now_ms: u64,
    ) -> Result<(), ProactivityDenial> {
        if counters.hour_count >= self.max_per_hour {
            return Err(ProactivityDenial::HourlyCapReached);
        }
        if counters.day_count >= self.max_per_day {
            return Err(ProactivityDenial::DailyCapReached);
        }
        if let Some(last) = counters.last_proposed_at_ms {
            // A clock that moved backwards must not unlock the interval.
            let elapsed = now_ms.saturating_sub(last);
            if now_ms < last || elapsed < self.min_interval_ms {
                return Err(ProactivityDenial::TooSoon {
                    retry_after_ms: self.min_interval_ms.saturating_sub(elapsed),
                });
            }
        }
        Ok(())
    }

    /// Full decision: policy first (pause and quiet hours suppress a proposal
    /// outright), then the budget.
    pub fn decide(
        &self,
        policy: &AmbientPolicy,
        counters: ProactivityCounters,
        now_ms: u64,
        minute_of_day: u32,
    ) -> Result<(), ProactivityDenial> {
        if policy.paused {
            return Err(ProactivityDenial::Paused);
        }
        if policy.is_quiet_at(minute_of_day) {
            return Err(ProactivityDenial::QuietHours);
        }
        self.check(counters, now_ms)
    }
}

impl Default for ProactivityBudget {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Default proposal lifetime before user silence counts as expiration.
pub const PROPOSAL_LIFETIME_MS: u64 = 24 * 60 * 60 * 1000;

/// Width of the deduplication time bucket in `proposal_key`.
pub const PROPOSAL_BUCKET_MS: u64 = 60 * 60 * 1000;

const ROLLING_HOUR_MS: u64 = 60 * 60 * 1000;
const ROLLING_DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// Maximum byte length of a normalized subject key.
pub const MAX_SUBJECT_KEY_BYTES: usize = 64;

/// Closed set of effects proactivity may attempt to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProactiveEffect {
    /// Proposal card shown in the UI.
    ProposalCard,
    /// Non-executable reminder item.
    PendingReminder,
    /// Autonomous task launch (never allowed proactively).
    StartTask,
    /// Tool invocation (never allowed proactively).
    ToolCall,
    /// File modification (never allowed proactively).
    FileWrite,
    /// Network egress (never allowed proactively).
    NetworkRequest,
}

impl ProactiveEffect {
    /// Canonical list of all defined effects.
    pub const ALL: [ProactiveEffect; 6] = [
        ProactiveEffect::ProposalCard,
        ProactiveEffect::PendingReminder,
        ProactiveEffect::StartTask,
        ProactiveEffect::ToolCall,
        ProactiveEffect::FileWrite,
        ProactiveEffect::NetworkRequest,
    ];

    /// Canonical wire string for traces and JSON capability payloads.
    pub const fn as_str(self) -> &'static str {
        match self {
            ProactiveEffect::ProposalCard => "proposal_card",
            ProactiveEffect::PendingReminder => "pending_reminder",
            ProactiveEffect::StartTask => "start_task",
            ProactiveEffect::ToolCall => "tool_call",
            ProactiveEffect::FileWrite => "file_write",
            ProactiveEffect::NetworkRequest => "network_request",
        }
    }

    /// Whether the effect is allowed without an explicit user click.
    pub const fn is_proactively_allowed(self) -> bool {
        matches!(
            self,
            ProactiveEffect::ProposalCard | ProactiveEffect::PendingReminder
        )
    }

    /// Parses an effect from its canonical [`Self::as_str`] representation.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|effect| effect.as_str() == value)
    }
}

/// Reason why a proactive proposal attempt was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalRejection {
    /// Effect is outside the closed proactively-allowed list.
    Forbidden(ProactiveEffect),
    /// Subject was muted by a prior user choice.
    Muted,
    /// Policy or ceiling denial from the proactivity budget.
    Denied(ProactivityDenial),
    /// Decision service did not respond or returned an invalid payload.
    ServiceUnavailable,
}

impl ProposalRejection {
    /// Canonical string code for trace and log events.
    pub fn as_str(self) -> &'static str {
        match self {
            ProposalRejection::Forbidden(_) => "effect_forbidden",
            ProposalRejection::Muted => "muted",
            ProposalRejection::Denied(ProactivityDenial::Paused) => "paused",
            ProposalRejection::Denied(ProactivityDenial::QuietHours) => "quiet_hours",
            ProposalRejection::Denied(ProactivityDenial::HourlyCapReached) => "hourly_cap",
            ProposalRejection::Denied(ProactivityDenial::DailyCapReached) => "daily_cap",
            ProposalRejection::Denied(ProactivityDenial::TooSoon { .. }) => "too_soon",
            ProposalRejection::ServiceUnavailable => "service_unavailable",
        }
    }
}

/// Proof token representing an effect authorized for proactive presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorizedEffect(ProactiveEffect);

impl AuthorizedEffect {
    /// Returns the authorized effect.
    pub const fn effect(self) -> ProactiveEffect {
        self.0
    }
}

/// Authorizes an effect only if it belongs to the closed proactive subset.
pub fn authorize_proactive(effect: ProactiveEffect) -> Result<AuthorizedEffect, ProposalRejection> {
    if effect.is_proactively_allowed() {
        Ok(AuthorizedEffect(effect))
    } else {
        Err(ProposalRejection::Forbidden(effect))
    }
}

/// Maps a proposal kind to the proactive effect it produces.
pub const fn effect_of(kind: crate::log::ProposalKind) -> ProactiveEffect {
    match kind {
        crate::log::ProposalKind::Suggestion => ProactiveEffect::ProposalCard,
        crate::log::ProposalKind::Reminder => ProactiveEffect::PendingReminder,
    }
}

/// Normalizes a canonical subject string into a bounded [`crate::ids::SubjectKey`].
pub fn subject_key(subject: &str) -> crate::ids::SubjectKey {
    let mut slug = String::new();
    let mut pending_separator = false;
    for character in subject.trim().chars() {
        if character.is_ascii_alphanumeric() {
            if pending_separator && !slug.is_empty() {
                slug.push('-');
            }
            pending_separator = false;
            slug.push(character.to_ascii_lowercase());
        } else {
            pending_separator = true;
        }
        if slug.len() >= MAX_SUBJECT_KEY_BYTES {
            break;
        }
    }
    if slug.is_empty() {
        slug = subject_fingerprint(subject);
    }
    match crate::ids::SubjectKey::new(slug) {
        Ok(key) => key,
        Err(_) => match crate::ids::SubjectKey::new(subject_fingerprint(subject)) {
            Ok(key) => key,
            Err(_) => match crate::ids::SubjectKey::new("invalid-subject") {
                Ok(key) => key,
                Err(_) => unreachable!("static subject key is part of the contract"),
            },
        },
    }
}

fn subject_fingerprint(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(value.trim().to_lowercase().as_bytes());
    hasher
        .finalize()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Deduplication key: proposal kind + normalized subject + time bucket.
pub fn proposal_key(
    kind: crate::log::ProposalKind,
    subject: &crate::ids::SubjectKey,
    now_ms: u64,
) -> String {
    format!(
        "{}:{}:{}",
        kind.as_str(),
        subject.as_str(),
        now_ms / PROPOSAL_BUCKET_MS
    )
}

/// Permanent mute key: proposal kind + normalized subject without time bucket.
pub fn mute_key(kind: crate::log::ProposalKind, subject: &crate::ids::SubjectKey) -> String {
    format!("{}:{}", kind.as_str(), subject.as_str())
}

/// Rolling hour/day counter state persisted by Core and sent to `ambient`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollingCounters {
    /// Start of the current hourly window in Unix milliseconds.
    pub hour_started_at_ms: u64,
    /// Proposals shown in the current hourly window.
    pub hour_count: u32,
    /// Start of the current daily window in Unix milliseconds.
    pub day_started_at_ms: u64,
    /// Proposals shown in the current daily window.
    pub day_count: u32,
    /// Timestamp of the last shown proposal in Unix milliseconds.
    pub last_proposed_at_ms: Option<u64>,
}

impl RollingCounters {
    /// Advances rolling windows to `now_ms`.
    pub fn rolled(mut self, now_ms: u64) -> Self {
        if now_ms.saturating_sub(self.hour_started_at_ms) >= ROLLING_HOUR_MS {
            self.hour_started_at_ms = now_ms;
            self.hour_count = 0;
        }
        if now_ms.saturating_sub(self.day_started_at_ms) >= ROLLING_DAY_MS {
            self.day_started_at_ms = now_ms;
            self.day_count = 0;
        }
        self
    }

    /// Converts rolling counters into a bounded [`ProactivityCounters`] snapshot.
    pub fn snapshot(self) -> ProactivityCounters {
        ProactivityCounters {
            hour_count: self.hour_count,
            day_count: self.day_count,
            last_proposed_at_ms: self.last_proposed_at_ms,
        }
    }

    /// Records a shown proposal at `now_ms`.
    pub fn record(mut self, now_ms: u64) -> Self {
        self = self.rolled(now_ms);
        if self.hour_count == 0 {
            self.hour_started_at_ms = now_ms;
        }
        if self.day_count == 0 {
            self.day_started_at_ms = now_ms;
        }
        self.hour_count = self.hour_count.saturating_add(1);
        self.day_count = self.day_count.saturating_add(1);
        self.last_proposed_at_ms = Some(now_ms);
        self
    }
}

/// Bounded input of the `ambient.proactivity_decide` capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProactivityDecideRequest {
    /// Effect the proposal would produce, in [`ProactiveEffect::as_str`] form.
    pub effect: String,
    /// Whether the subject is muted by a prior user choice.
    pub muted: bool,
    /// Immutable user policy read by Core from its data directory.
    pub policy: AmbientPolicy,
    /// Immutable proactivity ceilings from the listener contract.
    pub budget: ProactivityBudget,
    /// Rolling counters loaded from Core's journal.
    pub counters: RollingCounters,
    /// Current wall-clock time in milliseconds.
    pub now_ms: u64,
    /// Current local minute of day (0..=1439), computed by Core.
    pub minute_of_day: u32,
}

/// Bounded output of the `ambient.proactivity_decide` capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProactivityDecideResponse {
    /// `true` when the proposal may be shown to the user.
    pub authorized: bool,
    /// Authorized effect in [`ProactiveEffect::as_str`] form; set when
    /// [`Self::authorized`] is `true`.
    pub effect: Option<String>,
    /// Typed rejection reason; set when [`Self::authorized`] is `false`.
    pub rejection: Option<ProactivityRejectionPayload>,
}

/// Typed rejection reason crossing the process boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProactivityRejectionPayload {
    /// The effect is outside the closed proactively-allowed list.
    Forbidden {
        /// Rejected effect in [`ProactiveEffect::as_str`] form.
        effect: String,
    },
    /// The subject was muted by a prior user choice.
    Muted,
    /// Policy or ceiling denial from the listener contract.
    Denied {
        /// Typed denial with its bounded payload.
        denial: ProactivityDenial,
    },
}

impl ProactivityRejectionPayload {
    /// Converts a [`ProposalRejection`] into its wire payload, if representable.
    pub fn from_rejection(rejection: ProposalRejection) -> Option<Self> {
        match rejection {
            ProposalRejection::Forbidden(effect) => Some(Self::Forbidden {
                effect: effect.as_str().to_owned(),
            }),
            ProposalRejection::Muted => Some(Self::Muted),
            ProposalRejection::Denied(denial) => Some(Self::Denied { denial }),
            ProposalRejection::ServiceUnavailable => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::QuietHours;

    const HOUR_MS: u64 = 3_600_000;

    #[test]
    fn documented_ceiling_is_three_per_hour_ten_per_day_ten_minutes_apart() {
        let budget = ProactivityBudget::DEFAULT;
        assert_eq!(budget.max_per_hour, 3);
        assert_eq!(budget.max_per_day, 10);
        assert_eq!(budget.min_interval_ms, 600_000);
        assert_eq!(budget.validate(), Ok(()));
        assert_eq!(
            budget.check(ProactivityCounters::default(), HOUR_MS),
            Ok(())
        );
    }

    #[test]
    fn caps_and_spacing_are_enforced() {
        let budget = ProactivityBudget::DEFAULT;
        assert_eq!(
            budget.check(
                ProactivityCounters {
                    hour_count: 3,
                    day_count: 3,
                    last_proposed_at_ms: None,
                },
                HOUR_MS
            ),
            Err(ProactivityDenial::HourlyCapReached)
        );
        assert_eq!(
            budget.check(
                ProactivityCounters {
                    hour_count: 0,
                    day_count: 10,
                    last_proposed_at_ms: None,
                },
                HOUR_MS
            ),
            Err(ProactivityDenial::DailyCapReached)
        );
        assert_eq!(
            budget.check(
                ProactivityCounters {
                    hour_count: 1,
                    day_count: 1,
                    last_proposed_at_ms: Some(HOUR_MS),
                },
                HOUR_MS + 599_999
            ),
            Err(ProactivityDenial::TooSoon { retry_after_ms: 1 })
        );
        assert_eq!(
            budget.check(
                ProactivityCounters {
                    hour_count: 1,
                    day_count: 1,
                    last_proposed_at_ms: Some(HOUR_MS),
                },
                HOUR_MS + 600_000
            ),
            Ok(())
        );
    }

    #[test]
    fn backwards_clock_does_not_unlock_the_interval() {
        let budget = ProactivityBudget::DEFAULT;
        assert_eq!(
            budget.check(
                ProactivityCounters {
                    hour_count: 0,
                    day_count: 0,
                    last_proposed_at_ms: Some(2 * HOUR_MS),
                },
                HOUR_MS
            ),
            Err(ProactivityDenial::TooSoon {
                retry_after_ms: 600_000
            })
        );
    }

    #[test]
    fn pause_and_quiet_hours_suppress_proposals() {
        let budget = ProactivityBudget::DEFAULT;
        let paused = AmbientPolicy {
            paused: true,
            ..AmbientPolicy::default()
        };
        assert_eq!(
            budget.decide(&paused, ProactivityCounters::default(), HOUR_MS, 12 * 60),
            Err(ProactivityDenial::Paused)
        );
        let quiet = AmbientPolicy {
            quiet_hours: vec![QuietHours::new(23 * 60, 7 * 60).unwrap()],
            ..AmbientPolicy::default()
        };
        assert_eq!(
            budget.decide(&quiet, ProactivityCounters::default(), HOUR_MS, 2 * 60),
            Err(ProactivityDenial::QuietHours)
        );
        assert_eq!(
            budget.decide(&quiet, ProactivityCounters::default(), HOUR_MS, 12 * 60),
            Ok(())
        );
    }

    #[test]
    fn absurd_budgets_are_rejected() {
        for (budget, field) in [
            (
                ProactivityBudget {
                    max_per_hour: 0,
                    ..ProactivityBudget::DEFAULT
                },
                "max_per_hour",
            ),
            (
                ProactivityBudget {
                    max_per_hour: 100,
                    ..ProactivityBudget::DEFAULT
                },
                "max_per_hour",
            ),
            (
                ProactivityBudget {
                    max_per_day: 1,
                    ..ProactivityBudget::DEFAULT
                },
                "max_per_day",
            ),
            (
                ProactivityBudget {
                    min_interval_ms: 0,
                    ..ProactivityBudget::DEFAULT
                },
                "min_interval_ms",
            ),
        ] {
            assert_eq!(
                budget.validate(),
                Err(ContractError::BudgetOutOfBounds(field))
            );
        }
    }
}
