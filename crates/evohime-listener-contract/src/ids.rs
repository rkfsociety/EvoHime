use crate::error::ContractError;
use serde::{Deserialize, Serialize};

pub const MAX_ID_BYTES: usize = 128;

/// Identifiers are bounded opaque tokens, never sentences.
///
/// Every ambient identifier goes through this charset: ASCII alphanumerics
/// plus `-`, `_`, `.`, `:` and `+`.  Spaces are excluded on purpose — the
/// cheapest way to leak speech through a "metadata-only" event is an `id`
/// field that happily accepts a phrase.
fn validate_id(value: &str, field: &'static str) -> Result<(), ContractError> {
    if value.is_empty() {
        return Err(ContractError::EmptyField(field));
    }
    if value.len() > MAX_ID_BYTES {
        return Err(ContractError::FieldTooLong(field));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '+'))
    {
        return Err(ContractError::InvalidCharacter(field));
    }
    Ok(())
}

macro_rules! bounded_id {
    ($name:ident, $field:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, ContractError> {
                let value = value.into();
                validate_id(&value, $field)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = ContractError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

bounded_id!(
    DeviceId,
    "device_id",
    "Opaque capture-device identifier reported by the audio backend."
);
bounded_id!(
    EpisodeId,
    "episode_id",
    "Identifier of one ambient episode row."
);
bounded_id!(
    ProposalId,
    "proposal_id",
    "Identifier of one bounded proactive proposal."
);
bounded_id!(
    SubjectKey,
    "subject_key",
    "Bounded, opaque key of one proposal subject.

It is a token, not a phrase: the charset excludes spaces, so a canonical
subject reaches an event only after Core has reduced it to a slug (or, when
nothing ASCII survives, to a short fingerprint). The card's human-readable
text never travels this way — it is read back with a command."
);
bounded_id!(
    CommandId,
    "command_id",
    "Identifier of one heard voice command awaiting a decision."
);
bounded_id!(
    AppId,
    "app_id",
    "Bounded key of one application in the launch catalog.

It is a catalog key, not a name the user said: Core resolves speech to an
entry first, and only the entry's key reaches an event. The human-readable
title travels the same way as a proposal's text — it is read back with a
command, never through the log."
);
bounded_id!(
    EngineVersion,
    "engine_version",
    "Speech-engine build identifier, e.g. `whisper-base-q5_1`."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_accept_opaque_tokens() {
        assert_eq!(
            EpisodeId::new("01J8-episode.7").unwrap().as_str(),
            "01J8-episode.7"
        );
        assert_eq!(
            EngineVersion::new("whisper-base-q5_1").unwrap().to_string(),
            "whisper-base-q5_1"
        );
    }

    #[test]
    fn ids_reject_free_text() {
        assert_eq!(
            DeviceId::new("позвони маме завтра"),
            Err(ContractError::InvalidCharacter("device_id"))
        );
        assert_eq!(
            ProposalId::new(""),
            Err(ContractError::EmptyField("proposal_id"))
        );
        assert_eq!(
            EpisodeId::new("a".repeat(MAX_ID_BYTES + 1)),
            Err(ContractError::FieldTooLong("episode_id"))
        );
        assert_eq!(
            SubjectKey::new("купить хлеб"),
            Err(ContractError::InvalidCharacter("subject_key"))
        );
        assert_eq!(
            DeviceId::new("mic 1"),
            Err(ContractError::InvalidCharacter("device_id"))
        );
    }

    #[test]
    fn deserialization_validates_too() {
        let parsed: Result<EpisodeId, _> = serde_json::from_str("\"ok-1\"");
        assert!(parsed.is_ok());
        let rejected: Result<EpisodeId, _> = serde_json::from_str("\"я сказал вслух\"");
        assert!(rejected.is_err());
    }
}
