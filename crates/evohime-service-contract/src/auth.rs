//! HMAC-SHA256 channel authentication between `evohime-core` and isolated
//! service modules.
//!
//! Core generates one random session secret per spawned child process and
//! passes it through the [`SERVICE_SESSION_SECRET_ENV`] environment variable.
//! Before the first real request the child must answer an `Authenticate`
//! request carrying a fresh nonce and `HMAC-SHA256(secret, nonce)` as proof.
//! The derivation mirrors `crates/desktop-ipc/src/session.rs`: a manual
//! HMAC-SHA256 over `sha2` with constant-time proof comparison.

use sha2::{Digest, Sha256};

use crate::{ServiceContractError, ServiceErrorCode};

/// Environment variable through which Core passes the per-child session secret
/// to a service module process.
pub const SERVICE_SESSION_SECRET_ENV: &str = "EVOHIME_SERVICE_SESSION_SECRET";

/// Length in bytes of the per-child shared session secret.
pub const SESSION_SECRET_BYTES: usize = 32;

/// Length in bytes of each single-use handshake nonce.
pub const HANDSHAKE_NONCE_BYTES: usize = 32;

/// Number of hexadecimal characters in a serialized 32-byte value.
const HEX_BYTES_32: usize = SESSION_SECRET_BYTES * 2;

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_lowercase_hex_of_len(value: &str, characters: usize) -> bool {
    value.len() == characters
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_hex_of_len(value: &str, characters: usize) -> bool {
    value.len() == characters && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Generates a cryptographically random session secret as 64 lowercase hex
/// characters (32 random bytes). The value must reach the child process only
/// through [`SERVICE_SESSION_SECRET_ENV`] and never through logs or arguments.
pub fn generate_session_secret() -> Result<String, ServiceContractError> {
    random_hex(SESSION_SECRET_BYTES)
}

/// Generates a fresh single-use handshake nonce as 64 lowercase hex characters
/// (32 random bytes).
pub fn generate_handshake_nonce() -> Result<String, ServiceContractError> {
    random_hex(HANDSHAKE_NONCE_BYTES)
}

fn random_hex(bytes: usize) -> Result<String, ServiceContractError> {
    let mut buffer = vec![0_u8; bytes];
    if getrandom::fill(&mut buffer).is_err() {
        return Err(ServiceContractError::new(
            ServiceErrorCode::Internal,
            "secure random generation failed",
        ));
    }
    Ok(hex_encode(&buffer))
}

/// Computes the lowercase hex HMAC-SHA256 proof that authenticates one
/// handshake: `hex(HMAC-SHA256(secret_bytes, nonce_bytes))`.
///
/// The known-answer example reuses RFC 4231 test case 2 and is a test vector,
/// not a production secret.
///
/// ```
/// use evohime_service_contract::compute_service_proof;
/// assert_eq!(
///     compute_service_proof("Jefe", "what do ya want for nothing?"),
///     "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
/// );
/// ```
pub fn compute_service_proof(secret: &str, nonce: &str) -> String {
    hex_encode(&hmac_sha256(secret.as_bytes(), nonce.as_bytes()))
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut block = [0_u8; BLOCK];
    if key.len() > BLOCK {
        let digest = Sha256::digest(key);
        block[..32].copy_from_slice(&digest);
    } else {
        block[..key.len()].copy_from_slice(key);
    }

    let mut inner_key = [0_u8; BLOCK];
    let mut outer_key = [0_u8; BLOCK];
    for index in 0..BLOCK {
        inner_key[index] = block[index] ^ 0x36;
        outer_key[index] = block[index] ^ 0x5c;
    }

    let mut inner = Sha256::new();
    inner.update(inner_key);
    inner.update(message);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_key);
    outer.update(inner_digest);
    outer.finalize().into()
}

/// Compares two ASCII values without an early exit on the first difference.
pub(crate) fn constant_time_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (a, b) in left.bytes().zip(right.bytes()) {
        difference |= a ^ b;
    }
    difference == 0
}

