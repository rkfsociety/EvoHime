//! Receipt key lifecycle for stage 01.2.
//!
//! The module owns the receipt key material and transition chain.  Callers get
//! public metadata or signatures only; protected key bytes never cross this
//! API boundary.

use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
use chrono::Utc;
use ring::{
    rand::SystemRandom,
    signature::{self, Ed25519KeyPair, KeyPair},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

pub const KEY_DIR: &str = "receipts/keys";
pub const ACTIVE_KEY_FILE: &str = "active-key-v1.json";
pub const HISTORY_FILE: &str = "public-history-v1.jsonl";
pub const TRUST_FILE: &str = "trusted-roots-v1.json";
pub const JOURNAL_FILE: &str = "rotation-state-v1.json";
pub const MAX_TRANSITIONS: usize = 100;
pub const MAX_HISTORY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum KeyError {
    #[error("key.not_initialized")]
    NotInitialized,
    #[error("key.dpapi_failed")]
    DpapiFailed,
    #[error("key.dacl_invalid")]
    DaclInvalid,
    #[error("key.corrupt")]
    Corrupt,
    #[error("key.public_mismatch")]
    PublicMismatch,
    #[error("key.rotation_incomplete")]
    RotationIncomplete,
    #[error("key.rotation_fork")]
    RotationFork,
    #[error("key.cleanup_required")]
    CleanupRequired,
    #[error("key.trust_required")]
    TrustRequired,
    #[error("key.rotation_limit")]
    RotationLimit,
    #[error("key.history_incomplete")]
    HistoryIncomplete,
    #[error("key.invalid_transition")]
    InvalidTransition,
    #[error("key.unsupported_platform")]
    UnsupportedPlatform,
    #[error("I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("signature failed")]
    Signature,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveKeyMetadata {
    pub storage_version: u8,
    pub key_id: String,
    pub public_key: String,
    pub created_at: String,
    pub protected_pkcs8: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyTransition {
    pub transition_version: u8,
    pub transition_id: String,
    pub created_at: String,
    pub reason: String,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_key_id: Option<String>,
    pub new_key_id: String,
    pub new_public_key: String,
    pub continuity: String,
    pub signed_by_key_id: String,
    pub signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_transition_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RotationState {
    pub state_version: u8,
    pub rotation_id: String,
    pub phase: String,
    pub old_key_id: String,
    pub new_key_id: String,
    pub transition_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub reason: String,
    pub actor: String,
    pub active_key_observed: bool,
    pub audit_event_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedRoot {
    pub root_version: u8,
    pub root_id: String,
    pub genesis_key_id: String,
    pub pinned_at: String,
    pub source: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedRootsFile {
    pub schema_version: u8,
    pub roots: Vec<TrustedRoot>,
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}
fn decode_b64(value: &str) -> Result<Vec<u8>, KeyError> {
    URL_SAFE_NO_PAD.decode(value).map_err(|_| KeyError::Corrupt)
}
pub fn key_id(public_key: &[u8]) -> String {
    format!("ed25519:{:x}", Sha256::digest(public_key))
}
pub fn transition_hash(transition: &KeyTransition) -> Result<String, KeyError> {
    Ok(sha256_hex(&canonical_json(transition)?))
}
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Canonical bytes for lifecycle objects. Struct declaration order is the
/// normative schema order; serde rejects no unknown fields when deserializing
/// externally, so the verifier performs the bounded field checks below.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, KeyError> {
    Ok(serde_json::to_vec(value)?)
}
fn signed_bytes(transition: &KeyTransition) -> Result<Vec<u8>, KeyError> {
    let mut value = serde_json::to_value(transition)?;
    value
        .as_object_mut()
        .ok_or(KeyError::InvalidTransition)?
        .remove("signature");
    Ok(serde_json::to_vec(&value)?)
}

pub struct SecretSigner(Ed25519KeyPair);
impl SecretSigner {
    fn generate() -> Result<(Self, Vec<u8>), KeyError> {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .map_err(|_| KeyError::DpapiFailed)?;
        let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| KeyError::Corrupt)?;
        Ok((Self(key), pkcs8.as_ref().to_vec()))
    }
    fn from_pkcs8(bytes: &[u8]) -> Result<Self, KeyError> {
        Ok(Self(
            Ed25519KeyPair::from_pkcs8(bytes).map_err(|_| KeyError::Corrupt)?,
        ))
    }
    fn sign(&self, bytes: &[u8]) -> String {
        b64(self.0.sign(bytes).as_ref())
    }
    fn public(&self) -> &[u8] {
        self.0.public_key().as_ref()
    }
}

pub struct ReceiptKeyManager {
    root: PathBuf,
}
impl ReceiptKeyManager {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            root: data_dir.as_ref().join(KEY_DIR),
        }
    }
    pub fn key_dir(&self) -> &Path {
        &self.root
    }
    pub fn active_path(&self) -> PathBuf {
        self.root.join(ACTIVE_KEY_FILE)
    }
    pub fn history_path(&self) -> PathBuf {
        self.root.join(HISTORY_FILE)
    }
    pub fn trust_path(&self) -> PathBuf {
        self.root.join(TRUST_FILE)
    }
    pub fn journal_path(&self) -> PathBuf {
        self.root.join(JOURNAL_FILE)
    }

    pub fn initialize(&self) -> Result<String, KeyError> {
        if self.active_path().exists() {
            return Err(KeyError::Corrupt);
        }
        fs::create_dir_all(&self.root)?;
        let (signer, pkcs8) = SecretSigner::generate()?;
        let public = signer.public().to_vec();
        let id = key_id(&public);
        let metadata = ActiveKeyMetadata {
            storage_version: 1,
            key_id: id.clone(),
            public_key: b64(&public),
            created_at: now(),
            protected_pkcs8: STANDARD.encode(protect(&pkcs8)?),
        };
        atomic_write_json(&self.active_path(), &metadata)?;
        let genesis = KeyTransition {
            transition_version: 1,
            transition_id: Uuid::now_v7().to_string(),
            created_at: metadata.created_at.clone(),
            reason: "initial".into(),
            actor: "system".into(),
            previous_key_id: None,
            new_key_id: id.clone(),
            new_public_key: metadata.public_key.clone(),
            continuity: "genesis".into(),
            signed_by_key_id: id.clone(),
            signature: String::new(),
            previous_transition_hash: None,
        };
        let mut genesis = genesis;
        genesis.signature = signer.sign(&signed_bytes(&genesis)?);
        atomic_write_lines(&self.history_path(), &[canonical_json(&genesis)?])?;
        Ok(id)
    }

    pub fn load_signer(&self) -> Result<(ActiveKeyMetadata, SecretSigner), KeyError> {
        let raw = fs::read(self.active_path()).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                KeyError::NotInitialized
            } else {
                KeyError::Io(e)
            }
        })?;
        let metadata: ActiveKeyMetadata = serde_json::from_slice(&raw)?;
        if metadata.storage_version != 1 {
            return Err(KeyError::Corrupt);
        }
        let public = decode_b64(&metadata.public_key)?;
        if public.len() != 32 || key_id(&public) != metadata.key_id {
            return Err(KeyError::PublicMismatch);
        }
        let protected = STANDARD
            .decode(&metadata.protected_pkcs8)
            .map_err(|_| KeyError::Corrupt)?;
        let plain = Zeroizing::new(unprotect(&protected)?);
        let signer = SecretSigner::from_pkcs8(&plain)?;
        if signer.public() != public {
            return Err(KeyError::PublicMismatch);
        }
        Ok((metadata, signer))
    }

    pub fn sign_payload(&self, payload: &serde_json::Value) -> Result<(String, String), KeyError> {
        let (metadata, signer) = self.load_signer()?;
        let bytes = crate::payload_bytes(payload).map_err(|_| KeyError::InvalidTransition)?;
        Ok((metadata.key_id, signer.sign(&bytes)))
    }

    pub fn rotate(&self, reason: &str, actor: &str) -> Result<String, KeyError> {
        if !matches!(reason, "scheduled" | "manual" | "compromise")
            || !matches!(actor, "system" | "user")
        {
            return Err(KeyError::InvalidTransition);
        }
        if self.journal_path().exists() {
            return Err(KeyError::RotationIncomplete);
        }
        let (old, old_signer) = self.load_signer()?;
        let history = self.load_history()?;
        let previous = history.last().ok_or(KeyError::HistoryIncomplete)?;
        if history.len() >= MAX_TRANSITIONS {
            return Err(KeyError::RotationLimit);
        }
        let (new_signer, pkcs8) = SecretSigner::generate()?;
        let new_public = new_signer.public().to_vec();
        let new_id = key_id(&new_public);
        let continuity = if reason == "compromise" {
            "compromised"
        } else {
            "chained"
        };
        let mut transition = KeyTransition {
            transition_version: 1,
            transition_id: Uuid::now_v7().to_string(),
            created_at: now(),
            reason: reason.into(),
            actor: actor.into(),
            previous_key_id: Some(old.key_id.clone()),
            new_key_id: new_id.clone(),
            new_public_key: b64(&new_public),
            continuity: continuity.into(),
            signed_by_key_id: old.key_id.clone(),
            signature: String::new(),
            previous_transition_hash: Some(transition_hash(previous)?),
        };
        transition.signature = old_signer.sign(&signed_bytes(&transition)?);
        let hash = transition_hash(&transition)?;
        let metadata = ActiveKeyMetadata {
            storage_version: 1,
            key_id: new_id.clone(),
            public_key: b64(&new_public),
            created_at: now(),
            protected_pkcs8: STANDARD.encode(protect(&pkcs8)?),
        };
        let rotation_id = Uuid::now_v7().to_string();
        let state = RotationState {
            state_version: 1,
            rotation_id: rotation_id.clone(),
            phase: "prepared".into(),
            old_key_id: old.key_id.clone(),
            new_key_id: new_id.clone(),
            transition_hash: hash.clone(),
            error_code: None,
            created_at: now(),
            updated_at: now(),
            reason: reason.into(),
            actor: actor.into(),
            active_key_observed: true,
            audit_event_id: format!("key-{}", transition.transition_id),
        };
        self.write_rotation_state(&state)?;
        let mut next = history;
        next.push(transition);
        let lines = next
            .iter()
            .map(canonical_json)
            .collect::<Result<Vec<_>, _>>()?;
        let history_bytes = lines.iter().map(|line| line.len() + 1).sum::<usize>();
        if history_bytes > MAX_HISTORY_BYTES {
            return Err(KeyError::RotationLimit);
        }
        atomic_write_lines(&self.history_path(), &lines)?;
        let mut state = state;
        state.phase = "transition_durable".into();
        state.updated_at = now();
        self.write_rotation_state(&state)?;
        self.append_audit(&state, "key.rotated", "ok")?;
        state.phase = "audit_durable".into();
        state.updated_at = now();
        self.write_rotation_state(&state)?;
        atomic_write_json(&self.active_path(), &metadata)?;
        state.phase = "active_key_replaced".into();
        state.updated_at = now();
        self.write_rotation_state(&state)?;
        fs::remove_file(self.journal_path())?;
        let _ = new_signer;
        Ok(new_id)
    }

    fn append_audit(
        &self,
        state: &RotationState,
        event_type: &str,
        outcome: &str,
    ) -> Result<(), KeyError> {
        let path = self
            .root
            .parent()
            .unwrap_or(&self.root)
            .join("logs")
            .join("audit.jsonl");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let event = serde_json::json!({"event_type": event_type, "timestamp": now(), "old_key_id": state.old_key_id, "new_key_id": state.new_key_id, "reason": state.reason, "actor": state.actor, "transition_hash": state.transition_hash, "outcome": outcome, "error_code": null});
        let bytes = canonical_json(&event)?;
        if bytes.len() > 4096 {
            return Err(KeyError::Corrupt);
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    }

    pub fn load_history(&self) -> Result<Vec<KeyTransition>, KeyError> {
        let raw = fs::read(self.history_path()).map_err(|_| KeyError::HistoryIncomplete)?;
        if raw.len() > MAX_HISTORY_BYTES || raw.is_empty() || !raw.ends_with(b"\n") {
            return Err(KeyError::HistoryIncomplete);
        }
        raw.split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).map_err(KeyError::from))
            .collect()
    }

    pub fn verify_history(&self, trust_key: Option<&str>) -> Result<VerificationStatus, KeyError> {
        let transitions = self.load_history()?;
        verify_transitions(&transitions, trust_key)
    }

    pub fn trust_genesis(&self, genesis_key_id: &str, source: &str) -> Result<(), KeyError> {
        let history = self.load_history()?;
        let expected = history
            .first()
            .ok_or(KeyError::HistoryIncomplete)?
            .new_key_id
            .clone();
        if genesis_key_id != expected {
            return Err(KeyError::PublicMismatch);
        }
        let mut roots = if self.trust_path().exists() {
            serde_json::from_slice::<TrustedRootsFile>(&fs::read(self.trust_path())?)?
        } else {
            TrustedRootsFile {
                schema_version: 1,
                roots: Vec::new(),
            }
        };
        if roots.schema_version != 1
            || roots
                .roots
                .iter()
                .any(|root| root.genesis_key_id == expected && root.status == "active")
        {
            return Ok(());
        }
        roots.roots.push(TrustedRoot {
            root_version: 1,
            root_id: Uuid::now_v7().to_string(),
            genesis_key_id: expected,
            pinned_at: now(),
            source: source.chars().take(64).collect(),
            status: "active".into(),
            superseded_by: None,
        });
        atomic_write_json(&self.trust_path(), &roots)
    }

    pub fn write_rotation_state(&self, state: &RotationState) -> Result<(), KeyError> {
        validate_rotation_state(state)?;
        atomic_write_json(&self.journal_path(), state)
    }

    pub fn read_rotation_state(&self) -> Result<Option<RotationState>, KeyError> {
        if !self.journal_path().exists() {
            return Ok(None);
        }
        let state: RotationState = serde_json::from_slice(&fs::read(self.journal_path())?)?;
        validate_rotation_state(&state)?;
        Ok(Some(state))
    }
}

