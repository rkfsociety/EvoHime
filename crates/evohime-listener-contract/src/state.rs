use crate::error::ContractError;
use serde::{Deserialize, Serialize};

/// Listening state machine.  Only `Listening` opens the capture stream.
///
/// `Denied` is terminal until the capability is granted again: it may only
/// fall back to `Stopped`, so a revoked microphone can never resume without
/// passing through the full start path and its capability check.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListeningState {
    Stopped,
    Starting,
    Listening,
    PausedByUser,
    PausedByPolicy,
    DeviceConflict,
    DeviceDisconnected,
    EngineUnavailable,
    Denied,
}

/// Why the listener is in its current state.  Closed set: the reason is shown
/// in the UI and must never be a free-form message.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListeningReason {
    UserRequest,
    QuietHours,
    Blocklist,
    StopWord,
    PermissionDenied,
    DeviceConflict,
    DeviceDisconnected,
    EngineUnavailable,
    /// Движок распознавания не укладывается в бюджет даже на самой лёгкой
    /// модели лестницы (этап 04.4). Слушание остановлено политикой, а не
    /// пользователем и не отказом устройства.
    EngineDegraded,
    SystemSleep,
    StorageFailed,
    Unknown,
}

impl ListeningState {
    pub const ALL: [ListeningState; 9] = [
        ListeningState::Stopped,
        ListeningState::Starting,
        ListeningState::Listening,
        ListeningState::PausedByUser,
        ListeningState::PausedByPolicy,
        ListeningState::DeviceConflict,
        ListeningState::DeviceDisconnected,
        ListeningState::EngineUnavailable,
        ListeningState::Denied,
    ];

    /// True only while the capture stream is open.  Pause, quiet hours and
    /// blocklist close the stream instead of filtering frames, so every other
    /// state means the microphone is not being read.
    pub const fn is_capturing(self) -> bool {
        matches!(self, ListeningState::Listening)
    }

    /// True when the UI must warn "проверка состояния" rather than claim that
    /// listening is off.
    pub const fn is_degraded(self) -> bool {
        matches!(
            self,
            ListeningState::DeviceConflict
                | ListeningState::DeviceDisconnected
                | ListeningState::EngineUnavailable
        )
    }

    /// Allowed successors.  Self-transitions are not allowed: a repeated state
    /// is not a change and must not publish `ambient.state`.
    pub const fn allowed_next(self) -> &'static [ListeningState] {
        use ListeningState::*;
        match self {
            Stopped => &[Starting, Denied],
            Starting => &[
                Listening,
                Stopped,
                PausedByPolicy,
                DeviceConflict,
                DeviceDisconnected,
                EngineUnavailable,
                Denied,
            ],
            Listening => &[
                Stopped,
                PausedByUser,
                PausedByPolicy,
                DeviceConflict,
                DeviceDisconnected,
                EngineUnavailable,
                Denied,
            ],
            PausedByUser => &[Starting, Stopped, PausedByPolicy, Denied],
            PausedByPolicy => &[Starting, Stopped, PausedByUser, Denied],
            DeviceConflict => &[Starting, Stopped, DeviceDisconnected, Denied],
            DeviceDisconnected => &[Starting, Stopped, DeviceConflict, Denied],
            EngineUnavailable => &[Starting, Stopped, Denied],
            Denied => &[Stopped],
        }
    }

    pub fn can_transition(self, next: ListeningState) -> bool {
        self.allowed_next().contains(&next)
    }

    pub fn transition(self, next: ListeningState) -> Result<ListeningState, ContractError> {
        if self.can_transition(next) {
            Ok(next)
        } else {
            Err(ContractError::InvalidTransition {
                from: self,
                to: next,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pair_is_decided_and_self_transitions_are_rejected() {
        for from in ListeningState::ALL {
            for to in ListeningState::ALL {
                let allowed = from.can_transition(to);
                if from == to {
                    assert!(!allowed, "self-transition allowed for {from:?}");
                }
                assert_eq!(allowed, from.transition(to).is_ok());
                if !allowed {
                    assert_eq!(
                        from.transition(to),
                        Err(ContractError::InvalidTransition { from, to })
                    );
                }
            }
        }
    }

    #[test]
    fn denied_only_falls_back_to_stopped() {
        assert_eq!(
            ListeningState::Denied.allowed_next(),
            &[ListeningState::Stopped]
        );
        for to in ListeningState::ALL {
            if to != ListeningState::Stopped {
                assert!(!ListeningState::Denied.can_transition(to));
            }
        }
    }

    #[test]
    fn listening_is_reachable_only_through_starting() {
        for from in ListeningState::ALL {
            if from == ListeningState::Starting {
                continue;
            }
            assert!(
                !from.can_transition(ListeningState::Listening),
                "{from:?} reaches Listening without Starting"
            );
        }
    }

    #[test]
    fn any_live_state_can_be_denied() {
        for from in ListeningState::ALL {
            if from == ListeningState::Denied {
                continue;
            }
            assert!(from.can_transition(ListeningState::Denied));
        }
    }

    #[test]
    fn only_listening_captures() {
        for state in ListeningState::ALL {
            assert_eq!(state.is_capturing(), state == ListeningState::Listening);
        }
    }
}
