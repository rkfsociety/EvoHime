//! API key encryption and secure storage (Phase 5.7).
//!
//! Encrypts model API keys in `app_settings` using AES-GCM with a derived key.
//! Key derivation: SHA256(workspace_root + VERSION) ensures different keys per deployment.
//! Empty keys are not encrypted (already safe).
//!
//! TODO: production should use OS keychain (Windows DPAPI / macOS Keychain / Linux Secret Service)

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use base64::engine::general_purpose::STANDARD as BASE64_ENGINE;
use base64::Engine;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const ENCRYPTION_VERSION: &[u8] = b"v1"; // bump if key derivation changes
const NONCE_SIZE: usize = 12; // 96 bits for AES-GCM

/// Derive a stable 256-bit encryption key from workspace root.
/// Returns the same key for the same workspace, allowing decryption of stored keys.
pub fn derive_key(workspace_root: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(workspace_root.as_bytes());
    hasher.update(ENCRYPTION_VERSION);
    let mut key = [0u8; 32];
    key.copy_from_slice(&hasher.finalize()[..]);
    key
}

/// Encrypt an API key using AES-256-GCM.
/// Returns base64(version + nonce + ciphertext) for safe storage.
/// Empty or whitespace keys are returned as-is (not encrypted).
pub fn encrypt_api_key(key: &str, workspace_root: &str) -> String {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let encryption_key = derive_key(workspace_root);
    let cipher = Aes256Gcm::new_from_slice(&encryption_key).expect("valid key");

    // Generate random 96-bit nonce from UUID
    let nonce_bytes = Uuid::new_v4().as_bytes()[..NONCE_SIZE].to_vec();
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt
    let ciphertext = cipher
        .encrypt(nonce, Payload::from(trimmed.as_bytes()))
        .expect("encryption failed");

    // Format: version (1 byte) + nonce (12 bytes) + ciphertext
    let mut encrypted = vec![1u8]; // version
    encrypted.extend_from_slice(&nonce_bytes);
    encrypted.extend_from_slice(&ciphertext);

    // Return base64-encoded
    BASE64_ENGINE.encode(&encrypted)
}

/// Decrypt an API key encrypted by `encrypt_api_key`.
/// Returns the original key, or empty string if decryption fails.
pub fn decrypt_api_key(encrypted: &str, workspace_root: &str) -> String {
    let encrypted_bytes = match BASE64_ENGINE.decode(encrypted) {
        Ok(b) => b,
        Err(_) => return String::new(),
    };

    if encrypted_bytes.is_empty() || encrypted_bytes[0] != 1 {
        // Not encrypted or unknown version
        return String::new();
    }

    if encrypted_bytes.len() < 1 + NONCE_SIZE {
        return String::new();
    }

    let encryption_key = derive_key(workspace_root);
    let cipher = Aes256Gcm::new_from_slice(&encryption_key).expect("valid key");

    let nonce = Nonce::from_slice(&encrypted_bytes[1..1 + NONCE_SIZE]);
    let ciphertext = &encrypted_bytes[1 + NONCE_SIZE..];

    match cipher.decrypt(nonce, Payload::from(ciphertext)) {
        Ok(plaintext) => String::from_utf8_lossy(&plaintext).to_string(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let workspace = "/home/user/project";
        let original = "sk_test_abc123";

        let encrypted = encrypt_api_key(original, workspace);
        assert!(!encrypted.is_empty());
        assert_ne!(encrypted, original);

        let decrypted = decrypt_api_key(&encrypted, workspace);
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_empty_key_not_encrypted() {
        let workspace = "/home/user/project";
        let encrypted = encrypt_api_key("", workspace);
        assert_eq!(encrypted, "");

        let encrypted2 = encrypt_api_key("  ", workspace);
        assert_eq!(encrypted2, "");
    }

    #[test]
    fn test_wrong_workspace_fails_decrypt() {
        let workspace1 = "/home/user/project1";
        let workspace2 = "/home/user/project2";
        let original = "sk_test_abc123";

        let encrypted = encrypt_api_key(original, workspace1);
        let decrypted = decrypt_api_key(&encrypted, workspace2);

        // Wrong workspace key should produce garbage or fail
        assert_ne!(decrypted, original);
    }

    #[test]
    fn test_corrupted_encryption_fails_gracefully() {
        let workspace = "/home/user/project";
        let corrupted = "invalid_base64!@#";

        let decrypted = decrypt_api_key(corrupted, workspace);
        assert_eq!(decrypted, "");
    }
}
