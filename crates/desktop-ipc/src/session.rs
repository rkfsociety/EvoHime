//! Session authentication contract for `desktop-ipc-v1`.
//!
//! The supervisor generates one launch context per Core generation: an
//! unpredictable pipe name, a session secret, and the identity (user SID and
//! Windows logon session) that the shell is expected to run as. The context
//! reaches Core through a protected launch channel, never through renderer
//! arguments. A missing context is accepted only for an explicitly enabled
//! developer launch.
//!
//! On every connection Core issues a fresh, single-use, time-bounded nonce and
//! the client answers with `HMAC-SHA256(secret, role | client_id | nonce)`.
//! A replayed nonce, an expired nonce, a wrong proof, a foreign identity or an
//! incompatible protocol major all reject the connection. Because the secret
//! outlives one connection, a shell restart can still authenticate, while a
//! captured handshake cannot be replayed.
//!
//! The ACL on the pipe remains the primary defence against another user or
//! another logon session; this contract additionally binds a connection to the
//! Core generation that issued the secret. Neither protects against malware
//! already running as the same user — see `docs/security/`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Length in bytes of the per-Core-generation shared secret.
pub const SECRET_BYTES: usize = 32;
/// Length in bytes of each single-use handshake nonce.
pub const NONCE_BYTES: usize = 32;
/// Default connection nonce lifetime in milliseconds.
pub const DEFAULT_NONCE_TTL_MS: u64 = 30_000;
/// Hard maximum connection nonce lifetime in milliseconds.
pub const MAX_NONCE_TTL_MS: u64 = 300_000;
/// Maximum number of Unicode scalar values in handshake identifiers.
pub const MAX_IDENTIFIER_CHARS: usize = 128;
/// Required Windows named-pipe prefix.
pub const PIPE_PREFIX: &str = r"\\.\pipe\";
/// Stem used for Core-owned named-pipe instances.
pub const PIPE_NAME_STEM: &str = "evohime-core-";

/// Roles a client may claim in the handshake. Every role is still subject to
/// Core's own capability and policy checks; the role only narrows what the
/// transport accepts.
pub const ALLOWED_CLIENT_ROLES: [&str; 3] = ["shell", "listener", "cli"];

/// Invalid secrets, identifiers, nonce limits, or secure random generation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SessionError {
    /// A secret or nonce is not hexadecimal of the required length.
    #[error("session value is not lowercase hex of the expected length")]
    MalformedValue,
    /// A pipe or client identity violates the identifier constraints.
    #[error("identifier is empty or exceeds the {MAX_IDENTIFIER_CHARS} character limit")]
    InvalidIdentifier,
    /// A configured nonce lifetime is zero or over the hard maximum.
    #[error("nonce time-to-live must be between 1 and {MAX_NONCE_TTL_MS} ms")]
    InvalidTtl,
    /// The operating system secure random source failed.
    #[error("secure random generation failed")]
    RandomFailure,
}

