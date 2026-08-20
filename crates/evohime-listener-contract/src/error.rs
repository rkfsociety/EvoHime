use crate::state::ListeningState;
use serde::{Deserialize, Serialize};

/// Closed set of error codes crossing the listener/Core/renderer boundary.
///
/// The renderer maps a known code to a localized string; an unknown code is
/// shown as a generic listener failure and is never treated as success.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbientErrorCode {
    ListenerUnavailable,
    DeviceConflict,
    DeviceDisconnected,
    PermissionDenied,
    PolicyInvalid,
    EngineNotReady,
    StorageFailed,
    ConfirmationRequired,
    InvalidArgument,
}

impl AmbientErrorCode {
    pub const ALL: [AmbientErrorCode; 9] = [
        AmbientErrorCode::ListenerUnavailable,
        AmbientErrorCode::DeviceConflict,
        AmbientErrorCode::DeviceDisconnected,
        AmbientErrorCode::PermissionDenied,
        AmbientErrorCode::PolicyInvalid,
        AmbientErrorCode::EngineNotReady,
        AmbientErrorCode::StorageFailed,
        AmbientErrorCode::ConfirmationRequired,
        AmbientErrorCode::InvalidArgument,
    ];

    /// Wire form used by IPC results and by the renderer.
    pub const fn as_str(self) -> &'static str {
        match self {
            AmbientErrorCode::ListenerUnavailable => "LISTENER_UNAVAILABLE",
            AmbientErrorCode::DeviceConflict => "DEVICE_CONFLICT",
            AmbientErrorCode::DeviceDisconnected => "DEVICE_DISCONNECTED",
            AmbientErrorCode::PermissionDenied => "PERMISSION_DENIED",
            AmbientErrorCode::PolicyInvalid => "POLICY_INVALID",
            AmbientErrorCode::EngineNotReady => "ENGINE_NOT_READY",
            AmbientErrorCode::StorageFailed => "STORAGE_FAILED",
            AmbientErrorCode::ConfirmationRequired => "CONFIRMATION_REQUIRED",
            AmbientErrorCode::InvalidArgument => "INVALID_ARGUMENT",
        }
    }

    /// Parses a wire code; an unknown string stays unknown instead of being
    /// coerced into a neighbouring meaning.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|code| code.as_str() == value)
    }
}

impl std::fmt::Display for AmbientErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Validation and state-machine failures raised inside the contract itself.
///
/// No variant carries caller-supplied text: field names are `&'static str`
/// chosen by this crate, so an error message cannot become a content leak.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractError {
    InvalidTransition {
        from: ListeningState,
        to: ListeningState,
    },
    EmptyField(&'static str),
    FieldTooLong(&'static str),
    InvalidCharacter(&'static str),
    TooManyEntries(&'static str),
    PatternTooComplex(&'static str),
    InvalidQuietHours,
    RetentionOutOfBounds,
    LimitsOutOfBounds(&'static str),
    BudgetOutOfBounds(&'static str),
}

impl ContractError {
    /// Code reported to the renderer for this failure.
    pub const fn code(self) -> AmbientErrorCode {
        match self {
            ContractError::InvalidTransition { .. } => AmbientErrorCode::InvalidArgument,
            ContractError::EmptyField(_)
            | ContractError::FieldTooLong(_)
            | ContractError::InvalidCharacter(_) => AmbientErrorCode::InvalidArgument,
            ContractError::TooManyEntries(_)
            | ContractError::PatternTooComplex(_)
            | ContractError::InvalidQuietHours
            | ContractError::RetentionOutOfBounds => AmbientErrorCode::PolicyInvalid,
            ContractError::LimitsOutOfBounds(_) | ContractError::BudgetOutOfBounds(_) => {
                AmbientErrorCode::InvalidArgument
            }
        }
    }
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContractError::InvalidTransition { from, to } => {
                write!(f, "invalid listening transition {from:?} -> {to:?}")
            }
            ContractError::EmptyField(field) => write!(f, "empty field: {field}"),
            ContractError::FieldTooLong(field) => write!(f, "field too long: {field}"),
            ContractError::InvalidCharacter(field) => {
                write!(f, "field contains a forbidden character: {field}")
            }
            ContractError::TooManyEntries(field) => write!(f, "too many entries: {field}"),
            ContractError::PatternTooComplex(field) => write!(f, "pattern too complex: {field}"),
            ContractError::InvalidQuietHours => f.write_str("invalid quiet hours window"),
            ContractError::RetentionOutOfBounds => f.write_str("retention days out of bounds"),
            ContractError::LimitsOutOfBounds(field) => write!(f, "limit out of bounds: {field}"),
            ContractError::BudgetOutOfBounds(field) => write!(f, "budget out of bounds: {field}"),
        }
    }
}

impl std::error::Error for ContractError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_codes_round_trip_and_are_unique() {
        let mut seen = Vec::new();
        for code in AmbientErrorCode::ALL {
            assert_eq!(AmbientErrorCode::parse(code.as_str()), Some(code));
            assert!(!seen.contains(&code.as_str()));
            seen.push(code.as_str());
        }
        assert_eq!(AmbientErrorCode::parse("microphone_on_fire"), None);
    }

    #[test]
    fn policy_failures_report_policy_invalid() {
        assert_eq!(
            ContractError::RetentionOutOfBounds.code(),
            AmbientErrorCode::PolicyInvalid
        );
        assert_eq!(
            ContractError::TooManyEntries("process_blocklist").code(),
            AmbientErrorCode::PolicyInvalid
        );
    }
}
