use crate::error::ContractError;
use serde::{Deserialize, Serialize};

pub const MIN_FRAME_MS: u32 = 10;
pub const MAX_FRAME_MS: u32 = 60;
pub const MAX_UTTERANCE_MS: u32 = 60_000;
pub const MAX_EPISODE_MS: u32 = 3_600_000;
pub const MAX_DEDUP_WINDOW_MS: u32 = 3_600_000;
/// Ceiling for pre-roll and hangover windows.
pub const MAX_WINDOW_MS: u32 = 5_000;

/// Immutable bounded capture limits for one listening session.
///
/// The listener checks every bound before it appends audio to an utterance;
/// the renderer may display the snapshot, but cannot raise a limit while the
/// stream is open.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AmbientLimits {
    /// Capture frame handed to the VAD.
    pub frame_ms: u32,
    /// Audio kept before speech onset so the first syllable is not cut.
    pub pre_roll_ms: u32,
    /// Silence tolerated inside one utterance before it is closed.
    pub hangover_ms: u32,
    /// Shorter speech is dropped: it is noise far more often than an utterance.
    pub min_utterance_ms: u32,
    /// Hard ceiling of a single utterance.
    pub max_utterance_ms: u32,
    /// Hard ceiling of one episode.
    pub max_episode_ms: u32,
    /// Window in which an identical transcript is treated as a duplicate.
    pub dedup_window_ms: u32,
}

impl AmbientLimits {
    pub const DEFAULT: AmbientLimits = AmbientLimits {
        frame_ms: 30,
        pre_roll_ms: 300,
        hangover_ms: 700,
        min_utterance_ms: 400,
        max_utterance_ms: 20_000,
        max_episode_ms: 600_000,
        dedup_window_ms: 60_000,
    };

    pub fn validate(&self) -> Result<(), ContractError> {
        if self.frame_ms < MIN_FRAME_MS || self.frame_ms > MAX_FRAME_MS {
            return Err(ContractError::LimitsOutOfBounds("frame_ms"));
        }
        // Whole frames, not exact multiples: the listener rounds a window up
        // to the next frame boundary, so 700 ms of hangover on a 30 ms frame
        // is a valid configuration, not a misconfiguration.
        if self.pre_roll_ms < self.frame_ms || self.pre_roll_ms > MAX_WINDOW_MS {
            return Err(ContractError::LimitsOutOfBounds("pre_roll_ms"));
        }
        if self.hangover_ms < self.frame_ms || self.hangover_ms > MAX_WINDOW_MS {
            return Err(ContractError::LimitsOutOfBounds("hangover_ms"));
        }
        if self.min_utterance_ms == 0 || self.min_utterance_ms >= self.max_utterance_ms {
            return Err(ContractError::LimitsOutOfBounds("min_utterance_ms"));
        }
        if self.max_utterance_ms > MAX_UTTERANCE_MS {
            return Err(ContractError::LimitsOutOfBounds("max_utterance_ms"));
        }
        if self.max_episode_ms < self.max_utterance_ms || self.max_episode_ms > MAX_EPISODE_MS {
            return Err(ContractError::LimitsOutOfBounds("max_episode_ms"));
        }
        if self.dedup_window_ms == 0 || self.dedup_window_ms > MAX_DEDUP_WINDOW_MS {
            return Err(ContractError::LimitsOutOfBounds("dedup_window_ms"));
        }
        Ok(())
    }

    /// True when an utterance of this length is worth transcribing.
    pub const fn accepts_utterance(&self, duration_ms: u32) -> bool {
        duration_ms >= self.min_utterance_ms && duration_ms <= self.max_utterance_ms
    }

    /// True when the episode must be closed and a new one opened.
    pub const fn episode_exhausted(&self, elapsed_ms: u32) -> bool {
        elapsed_ms >= self.max_episode_ms
    }
}

impl Default for AmbientLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_defaults_are_valid() {
        let limits = AmbientLimits::DEFAULT;
        assert_eq!(limits.frame_ms, 30);
        assert_eq!(limits.pre_roll_ms, 300);
        assert_eq!(limits.hangover_ms, 700);
        assert_eq!(limits.min_utterance_ms, 400);
        assert_eq!(limits.max_utterance_ms, 20_000);
        assert_eq!(limits.max_episode_ms, 600_000);
        assert_eq!(limits.dedup_window_ms, 60_000);
        assert_eq!(limits.validate(), Ok(()));
    }

    #[test]
    fn out_of_bounds_limits_are_rejected() {
        let cases: [(AmbientLimits, &'static str); 6] = [
            (
                AmbientLimits {
                    frame_ms: 500,
                    ..AmbientLimits::DEFAULT
                },
                "frame_ms",
            ),
            (
                AmbientLimits {
                    pre_roll_ms: 10_000,
                    ..AmbientLimits::DEFAULT
                },
                "pre_roll_ms",
            ),
            (
                AmbientLimits {
                    hangover_ms: 5,
                    ..AmbientLimits::DEFAULT
                },
                "hangover_ms",
            ),
            (
                AmbientLimits {
                    min_utterance_ms: 30_000,
                    ..AmbientLimits::DEFAULT
                },
                "min_utterance_ms",
            ),
            (
                AmbientLimits {
                    max_utterance_ms: 120_000,
                    ..AmbientLimits::DEFAULT
                },
                "max_utterance_ms",
            ),
            (
                AmbientLimits {
                    dedup_window_ms: 0,
                    ..AmbientLimits::DEFAULT
                },
                "dedup_window_ms",
            ),
        ];
        for (limits, field) in cases {
            assert_eq!(
                limits.validate(),
                Err(ContractError::LimitsOutOfBounds(field))
            );
        }
    }

    #[test]
    fn utterance_window_is_closed_on_both_ends() {
        let limits = AmbientLimits::DEFAULT;
        assert!(!limits.accepts_utterance(399));
        assert!(limits.accepts_utterance(400));
        assert!(limits.accepts_utterance(20_000));
        assert!(!limits.accepts_utterance(20_001));
        assert!(!limits.episode_exhausted(599_999));
        assert!(limits.episode_exhausted(600_000));
    }
}