/// Why a handshake was refused. The reason is reported to the client as a
/// bounded protocol error and never leaks the expected secret or nonce.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone, Copy)]
pub enum HandshakeRejection {
    /// The client and Core use incompatible protocol major versions.
    #[error("protocol major versions are incompatible")]
    MajorMismatch,
    /// Client or peer identity data is malformed.
    #[error("client identity is malformed")]
    MalformedIdentity,
    /// The observed operating-system identity differs from the launch context.
    #[error("client identity does not match the launch context")]
    IdentityMismatch,
    /// The client role is not accepted by this transport.
    #[error("client role is not accepted on this transport")]
    UnknownRole,
    /// No unconsumed nonce was issued for the connection.
    #[error("no nonce was issued for this connection")]
    NonceUnavailable,
    /// The issued nonce expired before verification.
    #[error("the issued nonce expired")]
    NonceExpired,
    /// The client answered with a different nonce.
    #[error("the answered nonce does not match the issued one")]
    NonceMismatch,
    /// The HMAC proof does not match the expected proof.
    #[error("the authentication proof is invalid")]
    ProofMismatch,
    /// The client supplied too many capability names.
    #[error("capability list is not bounded")]
    UnboundedCapabilities,
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_hex_of_len(value: &str, bytes: usize) -> bool {
    value.len() == bytes * 2 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn random_hex(bytes: usize) -> Result<String, SessionError> {
    let mut buffer = vec![0_u8; bytes];
    getrandom::fill(&mut buffer).map_err(|_| SessionError::RandomFailure)?;
    Ok(hex_encode(&buffer))
}

/// Compares two ASCII values without an early exit on the first difference.
fn constant_time_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (a, b) in left.bytes().zip(right.bytes()) {
        difference |= a ^ b;
    }
    difference == 0
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

/// Long-lived shared secret for one Core generation.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionSecret(String);

impl SessionSecret {
    /// Generates a cryptographically random secret for one Core generation.
    pub fn generate() -> Result<Self, SessionError> {
        Ok(Self(random_hex(SECRET_BYTES)?))
    }

    /// Parses a hexadecimal secret of exactly [`SECRET_BYTES`] bytes.
    pub fn parse(value: &str) -> Result<Self, SessionError> {
        if !is_hex_of_len(value, SECRET_BYTES) {
            return Err(SessionError::MalformedValue);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    /// Only the launch-context writer may read the raw value back.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Computes the lowercase hex HMAC-SHA256 handshake proof.
    ///
    /// The value binds the role and client identifier to the issued nonce.
    /// The known-answer example uses a fixed test key, not a production secret.
    ///
    /// ```
    /// use evohime_desktop_ipc::session::{SessionSecret, NONCE_BYTES, SECRET_BYTES};
    /// let secret = SessionSecret::parse(&"ab".repeat(SECRET_BYTES)).unwrap();
    /// let nonce = "cd".repeat(NONCE_BYTES);
    /// assert_eq!(
    ///     secret.proof("shell", "shell-1", &nonce),
    ///     "736f6218169dbdeee94f2b5c92552114f4b4703bcbe96f6f06af1d66dc678c63",
    /// );
    /// ```
    pub fn proof(&self, role: &str, client_id: &str, nonce: &str) -> String {
        let message = format!("{role}\n{client_id}\n{nonce}");
        hex_encode(&hmac_sha256(self.0.as_bytes(), message.as_bytes()))
    }
}

/// Never prints the secret, so it cannot reach a log or a crash report.
impl std::fmt::Debug for SessionSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SessionSecret([REDACTED])")
    }
}

/// Single-use, time-bounded value issued by Core for one connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthNonce {
    /// Lowercase hexadecimal nonce value sent to the client.
    pub value: String,
    /// Expiry time in Unix milliseconds.
    pub expires_at_ms: u64,
}

/// Protected launch context handed from the supervisor to Core and to the
/// shell. `expected_user_sid` and `expected_logon_session` are empty only in an
/// explicitly enabled developer launch without a supervisor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchContext {
    /// Named pipe used to connect to this Core generation.
    pub pipe_name: String,
    /// Shared secret used to authenticate each client connection.
    pub secret: SessionSecret,
    #[serde(default)]
    /// Expected Windows user SID; empty only for an explicit developer launch.
    pub expected_user_sid: String,
    #[serde(default)]
    /// Expected logon session identity, when enforced by the supervisor.
    pub expected_logon_session: String,
    #[serde(default)]
    /// Time the supervisor created this context in Unix milliseconds.
    pub issued_at_ms: u64,
    #[serde(default)]
    /// Process identifier of the supervisor that launched Core.
    pub supervisor_pid: u32,
    #[serde(default)]
    /// Named event used to detect supervisor liveness.
    pub supervisor_liveness_event: String,
    #[serde(default)]
    /// Optional authenticated supervisor control pipe.
    pub supervisor_pipe_name: Option<String>,
    #[serde(default)]
    /// Optional secret authenticating the supervisor control pipe.
    pub supervisor_secret: Option<SessionSecret>,
}

impl LaunchContext {
    /// Generates a Core pipe name and secret for the supplied host identity.
    pub fn generate(
        expected_user_sid: String,
        expected_logon_session: String,
        issued_at_ms: u64,
    ) -> Result<Self, SessionError> {
        Ok(Self {
            pipe_name: generate_pipe_name()?,
            secret: SessionSecret::generate()?,
            expected_user_sid,
            expected_logon_session,
            issued_at_ms,
            supervisor_pid: 0,
            supervisor_liveness_event: String::new(),
            supervisor_pipe_name: None,
            supervisor_secret: None,
        })
    }

