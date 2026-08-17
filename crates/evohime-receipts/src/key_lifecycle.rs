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
    aead,
    rand::{SecureRandom, SystemRandom},
    signature::{self, Ed25519KeyPair, KeyPair},
};
use rusqlite::OptionalExtension;
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
pub const HISTORY_MANIFEST_FILE: &str = "public-history-v1.manifest.json";
pub const RECOVERY_FORENSIC_FILE: &str = "recovery-forensic-v1.json";
pub const ROTATION_CHECK_FILE: &str = "rotation-check-v1.json";
pub const PENDING_KEY_FILE: &str = "pending-key-v1.json";
pub const CHECKPOINT_FILE: &str = "key-history-checkpoint-v1.json";
pub const STORAGE_KEY_FILE: &str = "recovery-storage-key-v1.json";
pub const STORAGE_KEY_HISTORY_FILE: &str = "recovery-storage-key-history-v1.json";
pub const MAX_TRANSITIONS: usize = 100;
pub const MAX_HISTORY_BYTES: usize = 16 * 1024 * 1024;
const STORAGE_KEY_BYTES: usize = 32;
const STORAGE_NONCE_BYTES: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StorageKeyMetadata {
    storage_version: u8,
    key_id: String,
    protected_key: String,
}

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
    #[error("key.history_export_failed")]
    HistoryExportFailed,
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
#[serde(deny_unknown_fields)]
pub struct ActiveKeyMetadata {
    pub storage_version: u8,
    pub key_id: String,
    pub public_key: String,
    pub created_at: String,
    pub protected_pkcs8: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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

#[derive(Serialize)]
struct UnsignedTransition<'a> {
    transition_version: u8,
    transition_id: &'a str,
    created_at: &'a str,
    reason: &'a str,
    actor: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_key_id: &'a Option<String>,
    new_key_id: &'a str,
    new_public_key: &'a str,
    continuity: &'a str,
    signed_by_key_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_transition_hash: &'a Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct TrustedRootsFile {
    pub schema_version: u8,
    pub roots: Vec<TrustedRoot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HistoryManifest {
    pub manifest_version: u8,
    pub history_schema: String,
    pub status: String,
    pub active_key_id: String,
    pub exported_transition_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeyHistoryCheckpoint {
    pub checkpoint_version: u8,
    pub checkpoint_id: String,
    pub created_at: String,
    pub genesis_key_id: String,
    pub lineage_id: String,
    pub covered_first_sequence: u64,
    pub covered_last_sequence: u64,
    pub covered_prefix_hash: String,
    pub last_transition_hash: String,
    pub retained_from_sequence: u64,
    pub signed_by_key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RotationCheck {
    schema_version: u8,
    checked_at: String,
}

#[derive(Serialize)]
struct UnsignedCheckpoint<'a> {
    checkpoint_version: u8,
    checkpoint_id: &'a str,
    created_at: &'a str,
    genesis_key_id: &'a str,
    lineage_id: &'a str,
    covered_first_sequence: u64,
    covered_last_sequence: u64,
    covered_prefix_hash: &'a str,
    last_transition_hash: &'a str,
    retained_from_sequence: u64,
    signed_by_key_id: &'a str,
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
pub fn public_key_bytes(value: &str) -> Result<Vec<u8>, KeyError> {
    let bytes = decode_b64(value)?;
    if bytes.len() != 32 {
        return Err(KeyError::PublicMismatch);
    }
    Ok(bytes)
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

/// Commits the authoritative transition and its bounded audit reference in a
/// single SQLite transaction. JSONL is deliberately not touched here: it is a
/// post-commit snapshot export.
pub fn commit_transition_and_audit(
    connection: &mut rusqlite::Connection,
    transition: &KeyTransition,
    event_id: &str,
    reason: &str,
    actor: &str,
    outcome: &str,
    error_code: Option<&str>,
) -> Result<(), KeyError> {
    let canonical = canonical_json(transition)?;
    let hash = sha256_hex(&canonical);
    let tx = connection
        .unchecked_transaction()
        .map_err(|_| KeyError::Corrupt)?;
    let existing: Option<(String, String)> = tx
        .query_row(
            "SELECT transition_hash, continuity FROM receipt_key_transitions WHERE transition_id = ?1",
            [transition.transition_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| KeyError::Corrupt)?;
    if let Some((existing_hash, _)) = existing {
        if existing_hash != hash {
            return Err(KeyError::RotationFork);
        }
    } else {
        tx.execute(
            "INSERT INTO receipt_key_transitions
             (transition_id, transition_hash, previous_key_id, new_key_id, continuity, canonical_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                transition.transition_id,
                hash,
                transition.previous_key_id,
                transition.new_key_id,
                transition.continuity,
                canonical,
                transition.created_at,
            ],
        )
        .map_err(|_| KeyError::Corrupt)?;
    }
    tx.execute(
        "INSERT INTO receipt_key_audit
         (event_id, transition_id, event_type, old_key_id, new_key_id, transition_hash, reason, actor, outcome, error_code, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(transition_id) DO UPDATE SET event_id = excluded.event_id,
           outcome = excluded.outcome, error_code = excluded.error_code",
        rusqlite::params![
            event_id,
            transition.transition_id,
            match transition.reason.as_str() {
                "initial" => "key.generated",
                "recovery" => "key.recovery_required",
                _ => "key.rotated",
            },
            transition.previous_key_id,
            transition.new_key_id,
            hash,
            reason,
            actor,
            outcome,
            error_code,
            transition.created_at,
        ],
    )
    .map_err(|_| KeyError::Corrupt)?;
    tx.commit().map_err(|_| KeyError::Corrupt)?;
    Ok(())
}

/// Canonical bytes for lifecycle objects. Struct declaration order is the
/// normative schema order; serde rejects no unknown fields when deserializing
/// externally, so the verifier performs the bounded field checks below.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, KeyError> {
    Ok(serde_json::to_vec(value)?)
}
fn signed_bytes(transition: &KeyTransition) -> Result<Vec<u8>, KeyError> {
    Ok(serde_json::to_vec(&UnsignedTransition {
        transition_version: transition.transition_version,
        transition_id: &transition.transition_id,
        created_at: &transition.created_at,
        reason: &transition.reason,
        actor: &transition.actor,
        previous_key_id: &transition.previous_key_id,
        new_key_id: &transition.new_key_id,
        new_public_key: &transition.new_public_key,
        continuity: &transition.continuity,
        signed_by_key_id: &transition.signed_by_key_id,
        previous_transition_hash: &transition.previous_transition_hash,
    })?)
}

fn checkpoint_signed_bytes(checkpoint: &KeyHistoryCheckpoint) -> Result<Vec<u8>, KeyError> {
    Ok(serde_json::to_vec(&UnsignedCheckpoint {
        checkpoint_version: checkpoint.checkpoint_version,
        checkpoint_id: &checkpoint.checkpoint_id,
        created_at: &checkpoint.created_at,
        genesis_key_id: &checkpoint.genesis_key_id,
        lineage_id: &checkpoint.lineage_id,
        covered_first_sequence: checkpoint.covered_first_sequence,
        covered_last_sequence: checkpoint.covered_last_sequence,
        covered_prefix_hash: &checkpoint.covered_prefix_hash,
        last_transition_hash: &checkpoint.last_transition_hash,
        retained_from_sequence: checkpoint.retained_from_sequence,
        signed_by_key_id: &checkpoint.signed_by_key_id,
    })?)
}

pub fn verify_checkpoint(
    checkpoint: &KeyHistoryCheckpoint,
    history: &[KeyTransition],
    trust_key: Option<&str>,
) -> Result<VerificationStatus, KeyError> {
    if checkpoint.checkpoint_version != 1
        || checkpoint.covered_first_sequence > checkpoint.covered_last_sequence
        || checkpoint.retained_from_sequence
            != checkpoint
                .covered_last_sequence
                .checked_add(1)
                .ok_or(KeyError::InvalidTransition)?
        || checkpoint.covered_last_sequence as usize >= MAX_TRANSITIONS
    {
        return Err(KeyError::InvalidTransition);
    }
    let signer = history
        .iter()
        .find(|item| item.new_key_id == checkpoint.signed_by_key_id)
        .ok_or(KeyError::InvalidTransition)?;
    let public = decode_b64(&signer.new_public_key)?;
    let signature = decode_b64(&checkpoint.signature)?;
    signature::UnparsedPublicKey::new(&signature::ED25519, &public)
        .verify(&checkpoint_signed_bytes(checkpoint)?, &signature)
        .map_err(|_| KeyError::Signature)?;
    let prefix = history
        .iter()
        .take(checkpoint.retained_from_sequence as usize)
        .map(canonical_json)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if sha256_hex(&prefix) != checkpoint.covered_prefix_hash {
        return Err(KeyError::HistoryIncomplete);
    }
    if history
        .get(checkpoint.covered_last_sequence as usize)
        .map(transition_hash)
        .transpose()?
        .as_deref()
        != Some(checkpoint.last_transition_hash.as_str())
    {
        return Err(KeyError::HistoryIncomplete);
    }
    let root = normalize_key_id(&checkpoint.genesis_key_id).ok_or(KeyError::InvalidTransition)?;
    let trusted =
        trust_key.is_some_and(|key| normalize_key_id(key).as_deref() == Some(root.as_str()));
    Ok(if trusted {
        VerificationStatus::Verified
    } else {
        VerificationStatus::Untrusted
    })
}

pub struct SecretSigner(Zeroizing<Vec<u8>>);
impl SecretSigner {
    fn generate() -> Result<(Self, Vec<u8>), KeyError> {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .map_err(|_| KeyError::DpapiFailed)?;
        let key = Self::from_pkcs8(pkcs8.as_ref())?;
        Ok((key, pkcs8.as_ref().to_vec()))
    }
    fn from_pkcs8(bytes: &[u8]) -> Result<Self, KeyError> {
        Ed25519KeyPair::from_pkcs8(bytes).map_err(|_| KeyError::Corrupt)?;
        Ok(Self(Zeroizing::new(bytes.to_vec())))
    }
    fn sign(&self, bytes: &[u8]) -> String {
        let key = Ed25519KeyPair::from_pkcs8(&self.0).expect("validated PKCS#8 signer");
        b64(key.sign(bytes).as_ref())
    }
    fn public(&self) -> Vec<u8> {
        Ed25519KeyPair::from_pkcs8(&self.0)
            .expect("validated PKCS#8 signer")
            .public_key()
            .as_ref()
            .to_vec()
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
    pub fn history_manifest_path(&self) -> PathBuf {
        self.root.join(HISTORY_MANIFEST_FILE)
    }
    pub fn trust_path(&self) -> PathBuf {
        self.root.join(TRUST_FILE)
    }
    pub fn journal_path(&self) -> PathBuf {
        self.root.join(JOURNAL_FILE)
    }
    pub fn pending_key_path(&self) -> PathBuf {
        self.root.join(PENDING_KEY_FILE)
    }
    pub fn checkpoint_path(&self) -> PathBuf {
        self.root.join(CHECKPOINT_FILE)
    }
    pub fn rotation_check_path(&self) -> PathBuf {
        self.root.join(ROTATION_CHECK_FILE)
    }

    fn storage_key_path(&self) -> PathBuf { self.root.join(STORAGE_KEY_FILE) }
    fn storage_key_history_path(&self) -> PathBuf { self.root.join(STORAGE_KEY_HISTORY_FILE) }

    pub fn initialize(&self) -> Result<String, KeyError> {
        if self.active_path().exists() {
            return Err(KeyError::Corrupt);
        }
        fs::create_dir_all(&self.root)?;
        harden_path(&self.root)?;
        let (signer, pkcs8) = SecretSigner::generate()?;
        let public = signer.public();
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
        atomic_write_json(
            &self.history_manifest_path(),
            &HistoryManifest {
                manifest_version: 1,
                history_schema: "key-history-v1".into(),
                status: "current".into(),
                active_key_id: id.clone(),
                exported_transition_count: 1,
            },
        )?;
        Ok(id)
    }

    pub fn initialize_with_database(
        &self,
        connection: &mut rusqlite::Connection,
    ) -> Result<String, KeyError> {
        if self.active_path().exists() || self.history_path().exists() {
            return Err(KeyError::Corrupt);
        }
        fs::create_dir_all(&self.root)?;
        harden_path(&self.root)?;
        let (signer, pkcs8) = SecretSigner::generate()?;
        let public = signer.public();
        let id = key_id(&public);
        let metadata = ActiveKeyMetadata {
            storage_version: 1,
            key_id: id.clone(),
            public_key: b64(&public),
            created_at: now(),
            protected_pkcs8: STANDARD.encode(protect(&pkcs8)?),
        };
        let mut genesis = KeyTransition {
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
        genesis.signature = signer.sign(&signed_bytes(&genesis)?);
        let event_id = format!("key-{}", genesis.transition_id);
        commit_transition_and_audit(
            connection, &genesis, &event_id, "initial", "system", "ok", None,
        )?;
        atomic_write_json(&self.active_path(), &metadata)?;
        self.export_history_snapshot(connection, &metadata.key_id)?;
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

    /// Signs the 32-byte SHA-256 payload digest used by Runtime Receipt v1.
    /// This is intentionally separate from the legacy contract helper above:
    /// changing its input would invalidate the already published vectors.
    pub fn sign_payload_hash(&self, payload_hash: &str) -> Result<(String, String), KeyError> {
        if payload_hash.len() != 64 || !payload_hash.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
            return Err(KeyError::InvalidTransition);
        }
        let (metadata, signer) = self.load_signer()?;
        let bytes = hex::decode(payload_hash).map_err(|_| KeyError::InvalidTransition)?;
        Ok((metadata.key_id, signer.sign(&bytes)))
    }

    fn new_storage_metadata(&self) -> Result<(StorageKeyMetadata, [u8; STORAGE_KEY_BYTES]), KeyError> {
        fs::create_dir_all(&self.root)?;
        let mut key = [0u8; STORAGE_KEY_BYTES];
        SystemRandom::new().fill(&mut key).map_err(|_| KeyError::DpapiFailed)?;
        let id = format!("{:x}", Sha256::digest(key));
        let metadata = StorageKeyMetadata { storage_version: 1, key_id: id, protected_key: STANDARD.encode(protect(&key)?) };
        atomic_write_json(&self.storage_key_path(), &metadata)?;
        Ok((metadata, key))
    }

    fn read_storage_metadata(&self, path: &Path) -> Result<(StorageKeyMetadata, [u8; STORAGE_KEY_BYTES]), KeyError> {
        let metadata: StorageKeyMetadata = serde_json::from_slice(&fs::read(path)?)?;
        if metadata.storage_version != 1 { return Err(KeyError::Corrupt); }
        let protected = STANDARD.decode(&metadata.protected_key).map_err(|_| KeyError::Corrupt)?;
        let plain = unprotect(&protected)?;
        if plain.len() != STORAGE_KEY_BYTES || format!("{:x}", Sha256::digest(&plain)) != metadata.key_id { return Err(KeyError::Corrupt); }
        let mut key = [0u8; STORAGE_KEY_BYTES];
        key.copy_from_slice(&plain);
        Ok((metadata, key))
    }

    fn active_storage_key(&self) -> Result<(StorageKeyMetadata, [u8; STORAGE_KEY_BYTES]), KeyError> {
        if self.storage_key_path().exists() { self.read_storage_metadata(&self.storage_key_path()) } else { self.new_storage_metadata() }
    }

    pub fn storage_key_id(&self) -> Result<String, KeyError> {
        Ok(self.active_storage_key()?.0.key_id)
    }

    /// Encrypts bounded recovery material with a versioned per-user storage
    /// key. The key itself is DPAPI-protected and never leaves this manager.
    pub fn protect_storage(&self, bytes: &[u8]) -> Result<Vec<u8>, KeyError> {
        let (_, key) = self.active_storage_key()?;
        let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &key).map_err(|_| KeyError::DpapiFailed)?;
        let less = aead::LessSafeKey::new(unbound);
        let mut nonce = [0u8; STORAGE_NONCE_BYTES];
        SystemRandom::new().fill(&mut nonce).map_err(|_| KeyError::DpapiFailed)?;
        let mut output = bytes.to_vec();
        less.seal_in_place_append_tag(aead::Nonce::assume_unique_for_key(nonce), aead::Aad::empty(), &mut output).map_err(|_| KeyError::DpapiFailed)?;
        let mut envelope = nonce.to_vec();
        envelope.extend(output);
        Ok(envelope)
    }

    pub fn unprotect_storage(&self, bytes: &[u8]) -> Result<Vec<u8>, KeyError> {
        if bytes.len() < STORAGE_NONCE_BYTES + aead::AES_256_GCM.tag_len() { return Err(KeyError::Corrupt); }
        let mut candidates = Vec::new();
        if self.storage_key_path().exists() {
            if let Ok((_, key)) = self.read_storage_metadata(&self.storage_key_path()) { candidates.push(key); }
        }
        if let Ok(raw) = fs::read(self.storage_key_history_path()) {
            if let Ok(history) = serde_json::from_slice::<Vec<StorageKeyMetadata>>(&raw) {
                for metadata in history.into_iter().rev().take(8) {
                    if let Ok((_, key)) = self.read_storage_metadata_from_value(&metadata) { candidates.push(key); }
                }
            }
        }
        let nonce_bytes: [u8; STORAGE_NONCE_BYTES] = bytes[..STORAGE_NONCE_BYTES].try_into().map_err(|_| KeyError::Corrupt)?;
        for key in candidates {
            let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &key).map_err(|_| KeyError::Corrupt)?;
            let less = aead::LessSafeKey::new(unbound);
            let mut ciphertext = bytes[STORAGE_NONCE_BYTES..].to_vec();
            if let Ok(plain) = less.open_in_place(aead::Nonce::assume_unique_for_key(nonce_bytes), aead::Aad::empty(), &mut ciphertext) { return Ok(plain.to_vec()); }
        }
        Err(KeyError::DpapiFailed)
    }

    /// Re-encrypts a protected recovery envelope with the current active
    /// storage key. Plaintext remains inside the key-manager boundary and is
    /// never returned to Core.
    pub fn rewrap_storage(&self, bytes: &[u8]) -> Result<Vec<u8>, KeyError> {
        let plaintext = self.unprotect_storage(bytes)?;
        self.protect_storage(&plaintext)
    }

    /// Rewraps a JSON recovery envelope while updating its bounded storage-key
    /// metadata. This keeps the row's authenticated key identifier aligned
    /// with the database index without exposing key bytes to Core.
    pub fn rewrap_storage_with_key_id(&self, bytes: &[u8], key_id: &str) -> Result<Vec<u8>, KeyError> {
        if key_id.is_empty() || key_id.len() > 128 || key_id.contains('\n') {
            return Err(KeyError::Corrupt);
        }
        let plaintext = self.unprotect_storage(bytes)?;
        let mut value: serde_json::Value = serde_json::from_slice(&plaintext).map_err(|_| KeyError::Corrupt)?;
        let object = value.as_object_mut().ok_or(KeyError::Corrupt)?;
        object.insert("key_id".into(), serde_json::Value::String(key_id.into()));
        let updated = serde_json::to_vec(&value).map_err(|_| KeyError::Corrupt)?;
        self.protect_storage(&updated)
    }

    fn read_storage_metadata_from_value(&self, metadata: &StorageKeyMetadata) -> Result<(StorageKeyMetadata, [u8; STORAGE_KEY_BYTES]), KeyError> {
        if metadata.storage_version != 1 { return Err(KeyError::Corrupt); }
        let protected = STANDARD.decode(&metadata.protected_key).map_err(|_| KeyError::Corrupt)?;
        let plain = unprotect(&protected)?;
        if plain.len() != STORAGE_KEY_BYTES || format!("{:x}", Sha256::digest(&plain)) != metadata.key_id { return Err(KeyError::Corrupt); }
        let mut key = [0u8; STORAGE_KEY_BYTES]; key.copy_from_slice(&plain); Ok((metadata.clone(), key))
    }

    pub fn rotate_storage_key(&self, authenticated_operator: bool) -> Result<String, KeyError> {
        if !authenticated_operator { return Err(KeyError::TrustRequired); }
        let (old, _) = self.active_storage_key()?;
        let mut history: Vec<StorageKeyMetadata> = fs::read(&self.storage_key_history_path()).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default();
        if !history.iter().any(|item: &StorageKeyMetadata| item.key_id == old.key_id) { history.push(old); }
        if history.len() > 8 { history.drain(..history.len() - 8); }
        atomic_write_json(&self.storage_key_history_path(), &history)?;
        Ok(self.new_storage_metadata()?.0.key_id)
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
        let new_public = new_signer.public();
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

    /// Full Core path. SQLite is committed first as the mutable source of
    /// truth; public JSONL is exported only from that committed snapshot.
    pub fn rotate_with_database(
        &self,
        connection: &mut rusqlite::Connection,
        reason: &str,
        actor: &str,
    ) -> Result<String, KeyError> {
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
        let new_public = new_signer.public();
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
        let transition_hash_value = transition_hash(&transition)?;
        let protected = protect(&pkcs8)?;
        let metadata = ActiveKeyMetadata {
            storage_version: 1,
            key_id: new_id.clone(),
            public_key: b64(&new_public),
            created_at: now(),
            protected_pkcs8: STANDARD.encode(protected),
        };
        let state = RotationState {
            state_version: 1,
            rotation_id: Uuid::now_v7().to_string(),
            phase: "prepared".into(),
            old_key_id: old.key_id.clone(),
            new_key_id: new_id.clone(),
            transition_hash: transition_hash_value,
            error_code: None,
            created_at: now(),
            updated_at: now(),
            reason: reason.into(),
            actor: actor.into(),
            active_key_observed: true,
            audit_event_id: format!("key-{}", transition.transition_id),
        };
        self.write_rotation_state(&state)?;
        atomic_write_json(&self.pending_key_path(), &metadata)?;
        commit_transition_and_audit(
            connection,
            &transition,
            &state.audit_event_id,
            reason,
            actor,
            "ok",
            None,
        )?;
        let mut state = state;
        state.phase = "transition_durable".into();
        state.updated_at = now();
        self.write_rotation_state(&state)?;
        self.append_audit(&state, "key.rotated", "ok")?;
        self.export_history_snapshot(connection, &old.key_id)?;
        state.phase = "audit_durable".into();
        state.updated_at = now();
        self.write_rotation_state(&state)?;
        atomic_write_json(&self.active_path(), &metadata)?;
        state.phase = "active_key_replaced".into();
        state.updated_at = now();
        self.write_rotation_state(&state)?;
        self.export_history_snapshot(connection, &new_id)?;
        if self.pending_key_path().exists() && fs::remove_file(self.pending_key_path()).is_err() {
            state.phase = "cleanup_required".into();
            state.error_code = Some("key.cleanup_required".into());
            state.updated_at = now();
            self.write_rotation_state(&state)?;
            return Err(KeyError::CleanupRequired);
        }
        fs::remove_file(self.journal_path())?;
        Ok(new_id)
    }

    pub fn create_new_genesis_with_database(
        &self,
        connection: &mut rusqlite::Connection,
        actor: &str,
    ) -> Result<String, KeyError> {
        if actor != "user" || !self.history_path().exists() {
            return Err(KeyError::NotInitialized);
        }
        let history = self.load_history()?;
        let previous = history.last().ok_or(KeyError::HistoryIncomplete)?;
        if history.len() >= MAX_TRANSITIONS {
            return Err(KeyError::RotationLimit);
        }
        let forensic = serde_json::json!({
            "schema_version": 1,
            "reason": "recovery",
            "previous_key_id": previous.new_key_id,
            "recorded_at": now(),
            "active_key_present": self.active_path().exists(),
        });
        atomic_write_json(&self.root.join(RECOVERY_FORENSIC_FILE), &forensic)?;
        let (signer, pkcs8) = SecretSigner::generate()?;
        let public = signer.public();
        let new_id = key_id(&public);
        let mut transition = KeyTransition {
            transition_version: 1,
            transition_id: Uuid::now_v7().to_string(),
            created_at: now(),
            reason: "recovery".into(),
            actor: actor.into(),
            previous_key_id: Some(previous.new_key_id.clone()),
            new_key_id: new_id.clone(),
            new_public_key: b64(&public),
            continuity: "broken".into(),
            signed_by_key_id: new_id.clone(),
            signature: String::new(),
            previous_transition_hash: Some(transition_hash(previous)?),
        };
        transition.signature = signer.sign(&signed_bytes(&transition)?);
        let protected = protect(&pkcs8)?;
        let metadata = ActiveKeyMetadata {
            storage_version: 1,
            key_id: new_id.clone(),
            public_key: b64(&public),
            created_at: now(),
            protected_pkcs8: STANDARD.encode(protected),
        };
        commit_transition_and_audit(
            connection,
            &transition,
            &format!("key-{}", transition.transition_id),
            "recovery",
            actor,
            "ok",
            Some("key.recovery_required"),
        )?;
        atomic_write_json(&self.active_path(), &metadata)?;
        self.export_history_snapshot(connection, &new_id)?;
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
            harden_path(parent)?;
        }
        let event = serde_json::json!({"event_type": event_type, "timestamp": now(), "old_key_id": state.old_key_id, "new_key_id": state.new_key_id, "reason": state.reason, "actor": state.actor, "transition_hash": state.transition_hash, "outcome": outcome, "error_code": null});
        let bytes = canonical_json(&event)?;
        if bytes.len() > 4096 {
            return Err(KeyError::Corrupt);
        }
        if path.exists() {
            harden_path(&path)?;
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

    pub fn export_history_snapshot(
        &self,
        connection: &rusqlite::Connection,
        active_key_id: &str,
    ) -> Result<(), KeyError> {
        let result = (|| {
            let mut statement = connection
                .prepare("SELECT canonical_json FROM receipt_key_transitions ORDER BY sequence ASC")
                .map_err(|_| KeyError::HistoryIncomplete)?;
            let rows = statement
                .query_map([], |row| row.get::<_, Vec<u8>>(0))
                .map_err(|_| KeyError::HistoryIncomplete)?;
            let mut lines = Vec::new();
            for row in rows {
                let line = row.map_err(|_| KeyError::HistoryIncomplete)?;
                if line.len() > 2048 {
                    return Err(KeyError::HistoryIncomplete);
                }
                lines.push(line);
            }
            if lines.is_empty() || lines.len() > MAX_TRANSITIONS {
                return Err(KeyError::HistoryIncomplete);
            }
            let total = lines.iter().map(|line| line.len() + 1).sum::<usize>();
            if total > MAX_HISTORY_BYTES {
                return Err(KeyError::RotationLimit);
            }
            atomic_write_lines(&self.history_path(), &lines)?;
            atomic_write_json(
                &self.history_manifest_path(),
                &HistoryManifest {
                    manifest_version: 1,
                    history_schema: "key-history-v1".into(),
                    status: "current".into(),
                    active_key_id: active_key_id.into(),
                    exported_transition_count: lines.len(),
                },
            )?;
            Ok(())
        })();
        if result.is_err() {
            let _ = atomic_write_json(
                &self.history_manifest_path(),
                &HistoryManifest {
                    manifest_version: 1,
                    history_schema: "key-history-v1".into(),
                    status: "key.history_export_failed".into(),
                    active_key_id: active_key_id.into(),
                    exported_transition_count: 0,
                },
            );
            return Err(KeyError::HistoryExportFailed);
        }
        result
    }

    pub fn verify_history(&self, trust_key: Option<&str>) -> Result<VerificationStatus, KeyError> {
        let transitions = self.load_history()?;
        verify_transitions(&transitions, trust_key)
    }

    pub fn create_checkpoint(
        &self,
        retained_from_sequence: u64,
    ) -> Result<KeyHistoryCheckpoint, KeyError> {
        let (active, signer) = self.load_signer()?;
        let history = self.load_history()?;
        if history.is_empty()
            || retained_from_sequence == 0
            || retained_from_sequence as usize > history.len()
        {
            return Err(KeyError::HistoryIncomplete);
        }
        let covered_last = retained_from_sequence - 1;
        let prefix = history
            .iter()
            .take(retained_from_sequence as usize)
            .map(canonical_json)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let last_hash = transition_hash(
            history
                .get(covered_last as usize)
                .ok_or(KeyError::HistoryIncomplete)?,
        )?;
        let mut checkpoint = KeyHistoryCheckpoint {
            checkpoint_version: 1,
            checkpoint_id: Uuid::now_v7().to_string(),
            created_at: now(),
            genesis_key_id: history
                .first()
                .ok_or(KeyError::HistoryIncomplete)?
                .new_key_id
                .clone(),
            lineage_id: history
                .first()
                .ok_or(KeyError::HistoryIncomplete)?
                .transition_id
                .clone(),
            covered_first_sequence: 0,
            covered_last_sequence: covered_last,
            covered_prefix_hash: sha256_hex(&prefix),
            last_transition_hash: last_hash,
            retained_from_sequence,
            signed_by_key_id: active.key_id,
            signature: String::new(),
        };
        checkpoint.signature = signer.sign(&checkpoint_signed_bytes(&checkpoint)?);
        if canonical_json(&checkpoint)?.len() > 4096 {
            return Err(KeyError::RotationLimit);
        }
        Ok(checkpoint)
    }

    /// Publishes a checkpoint only after it is complete and signed. The
    /// checkpoint is independent from the JSONL snapshot and is replaced
    /// atomically, so a torn write cannot be mistaken for a valid compaction.
    pub fn write_checkpoint(&self, checkpoint: &KeyHistoryCheckpoint) -> Result<(), KeyError> {
        atomic_write_json(&self.checkpoint_path(), checkpoint)
    }

    pub fn trust_genesis(&self, genesis_key_id: &str, source: &str) -> Result<(), KeyError> {
        let history = self.load_history()?;
        let expected = history
            .first()
            .ok_or(KeyError::HistoryIncomplete)?
            .new_key_id
            .clone();
        if normalize_key_id(genesis_key_id).as_deref() != Some(expected.as_str()) {
            return Err(KeyError::PublicMismatch);
        }
        let mut roots = self.load_trusted_roots()?.unwrap_or(TrustedRootsFile {
            schema_version: 1,
            roots: Vec::new(),
        });
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

    pub fn load_trusted_roots(&self) -> Result<Option<TrustedRootsFile>, KeyError> {
        if !self.trust_path().exists() {
            return Ok(None);
        }
        let roots: TrustedRootsFile = serde_json::from_slice(&fs::read(self.trust_path())?)?;
        if roots.schema_version != 1 || roots.roots.len() > 16 {
            return Err(KeyError::TrustRequired);
        }
        let mut ids = std::collections::HashSet::new();
        let mut genesis = std::collections::HashSet::new();
        for root in &roots.roots {
            if root.root_version != 1
                || !ids.insert(root.root_id.clone())
                || (root.status == "active" && !genesis.insert(root.genesis_key_id.clone()))
                || !matches!(root.status.as_str(), "active" | "revoked" | "superseded")
                || normalize_key_id(&root.genesis_key_id).is_none()
            {
                return Err(KeyError::TrustRequired);
            }
        }
        Ok(Some(roots))
    }

    pub fn trusted_genesis(&self, genesis_key_id: &str) -> Result<bool, KeyError> {
        let Some(roots) = self.load_trusted_roots()? else {
            return Ok(false);
        };
        let key = normalize_key_id(genesis_key_id).ok_or(KeyError::TrustRequired)?;
        Ok(roots
            .roots
            .iter()
            .any(|root| root.status == "active" && root.genesis_key_id == key))
    }

    pub fn scheduled_rotation_due(&self) -> Result<bool, KeyError> {
        let (metadata, _) = self.load_signer()?;
        let created = chrono::DateTime::parse_from_rfc3339(&metadata.created_at)
            .map_err(|_| KeyError::Corrupt)?
            .with_timezone(&Utc);
        let now = Utc::now();
        if now.signed_duration_since(created).num_days() < 90 {
            return Ok(false);
        }
        if let Ok(bytes) = fs::read(self.rotation_check_path()) {
            if let Ok(check) = serde_json::from_slice::<RotationCheck>(&bytes) {
                if let Ok(checked) = chrono::DateTime::parse_from_rfc3339(&check.checked_at) {
                    if now
                        .signed_duration_since(checked.with_timezone(&Utc))
                        .num_hours()
                        < 24
                    {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    pub fn record_rotation_check(&self) -> Result<(), KeyError> {
        atomic_write_json(
            &self.rotation_check_path(),
            &RotationCheck {
                schema_version: 1,
                checked_at: now(),
            },
        )
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

    pub fn startup_with_database(
        &self,
        connection: &mut rusqlite::Connection,
    ) -> Result<(), KeyError> {
        let transition_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM receipt_key_transitions", [], |row| {
                row.get(0)
            })
            .map_err(|_| KeyError::Corrupt)?;
        let active = self.active_path().exists();
        let history = self.history_path().exists();
        if !active && !history {
            if transition_count == 0 {
                self.initialize_with_database(connection)?;
                return Ok(());
            }
            return Err(KeyError::HistoryIncomplete);
        }
        if !active || !history {
            return Err(KeyError::HistoryIncomplete);
        }
        let (mut metadata, _) = self.load_signer()?;
        if let Some(state) = self.read_rotation_state()? {
            self.recover_rotation_state(connection, &state, &metadata.key_id)?;
            metadata = serde_json::from_slice(&fs::read(self.active_path())?)?;
        }
        let transitions = self.load_history()?;
        let db_hashes = {
            let mut statement = connection
                .prepare("SELECT canonical_json FROM receipt_key_transitions ORDER BY sequence ASC")
                .map_err(|_| KeyError::Corrupt)?;
            let rows = statement
                .query_map([], |row| row.get::<_, Vec<u8>>(0))
                .map_err(|_| KeyError::Corrupt)?;
            rows.map(|row| row.map(|bytes| sha256_hex(&bytes)))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| KeyError::Corrupt)?
        };
        let file_hashes = transitions
            .iter()
            .map(transition_hash)
            .collect::<Result<Vec<_>, _>>()?;
        if transitions.len() as i64 != transition_count
            || db_hashes != file_hashes
            || transitions.last().map(|item| item.new_key_id.as_str())
                != Some(metadata.key_id.as_str())
        {
            return Err(KeyError::HistoryIncomplete);
        }
        Ok(())
    }

    fn recover_rotation_state(
        &self,
        connection: &mut rusqlite::Connection,
        state: &RotationState,
        active_key_id: &str,
    ) -> Result<(), KeyError> {
        let exists: Option<String> = connection
            .query_row(
                "SELECT transition_id FROM receipt_key_transitions WHERE transition_hash = ?1",
                [state.transition_hash.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| KeyError::Corrupt)?;
        let audit_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM receipt_key_audit WHERE transition_hash = ?1)",
                [state.transition_hash.as_str()],
                |row| row.get(0),
            )
            .map_err(|_| KeyError::Corrupt)?;
        if exists.is_some() && !audit_exists {
            return Err(KeyError::RotationIncomplete);
        }
        match (
            state.phase.as_str(),
            exists,
            active_key_id == state.new_key_id,
        ) {
            ("prepared", None, true) => Err(KeyError::RotationIncomplete),
            ("prepared", None, false) => {
                let _ = fs::remove_file(self.pending_key_path());
                fs::remove_file(self.journal_path())?;
                Ok(())
            }
            (_, Some(_), true) => {
                self.export_history_snapshot(connection, active_key_id)?;
                let _ = fs::remove_file(self.pending_key_path());
                fs::remove_file(self.journal_path())?;
                Ok(())
            }
            (_, Some(_), false) => {
                let bytes =
                    fs::read(self.pending_key_path()).map_err(|_| KeyError::RotationIncomplete)?;
                let pending: ActiveKeyMetadata =
                    serde_json::from_slice(&bytes).map_err(|_| KeyError::RotationIncomplete)?;
                if pending.key_id != state.new_key_id {
                    return Err(KeyError::RotationIncomplete);
                }
                let public = public_key_bytes(&pending.public_key)
                    .map_err(|_| KeyError::RotationIncomplete)?;
                let protected = STANDARD
                    .decode(&pending.protected_pkcs8)
                    .map_err(|_| KeyError::RotationIncomplete)?;
                let plain = Zeroizing::new(unprotect(&protected)?);
                let signer = SecretSigner::from_pkcs8(&plain)?;
                if signer.public() != public {
                    return Err(KeyError::PublicMismatch);
                }
                atomic_write_json(&self.active_path(), &pending)?;
                self.export_history_snapshot(connection, &pending.key_id)?;
                let _ = fs::remove_file(self.pending_key_path());
                fs::remove_file(self.journal_path())?;
                Ok(())
            }
            (_, None, _) => Err(KeyError::RotationIncomplete),
        }
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
    use std::collections::{HashMap, HashSet};
    let mut ids = HashSet::new();
    let mut by_key = HashMap::new();
    let mut by_hash = HashMap::new();
    let mut genesis = Vec::new();
    for item in items {
        if item.transition_version != 1 || !ids.insert(item.transition_id.clone()) {
            return Err(KeyError::InvalidTransition);
        }
        if item.transition_id.len() != 36
            || item.transition_id.as_bytes().get(14) != Some(&b'7')
            || !matches!(
                item.transition_id.as_bytes().get(19),
                Some(b'8' | b'9' | b'a' | b'b')
            )
            || !item
                .transition_id
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == '-')
        {
            return Err(KeyError::InvalidTransition);
        }
        if !matches!(
            item.reason.as_str(),
            "initial" | "scheduled" | "manual" | "compromise" | "recovery"
        ) || !matches!(item.actor.as_str(), "system" | "user")
            || !matches!(
                item.continuity.as_str(),
                "genesis" | "chained" | "compromised" | "broken"
            )
        {
            return Err(KeyError::InvalidTransition);
        }
        let public = decode_b64(&item.new_public_key)?;
        if public.len() != 32 || key_id(&public) != item.new_key_id {
            return Err(KeyError::PublicMismatch);
        }
        if by_key.insert(item.new_key_id.clone(), item).is_some() {
            return Err(KeyError::RotationFork);
        }
        let hash = transition_hash(item)?;
        if by_hash.insert(hash, item).is_some() {
            return Err(KeyError::InvalidTransition);
        }
        if item.previous_key_id.is_none() {
            genesis.push(item);
        } else if item.previous_transition_hash.is_none() {
            return Err(KeyError::InvalidTransition);
        }
    }
    if genesis.len() != 1 {
        return Err(KeyError::HistoryIncomplete);
    }
    let first = genesis[0];
    if first.continuity != "genesis" || first.reason != "initial" || first.actor != "system" {
        return Err(KeyError::InvalidTransition);
    }
    let mut predecessor_of = HashSet::new();
    let mut successor_count: HashMap<String, usize> = HashMap::new();
    for item in items {
        let public = decode_b64(&item.new_public_key)?;
        let signature = decode_b64(&item.signature)?;
        let signer_public = if item.signed_by_key_id == item.new_key_id {
            public.clone()
        } else {
            let previous = by_key
                .get(&item.signed_by_key_id)
                .ok_or(KeyError::InvalidTransition)?;
            decode_b64(&previous.new_public_key)?
        };
        signature::UnparsedPublicKey::new(&signature::ED25519, &signer_public)
            .verify(&signed_bytes(item)?, &signature)
            .map_err(|_| KeyError::Signature)?;
        if let (Some(previous_key), Some(previous_hash)) = (
            item.previous_key_id.as_ref(),
            item.previous_transition_hash.as_ref(),
        ) {
            let previous = by_key
                .get(previous_key)
                .ok_or(KeyError::InvalidTransition)?;
            if by_hash
                .get(previous_hash)
                .map(|candidate| candidate.transition_id.as_str())
                != Some(previous.transition_id.as_str())
                || previous_hash != &transition_hash(previous)?
            {
                return Err(KeyError::InvalidTransition);
            }
            if !predecessor_of.insert(item.new_key_id.clone()) {
                return Err(KeyError::RotationFork);
            }
            *successor_count
                .entry(previous.new_key_id.clone())
                .or_default() += 1;
            if *successor_count.get(&previous.new_key_id).unwrap_or(&0) > 1 {
                return Err(KeyError::RotationFork);
            }
            match item.continuity.as_str() {
                "chained"
                    if !matches!(item.reason.as_str(), "scheduled" | "manual")
                        || item.signed_by_key_id != previous.new_key_id =>
                {
                    return Err(KeyError::InvalidTransition)
                }
                "compromised"
                    if item.reason != "compromise"
                        || item.signed_by_key_id != previous.new_key_id =>
                {
                    return Err(KeyError::InvalidTransition)
                }
                "broken"
                    if item.reason != "recovery" || item.signed_by_key_id != item.new_key_id =>
                {
                    return Err(KeyError::InvalidTransition)
                }
                _ => {}
            }
        } else if item.continuity != "genesis" {
            return Err(KeyError::InvalidTransition);
        }
    }
    let terminal_count = items
        .iter()
        .filter(|item| !successor_count.contains_key(&item.new_key_id))
        .count();
    if terminal_count != 1 {
        return Err(KeyError::HistoryIncomplete);
    }
    let mut visited = HashSet::new();
    let mut current = first;
    loop {
        if !visited.insert(current.new_key_id.clone()) {
            return Err(KeyError::InvalidTransition);
        }
        let next = items.iter().find(|candidate| {
            candidate.previous_key_id.as_deref() == Some(current.new_key_id.as_str())
        });
        match next {
            Some(next) => current = next,
            None => break,
        }
    }
    if visited.len() != items.len() {
        return Err(KeyError::HistoryIncomplete);
    }
    let genesis = &first.new_key_id;
    let trusted = trust_key.is_some_and(|key| normalize_key_id(key).as_deref() == Some(genesis));
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

fn normalize_key_id(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Some(format!("ed25519:{value}"))
    } else if value.starts_with("ed25519:")
        && value.len() == 72
        && value[8..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        Some(value)
    } else {
        None
    }
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
    harden_path(&tmp)?;
    fs::rename(&tmp, path)?;
    harden_path(path)?;
    if let Some(parent) = path.parent() {
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

#[cfg(windows)]
fn harden_path(path: &Path) -> Result<(), KeyError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{LocalFree, HLOCAL};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenUser, DACL_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(KeyError::DaclInvalid);
    }
    let mut required = 0_u32;
    unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut required) };
    if required == 0 {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(token) };
        return Err(KeyError::DaclInvalid);
    }
    let mut token_data = vec![0_u8; required as usize];
    if unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            token_data.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(token) };
        return Err(KeyError::DaclInvalid);
    }
    let user = unsafe { &*(token_data.as_ptr() as *const TOKEN_USER) };
    let mut sid_text = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid_text) } == 0 {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(token) };
        return Err(KeyError::DaclInvalid);
    }
    let mut length = 0;
    while unsafe { *sid_text.add(length) } != 0 {
        length += 1;
    }
    let sid = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(sid_text, length) });
    unsafe {
        LocalFree(sid_text as HLOCAL);
        windows_sys::Win32::Foundation::CloseHandle(token);
    }
    let sddl = format!("D:P(A;;FA;;;{sid})(A;;FA;;;SY)");
    let wide_sddl: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide_sddl.as_ptr(),
            SDDL_REVISION_1 as u32,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(KeyError::DaclInvalid);
    }
    let wide_path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let ok = unsafe {
        windows_sys::Win32::Security::SetFileSecurityW(
            wide_path.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    } != 0;
    unsafe {
        LocalFree(descriptor as HLOCAL);
    }
    if ok {
        Ok(())
    } else {
        Err(KeyError::DaclInvalid)
    }
}

