//! Самостоятельный data-contract для offline research evidence.
//!
//! Модуль намеренно не подключён к `lib.rs`: его можно интегрировать в Core
//! отдельным изменением, когда будут готовы IPC и storage-контуры research.
//! Все размеры ограничены до сериализации, а JSON получается из структуры с
//! фиксированным порядком полей.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_URL_CHARS: usize = 2_048;
pub const MAX_TITLE_CHARS: usize = 256;
pub const MAX_PUBLISHER_CHARS: usize = 256;
pub const MAX_CONTENT_TYPE_CHARS: usize = 128;
pub const MAX_EXCERPT_CHARS: usize = 4_096;
pub const MAX_TTL_MS: u64 = 31 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    EmptyField(&'static str),
    FieldTooLong { field: &'static str, max: usize },
    InvalidTimestamp,
    InvalidTtl,
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::InvalidTimestamp => write!(f, "timestamp must be a positive Unix timestamp"),
            Self::InvalidTtl => write!(f, "ttl must be between 1 ms and {MAX_TTL_MS} ms"),
        }
    }
}

impl std::error::Error for ContractError {}

/// Bounded, non-secret metadata identifying the origin of evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceMetadata {
    pub url: String,
    pub title: String,
    pub publisher: String,
    pub content_type: String,
    pub retrieved_at_ms: u64,
}

impl SourceMetadata {
    pub fn new(
        url: impl Into<String>,
        title: impl Into<String>,
        publisher: impl Into<String>,
        content_type: impl Into<String>,
        retrieved_at_ms: u64,
    ) -> Result<Self, ContractError> {
        let value = Self {
            url: url.into(),
            title: title.into(),
            publisher: publisher.into(),
            content_type: content_type.into(),
            retrieved_at_ms,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), ContractError> {
        validate_text("url", &self.url, MAX_URL_CHARS, true)?;
        validate_text("title", &self.title, MAX_TITLE_CHARS, true)?;
        validate_text("publisher", &self.publisher, MAX_PUBLISHER_CHARS, true)?;
        validate_text(
            "content_type",
            &self.content_type,
            MAX_CONTENT_TYPE_CHARS,
            true,
        )?;
        if self.retrieved_at_ms == 0 {
            return Err(ContractError::InvalidTimestamp);
        }
        Ok(())
    }
}

/// Immutable evidence captured offline. `excerpt` is already redacted and
/// `excerpt_sha256` hashes exactly those redacted UTF-8 bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchEvidence {
    pub source: SourceMetadata,
    pub excerpt: String,
    pub excerpt_sha256: String,
    pub captured_at_ms: u64,
    pub ttl_ms: u64,
}

impl ResearchEvidence {
    pub fn capture(
        source: SourceMetadata,
        excerpt: impl AsRef<str>,
        captured_at_ms: u64,
        ttl_ms: u64,
    ) -> Result<Self, ContractError> {
        if captured_at_ms == 0 {
            return Err(ContractError::InvalidTimestamp);
        }
        if !(1..=MAX_TTL_MS).contains(&ttl_ms) {
            return Err(ContractError::InvalidTtl);
        }
        let excerpt = redact_excerpt(excerpt.as_ref())?;
        let excerpt_sha256 = sha256_hex(excerpt.as_bytes());
        Ok(Self {
            source,
            excerpt,
            excerpt_sha256,
            captured_at_ms,
            ttl_ms,
        })
    }

    pub fn expires_at_ms(&self) -> Option<u64> {
        self.captured_at_ms.checked_add(self.ttl_ms)
    }

    pub fn is_fresh_at(&self, now_ms: u64) -> bool {
        now_ms >= self.captured_at_ms && now_ms < self.expires_at_ms().unwrap_or(u64::MAX)
    }