    /// Validates pipe names, secret encoding, and bounded identity metadata.
    pub fn validate(&self) -> Result<(), SessionError> {
        validate_pipe_name(&self.pipe_name)?;
        SessionSecret::parse(self.secret.expose())?;
        for identifier in [&self.expected_user_sid, &self.expected_logon_session] {
            if identifier.chars().count() > MAX_IDENTIFIER_CHARS {
                return Err(SessionError::InvalidIdentifier);
            }
        }
        if !self.supervisor_liveness_event.is_empty()
            && (self.supervisor_liveness_event.chars().count() > MAX_IDENTIFIER_CHARS
                || !self.supervisor_liveness_event.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '\\')
                }))
        {
            return Err(SessionError::InvalidIdentifier);
        }
        if let Some(pipe) = &self.supervisor_pipe_name {
            validate_pipe_name(pipe)?;
        }
        if let Some(secret) = &self.supervisor_secret {
            SessionSecret::parse(secret.expose())?;
        }
        Ok(())
    }

    /// True when the context binds the connection to a specific Windows user
    /// and logon session; an explicit developer launch does not.
    pub fn is_authenticated(&self) -> bool {
        !self.expected_user_sid.is_empty()
    }
}

/// `\\.\pipe\evohime-core-<hex>`; the name is unpredictable, but Core never
/// relies on its secrecy — the DACL and this handshake do the work.
pub fn generate_pipe_name() -> Result<String, SessionError> {
    Ok(format!("{PIPE_PREFIX}{PIPE_NAME_STEM}{}", random_hex(16)?))
}

/// Generates an unpredictable named pipe for supervisor control traffic.
pub fn generate_supervisor_pipe_name() -> Result<String, SessionError> {
    Ok(format!(
        "{PIPE_PREFIX}evohime-supervisor-{}",
        random_hex(16)?
    ))
}

/// Checks the required named-pipe prefix and safe bounded suffix characters.
pub fn validate_pipe_name(value: &str) -> Result<(), SessionError> {
    let Some(name) = value.strip_prefix(PIPE_PREFIX) else {
        return Err(SessionError::InvalidIdentifier);
    };
    if name.is_empty()
        || name.chars().count() > MAX_IDENTIFIER_CHARS
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(SessionError::InvalidIdentifier);
    }
    Ok(())
}

/// Reads a launch context from the protected runtime directory. The caller is
/// responsible for having created that directory with an owner-only DACL; the
/// context is validated before it is returned so a tampered file cannot widen
/// the pipe name or shorten the secret.
pub fn read_launch_context(path: &std::path::Path) -> Result<LaunchContext, std::io::Error> {
    let bytes = std::fs::read(path)?;
    let context: LaunchContext =
        serde_json::from_slice(&bytes).map_err(|error| std::io::Error::other(error.to_string()))?;
    context
        .validate()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(context)
}

/// Writes a launch context, replacing any previous generation's file.
pub fn write_launch_context(
    path: &std::path::Path,
    context: &LaunchContext,
) -> Result<(), std::io::Error> {
    context
        .validate()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let bytes =
        serde_json::to_vec(context).map_err(|error| std::io::Error::other(error.to_string()))?;
    std::fs::write(path, bytes)
}

/// Identity of the connected client as observed by the operating system, not
/// as claimed by the client itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PeerIdentity {
    /// User SID observed by the operating system for the connected peer.
    pub user_sid: String,
    /// Logon-session identity observed for the connected peer.
    pub logon_session: String,
}

/// Everything the transport needs to judge one handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeRequest {
    /// Protocol major version advertised by the client.
    pub protocol_major: u32,
    /// Stable client identifier included in the proof.
    pub client_id: String,
    /// Client role included in the proof and checked against the allowlist.
    pub client_role: String,
    /// Single-use nonce issued by Core for this connection.
    pub nonce: String,
    /// HMAC proof over role, client id, and nonce.
    pub proof: String,
    /// Bounded capability names offered by the client.
    pub capabilities: Vec<String>,
    /// Host-observed identity used to bind authentication to the launch context.
    pub peer: PeerIdentity,
}

