use crate::runtime::{ProtectedActionRow, RuntimeError, MAX_PROTECTED_ROW_BYTES};
use crate::{canonicalize_json, ReceiptError};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(crate) fn valid_recovery_code(value: &str) -> bool {
    matches!(value, "signature_failed" | "external_error" | "unknown")
}

/// Encrypts the bounded recovery projection. The nonce and GCM tag are part
/// of the 512-byte envelope; truncation is never permitted.
pub fn protect_action_row(
    row: &ProtectedActionRow,
    key: &[u8; 32],
) -> Result<Vec<u8>, RuntimeError> {
    if row.schema_version != 1
        || !valid_recovery_code(&row.recovery_code)
        || !matches!(
            row.result_status.as_str(),
            "succeeded" | "failed" | "cancelled"
        )
    {
        return Err(RuntimeError::Code("schema_violation"));
    }
    let plaintext =
        canonicalize_json(&serde_json::to_vec(row).map_err(|_| ReceiptError::InvalidJson)?)?;
    if plaintext.len() + 28 > MAX_PROTECTED_ROW_BYTES {
        return Err(RuntimeError::Code("pending_recovery"));
    }
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, key)
        .map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
    let key = aead::LessSafeKey::new(unbound);
    let mut nonce = [0u8; 12];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
    let nonce_value = aead::Nonce::assume_unique_for_key(nonce);
    let mut ciphertext = plaintext;
    key.seal_in_place_append_tag(nonce_value, aead::Aad::empty(), &mut ciphertext)
        .map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
    let mut output = nonce.to_vec();
    output.extend_from_slice(&ciphertext);
    if output.len() > MAX_PROTECTED_ROW_BYTES {
        return Err(RuntimeError::Code("pending_recovery"));
    }
    Ok(output)
}

/// Authenticates and decodes a protected recovery projection.
pub fn unprotect_action_row(
    envelope: &[u8],
    key: &[u8; 32],
) -> Result<ProtectedActionRow, RuntimeError> {
    if envelope.len() < 28 || envelope.len() > MAX_PROTECTED_ROW_BYTES {
        return Err(RuntimeError::Code("pending_recovery"));
    }
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&envelope[..12]);
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, key)
        .map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
    let key = aead::LessSafeKey::new(unbound);
    let mut ciphertext = envelope[12..].to_vec();
    let plaintext = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::empty(),
            &mut ciphertext,
        )
        .map_err(|_| RuntimeError::Code("pending_recovery"))?;
    let value: Value =
        serde_json::from_slice(plaintext).map_err(|_| RuntimeError::Code("pending_recovery"))?;
    let row: ProtectedActionRow =
        serde_json::from_value(value).map_err(|_| RuntimeError::Code("pending_recovery"))?;
    if row.schema_version != 1 || !valid_recovery_code(&row.recovery_code) {
        return Err(RuntimeError::Code("pending_recovery"));
    }
    Ok(row)
}

/// Selects a deterministic sample of read-only actions using the bounded percentage rate.
pub fn sampled_read_only(action_id: &str, tool_name: &str, rate: u8) -> bool {
    if rate == 0 {
        return false;
    }
    let mut bytes = b"evohime-sample-v1\0".to_vec();
    bytes.extend_from_slice(action_id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(tool_name.as_bytes());
    let digest = Sha256::digest(bytes);
    u16::from_be_bytes([digest[0], digest[1]]) % 100 < rate as u16
}

/// Encodes a size-limited result status and digest without persisting output contents.
pub fn bounded_result_marker(
    status: &str,
    hash: &str,
    error_category: Option<&str>,
    returned_at_ms: i64,
    output_present: bool,
) -> Result<Vec<u8>, RuntimeError> {
    if !matches!(status, "succeeded" | "failed" | "cancelled")
        || hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(RuntimeError::Code("schema_violation"));
    }
    let value = json!({"schema_version":1,"result_status":status,"result_hash":hash,"error_category":error_category,"returned_at_ms":returned_at_ms,"output_present":output_present});
    let bytes =
        canonicalize_json(&serde_json::to_vec(&value).map_err(|_| ReceiptError::InvalidJson)?)?;
    if bytes.len() > 256 {
        return Err(RuntimeError::Code("pending_recovery"));
    }
    Ok(bytes)
}