/// Verifies one `Authenticate` handshake against the shared session secret.
///
/// The nonce must be lowercase hex of exactly [`HANDSHAKE_NONCE_BYTES`] bytes,
/// the proof must be hex of the same character length, and the proof is
/// compared in constant time. Every rejection is reported as a uniform
/// [`ServiceErrorCode::PermissionDenied`] error whose message never reveals the
/// expected secret or which check failed.
pub(crate) fn verify_handshake(
    secret: &str,
    nonce: &str,
    proof: &str,
) -> Result<(), ServiceContractError> {
    if !is_lowercase_hex_of_len(nonce, HEX_BYTES_32) {
        return Err(ServiceContractError::permission_denied(
            "authentication handshake rejected",
        ));
    }
    if !is_hex_of_len(proof, HEX_BYTES_32) {
        return Err(ServiceContractError::permission_denied(
            "authentication handshake rejected",
        ));
    }
    let expected = compute_service_proof(secret, nonce);
    if !constant_time_eq(proof, &expected) {
        return Err(ServiceContractError::permission_denied(
            "authentication handshake rejected",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_a_published_rfc4231_vector() {
        // RFC 4231 test case 1: key = 0x0b * 20, data = "Hi There".
        let digest = hmac_sha256(&[0x0b; 20], b"Hi There");
        assert_eq!(
            hex_encode(&digest),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn service_proof_matches_rfc4231_case_2() {
        // RFC 4231 test case 2: key = "Jefe", data = "what do ya want for nothing?".
        assert_eq!(
            compute_service_proof("Jefe", "what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn generated_secret_and_nonce_are_well_formed_and_unique() {
        let first_secret = generate_session_secret().expect("secret");
        let second_secret = generate_session_secret().expect("secret");
        assert_eq!(first_secret.len(), HEX_BYTES_32);
        assert!(is_lowercase_hex_of_len(&first_secret, HEX_BYTES_32));
        assert_ne!(first_secret, second_secret);

        let first_nonce = generate_handshake_nonce().expect("nonce");
        let second_nonce = generate_handshake_nonce().expect("nonce");
        assert!(is_lowercase_hex_of_len(&first_nonce, HEX_BYTES_32));
        assert_ne!(first_nonce, second_nonce);
    }

    #[test]
    fn verify_handshake_accepts_only_the_correct_proof() {
        let secret = generate_session_secret().expect("secret");
        let nonce = generate_handshake_nonce().expect("nonce");
        let proof = compute_service_proof(&secret, &nonce);
        assert!(verify_handshake(&secret, &nonce, &proof).is_ok());

        let wrong = compute_service_proof("wrong-secret", &nonce);
        assert_eq!(
            verify_handshake(&secret, &nonce, &wrong)
                .expect_err("wrong proof must be rejected")
                .code,
            ServiceErrorCode::PermissionDenied
        );
    }

    #[test]
    fn verify_handshake_rejects_malformed_nonce_and_proof() {
        let secret = generate_session_secret().expect("secret");
        let nonce = generate_handshake_nonce().expect("nonce");
        let proof = compute_service_proof(&secret, &nonce);

        for bad_nonce in ["", "zz", "AB".repeat(32).as_str(), &nonce[..62]] {
            assert_eq!(
                verify_handshake(&secret, bad_nonce, &proof)
                    .expect_err("malformed nonce must be rejected")
                    .code,
                ServiceErrorCode::PermissionDenied
            );
        }
        for bad_proof in ["", "zz", "AB".repeat(32).as_str(), &proof[..62]] {
            assert_eq!(
                verify_handshake(&secret, &nonce, bad_proof)
                    .expect_err("malformed proof must be rejected")
                    .code,
                ServiceErrorCode::PermissionDenied
            );
        }
    }
}