/// Client identity accepted by the handshake verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedHandshake {
    /// Verified client identifier.
    pub client_id: String,
    /// Verified client role.
    pub client_role: String,
}

/// Issues nonces and verifies handshakes for one Core generation.
#[derive(Debug)]
pub struct HandshakeVerifier {
    context: LaunchContext,
    nonce_ttl_ms: u64,
    issued: Option<AuthNonce>,
}

impl HandshakeVerifier {
    /// Creates a verifier for one launch context and bounded nonce lifetime.
    pub fn new(context: LaunchContext, nonce_ttl_ms: u64) -> Result<Self, SessionError> {
        context.validate()?;
        if nonce_ttl_ms == 0 || nonce_ttl_ms > MAX_NONCE_TTL_MS {
            return Err(SessionError::InvalidTtl);
        }
        Ok(Self {
            context,
            nonce_ttl_ms,
            issued: None,
        })
    }

    /// Returns the Core pipe name covered by this verifier.
    pub fn pipe_name(&self) -> &str {
        &self.context.pipe_name
    }

    /// Issues the nonce for a new connection, discarding any nonce that a
    /// previous connection left unanswered.
    pub fn issue_nonce(&mut self, now_ms: u64) -> Result<AuthNonce, SessionError> {
        let nonce = AuthNonce {
            value: random_hex(NONCE_BYTES)?,
            expires_at_ms: now_ms.saturating_add(self.nonce_ttl_ms),
        };
        self.issued = Some(nonce.clone());
        Ok(nonce)
    }