    /// Stable compact JSON suitable for hashing, storage, or IPC fixtures.
    pub fn to_deterministic_json(&self) -> String {
        serde_json::to_string(self).expect("ResearchEvidence is serializable")
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
    required: bool,
) -> Result<(), ContractError> {
    if required && value.trim().is_empty() {
        return Err(ContractError::EmptyField(field));
    }
    if value.chars().count() > max {
        return Err(ContractError::FieldTooLong { field, max });
    }
    Ok(())
}

/// Redacts common bearer/API-token forms and bounds the result by Unicode
/// scalar values, preserving valid UTF-8 and making the transformation stable.
pub fn redact_excerpt(input: &str) -> Result<String, ContractError> {
    if input.chars().count() > MAX_EXCERPT_CHARS {
        return Err(ContractError::FieldTooLong {
            field: "excerpt",
            max: MAX_EXCERPT_CHARS,
        });
    }
    let mut output = Vec::new();
    for token in input.split_inclusive(char::is_whitespace) {
        let trimmed = token.trim_end_matches(char::is_whitespace);
        let suffix = &token[trimmed.len()..];
        let redacted = if is_secret_token(trimmed) {
            "[REDACTED]"
        } else {
            trimmed
        };
        output.push(redacted);
        output.push(suffix);
    }
    let redacted = output.concat();
    Ok(redacted)
}

fn is_secret_token(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("AIza")
        || (token.contains('@') && token.split('@').next().is_some_and(|part| !part.is_empty()))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = sha256(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

// Small dependency-free SHA-256 implementation for the contract's content hash.
fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in message.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in chunk.as_chunks::<4>().0.iter().take(16).enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            (hh, g, f, e, d, c, b, a) = (
                g,
                f,
                e,
                d.wrapping_add(temp1),
                c,
                b,
                a,
                temp1.wrapping_add(temp2),
            );
        }
        for (value, add) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *value = (*value).wrapping_add(add);
        }
    }
    let mut out = [0u8; 32];
    for (i, value) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&value.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SourceMetadata {
        SourceMetadata::new(
            "https://example.test/a",
            "Example",
            "Example Org",
            "text/html",
            1_700_000_000_000,
        )
        .unwrap()
    }

    #[test]
    fn captures_redacted_excerpt_and_sha256() {
        let evidence = ResearchEvidence::capture(
            source(),
            "Useful text sk-secret alice@example.test",
            2_000,
            1_000,
        )
        .unwrap();
        assert_eq!(evidence.excerpt, "Useful text [REDACTED] [REDACTED]");
        assert_eq!(
            evidence.excerpt_sha256,
            "35a7d5361181b0e82bb8016999917970bcf66e41a34b36f76451f41f559d1b36"
        );
        assert!(evidence.is_fresh_at(2_999));
        assert!(!evidence.is_fresh_at(3_000));
    }

    #[test]
    fn deterministic_json_is_stable_and_round_trips() {
        let evidence = ResearchEvidence::capture(source(), "same", 2_000, 1_000).unwrap();
        let json = evidence.to_deterministic_json();
        assert_eq!(json, evidence.to_deterministic_json());
        assert!(json.starts_with("{\"source\":{\"url\":\"https://example.test/a\""));
        assert_eq!(
            serde_json::from_str::<ResearchEvidence>(&json).unwrap(),
            evidence
        );
    }

    #[test]
    fn bounds_metadata_excerpt_and_ttl() {
        assert!(matches!(
            SourceMetadata::new("", "title", "publisher", "text/plain", 1),
            Err(ContractError::EmptyField("url"))
        ));
        assert!(matches!(
            ResearchEvidence::capture(source(), "x", 1, 0),
            Err(ContractError::InvalidTtl)
        ));
        assert!(matches!(
            ResearchEvidence::capture(source(), "x".repeat(MAX_EXCERPT_CHARS + 1), 1, 1),
            Err(ContractError::FieldTooLong {
                field: "excerpt",
                ..
            })
        ));
    }

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