pub fn validate_rotation_state(state: &RotationState) -> Result<(), KeyError> {
    if state.state_version != 1
        || !matches!(
            state.phase.as_str(),
            "prepared"
                | "transition_durable"
                | "audit_durable"
                | "active_key_replaced"
                | "cleanup_required"
                | "complete"
        )
        || !matches!(
            state.reason.as_str(),
            "scheduled" | "manual" | "compromise" | "recovery"
        )
        || !matches!(state.actor.as_str(), "system" | "user")
    {
        return Err(KeyError::RotationIncomplete);
    }
    for value in [&state.old_key_id, &state.new_key_id] {
        if !value.starts_with("ed25519:") || value.len() != 72 {
            return Err(KeyError::RotationIncomplete);
        }
    }
    if state.transition_hash.len() != 64
        || !state
            .transition_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || state.audit_event_id.len() > 128
    {
        return Err(KeyError::RotationIncomplete);
    }
    if state.phase == "cleanup_required" && state.error_code.as_deref().unwrap_or("").is_empty() {
        return Err(KeyError::RotationIncomplete);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    Verified,
    Untrusted,
    Broken,
    Unsupported,
}

pub fn verify_transitions(
    items: &[KeyTransition],
    trust_key: Option<&str>,
) -> Result<VerificationStatus, KeyError> {
    if items.is_empty() || items.len() > MAX_TRANSITIONS {
        return Err(KeyError::HistoryIncomplete);
    }
    let mut ids = std::collections::HashSet::new();
    let first = &items[0];
    if first.continuity != "genesis" || first.reason != "initial" || first.previous_key_id.is_some()
    {
        return Err(KeyError::InvalidTransition);
    }
    for (index, item) in items.iter().enumerate() {
        if item.transition_version != 1 || !ids.insert(item.transition_id.clone()) {
            return Err(KeyError::InvalidTransition);
        }
        let public = decode_b64(&item.new_public_key)?;
        if public.len() != 32 || key_id(&public) != item.new_key_id {
            return Err(KeyError::PublicMismatch);
        }
        let signature = decode_b64(&item.signature)?;
        let signer_public = if item.signed_by_key_id == item.new_key_id {
            public.clone()
        } else {
            let previous = items
                .iter()
                .find(|candidate| candidate.new_key_id == item.signed_by_key_id)
                .ok_or(KeyError::InvalidTransition)?;
            decode_b64(&previous.new_public_key)?
        };
        signature::UnparsedPublicKey::new(&signature::ED25519, &signer_public)
            .verify(&signed_bytes(item)?, &signature)
            .map_err(|_| KeyError::Signature)?;
        if index > 0 {
            let previous = &items[index - 1];
            if item.previous_key_id.as_deref() != Some(previous.new_key_id.as_str()) {
                return Err(KeyError::InvalidTransition);
            }
            if item.previous_transition_hash.as_deref() != Some(transition_hash(previous)?.as_str())
            {
                return Err(KeyError::InvalidTransition);
            }
        }
    }
    let genesis = &first.new_key_id;
    let trusted = trust_key.is_some_and(|key| key == genesis);
    if items
        .iter()
        .any(|item| item.continuity == "broken" || item.continuity == "compromised")
    {
        return Ok(VerificationStatus::Untrusted);
    }
    Ok(if trusted {
        VerificationStatus::Verified
    } else {
        VerificationStatus::Untrusted
    })
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), KeyError> {
    atomic_write(path, &canonical_json(value)?)
}
fn atomic_write_lines(path: &Path, lines: &[Vec<u8>]) -> Result<(), KeyError> {
    let mut bytes = Vec::new();
    for line in lines {
        bytes.extend_from_slice(line);
        bytes.push(b'\n');
    }
    atomic_write(path, &bytes)
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), KeyError> {
    let tmp = path.with_extension(format!("tmp-{}", Uuid::now_v7()));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    if let Some(parent) = path.parent() {
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

#[cfg(windows)]
fn protect(bytes: &[u8]) -> Result<Vec<u8>, KeyError> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    let mut input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &mut input,
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0x1,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(KeyError::DpapiFailed);
    }
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe { LocalFree(output.pbData as _) };
    Ok(result)
}
#[cfg(not(windows))]
fn protect(_: &[u8]) -> Result<Vec<u8>, KeyError> {
    Err(KeyError::UnsupportedPlatform)
}

#[cfg(windows)]
fn unprotect(bytes: &[u8]) -> Result<Vec<u8>, KeyError> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let mut input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &mut input,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(KeyError::DpapiFailed);
    }
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe { LocalFree(output.pbData as _) };
    Ok(result)
}
#[cfg(not(windows))]
fn unprotect(_: &[u8]) -> Result<Vec<u8>, KeyError> {
    Err(KeyError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn genesis() -> KeyTransition {
        let pair = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let signer = Ed25519KeyPair::from_pkcs8(pair.as_ref()).unwrap();
        let public = signer.public_key().as_ref().to_vec();
        let mut item = KeyTransition {
            transition_version: 1,
            transition_id: Uuid::now_v7().to_string(),
            created_at: now(),
            reason: "initial".into(),
            actor: "system".into(),
            previous_key_id: None,
            new_key_id: key_id(&public),
            new_public_key: b64(&public),
            continuity: "genesis".into(),
            signed_by_key_id: key_id(&public),
            signature: String::new(),
            previous_transition_hash: None,
        };
        item.signature = b64(signer.sign(&signed_bytes(&item).unwrap()).as_ref());
        item
    }

    #[test]
    fn synthetic_genesis_requires_explicit_pin() {
        let item = genesis();
        assert_eq!(
            verify_transitions(std::slice::from_ref(&item), None).unwrap(),
            VerificationStatus::Untrusted
        );
        assert_eq!(
            verify_transitions(std::slice::from_ref(&item), Some(&item.new_key_id)).unwrap(),
            VerificationStatus::Verified
        );
    }

    #[test]
    fn rotation_state_rejects_invalid_phase() {
        let state = RotationState {
            state_version: 1,
            rotation_id: Uuid::now_v7().to_string(),
            phase: "prepared".into(),
            old_key_id: "ed25519:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            new_key_id: "ed25519:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .into(),
            transition_hash: "c".repeat(64),
            error_code: None,
            created_at: now(),
            updated_at: now(),
            reason: "manual".into(),
            actor: "user".into(),
            active_key_observed: true,
            audit_event_id: "audit".into(),
        };
        assert!(validate_rotation_state(&state).is_ok());
        let mut bad = state;
        bad.phase = "private_key".into();
        assert_eq!(
            validate_rotation_state(&bad).unwrap_err().to_string(),
            "key.rotation_incomplete"
        );
    }
}