    /// Verifies one handshake and consumes the issued nonce, so the same
    /// nonce is never accepted twice — not even by a second connection.
    pub fn verify(
        &mut self,
        request: &HandshakeRequest,
        now_ms: u64,
    ) -> Result<VerifiedHandshake, HandshakeRejection> {
        let issued = self
            .issued
            .take()
            .ok_or(HandshakeRejection::NonceUnavailable)?;

        if request.protocol_major != crate::ProtocolVersion::new(1, 0).major {
            return Err(HandshakeRejection::MajorMismatch);
        }
        if request.client_id.is_empty()
            || request.client_id.chars().count() > MAX_IDENTIFIER_CHARS
            || request
                .client_id
                .chars()
                .any(|character| character.is_control())
        {
            return Err(HandshakeRejection::MalformedIdentity);
        }
        if !ALLOWED_CLIENT_ROLES.contains(&request.client_role.as_str()) {
            return Err(HandshakeRejection::UnknownRole);
        }
        if request.capabilities.len() > crate::MAX_CAPABILITIES {
            return Err(HandshakeRejection::UnboundedCapabilities);
        }
        if now_ms > issued.expires_at_ms {
            return Err(HandshakeRejection::NonceExpired);
        }
        if !constant_time_eq(&request.nonce, &issued.value) {
            return Err(HandshakeRejection::NonceMismatch);
        }
        if self.context.is_authenticated() {
            if request.peer.user_sid.is_empty() {
                return Err(HandshakeRejection::MalformedIdentity);
            }
            if !constant_time_eq(&request.peer.user_sid, &self.context.expected_user_sid) {
                return Err(HandshakeRejection::IdentityMismatch);
            }
            if !self.context.expected_logon_session.is_empty()
                && !constant_time_eq(
                    &request.peer.logon_session,
                    &self.context.expected_logon_session,
                )
            {
                return Err(HandshakeRejection::IdentityMismatch);
            }
        }

        let expected =
            self.context
                .secret
                .proof(&request.client_role, &request.client_id, &issued.value);
        if !constant_time_eq(&request.proof, &expected) {
            return Err(HandshakeRejection::ProofMismatch);
        }

        Ok(VerifiedHandshake {
            client_id: request.client_id.clone(),
            client_role: request.client_role.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> LaunchContext {
        LaunchContext {
            pipe_name: format!("{PIPE_PREFIX}{PIPE_NAME_STEM}0123456789abcdef"),
            secret: SessionSecret::parse(&"ab".repeat(SECRET_BYTES)).expect("secret"),
            expected_user_sid: "S-1-5-21-1-2-3-1001".into(),
            expected_logon_session: "0:123456".into(),
            issued_at_ms: 1_000,
            supervisor_pid: 0,
            supervisor_liveness_event: String::new(),
            supervisor_pipe_name: None,
            supervisor_secret: None,
        }
    }

    fn request(verifier: &HandshakeVerifier, nonce: &str) -> HandshakeRequest {
        let secret = SessionSecret::parse(verifier.context.secret.expose()).expect("secret");
        HandshakeRequest {
            protocol_major: 1,
            client_id: "shell-1".into(),
            client_role: "shell".into(),
            nonce: nonce.to_string(),
            proof: secret.proof("shell", "shell-1", nonce),
            capabilities: vec!["replay".into()],
            peer: PeerIdentity {
                user_sid: "S-1-5-21-1-2-3-1001".into(),
                logon_session: "0:123456".into(),
            },
        }
    }

    #[test]
    fn accepts_a_correct_proof_once() {
        let mut verifier =
            HandshakeVerifier::new(context(), DEFAULT_NONCE_TTL_MS).expect("verifier");
        let nonce = verifier.issue_nonce(1_000).expect("nonce");
        let request = request(&verifier, &nonce.value);

        let verified = verifier
            .verify(&request, 1_100)
            .expect("handshake accepted");
        assert_eq!(verified.client_role, "shell");
        // The same nonce cannot be replayed by a second connection.
        assert_eq!(
            verifier.verify(&request, 1_100),
            Err(HandshakeRejection::NonceUnavailable)
        );
    }

    #[test]
    fn rejects_expired_and_mismatched_nonces() {
        let mut verifier = HandshakeVerifier::new(context(), 1_000).expect("verifier");
        let nonce = verifier.issue_nonce(1_000).expect("nonce");
        assert_eq!(
            verifier.verify(&request(&verifier, &nonce.value), 5_000),
            Err(HandshakeRejection::NonceExpired)
        );

        let nonce = verifier.issue_nonce(1_000).expect("nonce");
        let mut foreign = request(&verifier, &nonce.value);
        foreign.nonce = "f".repeat(NONCE_BYTES * 2);
        assert_eq!(
            verifier.verify(&foreign, 1_100),
            Err(HandshakeRejection::NonceMismatch)
        );
    }

    #[test]
    fn rejects_a_wrong_secret() {
        let mut verifier =
            HandshakeVerifier::new(context(), DEFAULT_NONCE_TTL_MS).expect("verifier");
        let nonce = verifier.issue_nonce(1_000).expect("nonce");
        let mut forged = request(&verifier, &nonce.value);
        forged.proof = SessionSecret::parse(&"cd".repeat(SECRET_BYTES))
            .expect("secret")
            .proof("shell", "shell-1", &nonce.value);
        assert_eq!(
            verifier.verify(&forged, 1_100),
            Err(HandshakeRejection::ProofMismatch)
        );
    }

    #[test]
    fn proof_is_bound_to_role_and_client_id() {
        let mut verifier =
            HandshakeVerifier::new(context(), DEFAULT_NONCE_TTL_MS).expect("verifier");
        let nonce = verifier.issue_nonce(1_000).expect("nonce");
        let mut swapped = request(&verifier, &nonce.value);
        swapped.client_role = "listener".into();
        assert_eq!(
            verifier.verify(&swapped, 1_100),
            Err(HandshakeRejection::ProofMismatch)
        );
    }

    #[test]
    fn rejects_foreign_identity_and_unknown_role() {
        let mut verifier =
            HandshakeVerifier::new(context(), DEFAULT_NONCE_TTL_MS).expect("verifier");
        let nonce = verifier.issue_nonce(1_000).expect("nonce");
        let mut foreign = request(&verifier, &nonce.value);
        foreign.peer.user_sid = "S-1-5-21-9-9-9-500".into();
        assert_eq!(
            verifier.verify(&foreign, 1_100),
            Err(HandshakeRejection::IdentityMismatch)
        );

        let nonce = verifier.issue_nonce(1_000).expect("nonce");
        let mut unknown_role = request(&verifier, &nonce.value);
        unknown_role.client_role = "diagnostics".into();
        assert_eq!(
            verifier.verify(&unknown_role, 1_100),
            Err(HandshakeRejection::UnknownRole)
        );
    }

    #[test]
    fn rejects_major_mismatch_and_malformed_identity() {
        let mut verifier =
            HandshakeVerifier::new(context(), DEFAULT_NONCE_TTL_MS).expect("verifier");
        let nonce = verifier.issue_nonce(1_000).expect("nonce");
        let mut wrong_major = request(&verifier, &nonce.value);
        wrong_major.protocol_major = 2;
        assert_eq!(
            verifier.verify(&wrong_major, 1_100),
            Err(HandshakeRejection::MajorMismatch)
        );

        let nonce = verifier.issue_nonce(1_000).expect("nonce");
        let mut malformed = request(&verifier, &nonce.value);
        malformed.client_id = "shell\n1".into();
        assert_eq!(
            verifier.verify(&malformed, 1_100),
            Err(HandshakeRejection::MalformedIdentity)
        );
    }

    #[test]
    fn a_developer_context_skips_identity_binding() {
        let mut developer = context();
        developer.expected_user_sid = String::new();
        developer.expected_logon_session = String::new();
        assert!(!developer.is_authenticated());

        let mut verifier =
            HandshakeVerifier::new(developer, DEFAULT_NONCE_TTL_MS).expect("verifier");
        let nonce = verifier.issue_nonce(1_000).expect("nonce");
        let mut anonymous = request(&verifier, &nonce.value);
        anonymous.peer = PeerIdentity::default();
        assert!(verifier.verify(&anonymous, 1_100).is_ok());
    }

    #[test]
    fn generated_material_is_unpredictable_and_well_formed() {
        let first = LaunchContext::generate("S-1-5-21".into(), "0:1".into(), 1).expect("context");
        let second = LaunchContext::generate("S-1-5-21".into(), "0:1".into(), 1).expect("context");
        assert_ne!(first.pipe_name, second.pipe_name);
        assert_ne!(first.secret.expose(), second.secret.expose());
        first.validate().expect("generated context is valid");
        assert!(format!("{:?}", first.secret).contains("REDACTED"));
    }

    #[test]
    fn launch_context_round_trips_through_a_file() {
        let path = std::env::temp_dir().join(format!(
            "evohime-launch-context-{}.json",
            std::process::id()
        ));
        let original = context();
        write_launch_context(&path, &original).expect("context writes");
        let restored = read_launch_context(&path).expect("context reads");
        assert_eq!(restored, original);

        std::fs::write(&path, b"{\"pipe_name\":\"nope\"}").expect("tampered write");
        assert!(read_launch_context(&path).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_malformed_pipe_names_and_secrets() {
        assert_eq!(
            validate_pipe_name(r"\\attacker\pipe\evohime-core-1"),
            Err(SessionError::InvalidIdentifier)
        );
        assert_eq!(
            validate_pipe_name(&format!("{PIPE_PREFIX}evohime core")),
            Err(SessionError::InvalidIdentifier)
        );
        assert_eq!(
            SessionSecret::parse("abc"),
            Err(SessionError::MalformedValue)
        );
    }

    /// Pins the proof derivation across the Rust and Electron implementations.
    #[test]
    fn proof_matches_the_shared_rust_electron_vector() {
        let secret = SessionSecret::parse(&"ab".repeat(SECRET_BYTES)).expect("secret");
        assert_eq!(
            secret.proof("shell", "shell-1", &"cd".repeat(NONCE_BYTES)),
            "736f6218169dbdeee94f2b5c92552114f4b4703bcbe96f6f06af1d66dc678c63"
        );
    }

    #[test]
    fn hmac_matches_a_published_rfc4231_vector() {
        // RFC 4231 test case 1: key = 0x0b * 20, data = "Hi There".
        let digest = hmac_sha256(&[0x0b; 20], b"Hi There");
        assert_eq!(
            hex_encode(&digest),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }
}