#[cfg(not(windows))]
fn harden_path(_: &Path) -> Result<(), KeyError> {
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

    #[cfg(windows)]
    #[test]
    fn protected_storage_survives_rotation_with_history_fallback() {
        let root = std::env::temp_dir().join(format!("evohime-receipt-storage-{}", Uuid::now_v7()));
        let manager = ReceiptKeyManager::new(&root);
        manager.initialize().unwrap();
        let first_id = manager.storage_key_id().unwrap();
        let envelope = manager.protect_storage(b"bounded recovery").unwrap();
        let second_id = manager.rotate_storage_key(true).unwrap();
        assert_ne!(first_id, second_id);
        assert_eq!(manager.unprotect_storage(&envelope).unwrap(), b"bounded recovery");
        assert!(manager.rotate_storage_key(false).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

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

    #[test]
    fn verifier_accepts_reordered_chain_and_rejects_fork() {
        // Build a transition signed by the genesis key, retaining that signer
        // explicitly so the vector exercises the same verification path.
        let pair = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let genesis_signer = Ed25519KeyPair::from_pkcs8(pair.as_ref()).unwrap();
        let genesis_public = genesis_signer.public_key().as_ref().to_vec();
        let mut first = KeyTransition {
            transition_version: 1,
            transition_id: Uuid::now_v7().to_string(),
            created_at: now(),
            reason: "initial".into(),
            actor: "system".into(),
            previous_key_id: None,
            new_key_id: key_id(&genesis_public),
            new_public_key: b64(&genesis_public),
            continuity: "genesis".into(),
            signed_by_key_id: key_id(&genesis_public),
            signature: String::new(),
            previous_transition_hash: None,
        };
        first.signature = b64(genesis_signer.sign(&signed_bytes(&first).unwrap()).as_ref());
        let (second_signer, _) = SecretSigner::generate().unwrap();
        let second_public = second_signer.public();
        let mut second = KeyTransition {
            transition_version: 1,
            transition_id: Uuid::now_v7().to_string(),
            created_at: now(),
            reason: "manual".into(),
            actor: "user".into(),
            previous_key_id: Some(first.new_key_id.clone()),
            new_key_id: key_id(&second_public),
            new_public_key: b64(&second_public),
            continuity: "chained".into(),
            signed_by_key_id: first.new_key_id.clone(),
            signature: String::new(),
            previous_transition_hash: Some(transition_hash(&first).unwrap()),
        };
        second.signature = b64(genesis_signer
            .sign(&signed_bytes(&second).unwrap())
            .as_ref());
        assert_eq!(
            verify_transitions(&[second.clone(), first.clone()], Some(&first.new_key_id)).unwrap(),
            VerificationStatus::Verified
        );
        let mut fork = second;
        fork.transition_id = Uuid::now_v7().to_string();
        assert!(verify_transitions(&[first, fork], None).is_err());
    }

    #[test]
    fn shared_key_transition_vector_is_verified() {
        let vector: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/receipts/v1/key-transition-vectors.json"
        ))
        .unwrap();
        let value = &vector["positive"][0];
        let transition: KeyTransition = serde_json::from_value(serde_json::json!({
            "transition_version": 1,
            "transition_id": "018c4f4e-5c00-7abc-8def-0123456789ab",
            "created_at": "2025-01-15T12:34:56.789Z",
            "reason": "initial",
            "actor": "system",
            "new_key_id": value["new_key_id"],
            "new_public_key": value["new_public_key"],
            "continuity": "genesis",
            "signed_by_key_id": value["new_key_id"],
            "signature": value["signature"]
        }))
        .unwrap();
        assert_eq!(
            String::from_utf8(signed_bytes(&transition).unwrap()).unwrap(),
            value["canonical_unsigned"]
        );
        assert_eq!(
            verify_transitions(
                std::slice::from_ref(&transition),
                Some(&transition.new_key_id)
            )
            .unwrap(),
            VerificationStatus::Verified
        );
    }

    #[test]
    fn sqlite_transition_and_audit_commit_is_idempotent_and_fork_safe() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE receipt_key_transitions (sequence INTEGER PRIMARY KEY AUTOINCREMENT, transition_id TEXT NOT NULL UNIQUE, transition_hash TEXT NOT NULL UNIQUE, previous_key_id TEXT, new_key_id TEXT NOT NULL, continuity TEXT NOT NULL, canonical_json BLOB NOT NULL, created_at TEXT NOT NULL); CREATE TABLE receipt_key_audit (event_id TEXT PRIMARY KEY, transition_id TEXT NOT NULL UNIQUE, event_type TEXT NOT NULL, old_key_id TEXT, new_key_id TEXT, transition_hash TEXT NOT NULL, reason TEXT NOT NULL, actor TEXT NOT NULL, outcome TEXT NOT NULL, error_code TEXT, created_at TEXT NOT NULL);").unwrap();
        let transition = genesis();
        commit_transition_and_audit(
            &mut connection,
            &transition,
            "audit-1",
            "initial",
            "system",
            "ok",
            None,
        )
        .unwrap();
        commit_transition_and_audit(
            &mut connection,
            &transition,
            "audit-1",
            "initial",
            "system",
            "ok",
            None,
        )
        .unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM receipt_key_transitions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        let mut fork = transition;
        fork.signature = "invalid".into();
        assert_eq!(
            commit_transition_and_audit(
                &mut connection,
                &fork,
                "audit-2",
                "initial",
                "system",
                "ok",
                None
            )
            .unwrap_err()
            .to_string(),
            "key.rotation_fork"
        );
    }
}
