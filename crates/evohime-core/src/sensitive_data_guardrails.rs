//! Core-owned, bounded sensitive-data detection and redaction.
//!
//! This module deliberately has no network, filesystem, or provider dependency.
//! It is used at every less-trusted boundary; callers receive the transformed
//! value and bounded metadata, never the detector's raw match values.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current schema version for sensitive-data policies and metadata.
pub const CONTRACT_VERSION: u32 = 1;
/// Version of the deterministic detector pack used by the PII boundary.
pub const DETECTOR_PACK_VERSION: u32 = 1;
/// Maximum input size scanned by one redaction operation.
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
/// Maximum nested JSON depth traversed by structured redaction.
pub const MAX_JSON_DEPTH: usize = 16;
/// Maximum JSON values visited by structured redaction.
pub const MAX_JSON_NODES: usize = 512;
/// Number of trailing characters retained between streaming chunks.
pub const STREAM_CARRY_CHARS: usize = 96;

/// Built-in sensitive value detector used by a redaction rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Detector {
    /// Detects email-like addresses.
    Email,
    /// Detects provider-style secret tokens.
    SecretToken,
    /// Detects bearer authorization tokens.
    BearerToken,
    /// Detects private-key material that must be blocked.
    PrivateKey,
    /// Detects a labelled or E.164 phone number.
    Phone,
    /// Detects a payment-card number with a valid Luhn checksum.
    PaymentCard,
    /// Detects an IBAN with a valid country prefix and mod-97 checksum.
    Iban,
    /// Detects a narrowly contextual national/tax identifier.
    NationalTaxId,
}

/// Bounded category of a finding. Raw values and spans are intentionally absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitiveEntityKind {
    /// Email address.
    Email,
    /// Secret or bearer credential.
    Credential,
    /// Private key material.
    PrivateKey,
    /// Telephone number.
    Phone,
    /// Payment-card number.
    PaymentCard,
    /// International bank account number.
    Iban,
    /// Contextually detected national/tax number.
    NationalTaxId,
}

/// Confidence assigned by a local deterministic detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingConfidence {
    /// Structural and checksum/context evidence is present.
    High,
    /// Structural evidence is present but no checksum is available.
    Medium,
}

/// Metadata-only finding returned by classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensitiveFinding {
    /// Entity category, never the matched value.
    pub kind: SensitiveEntityKind,
    /// Detector pack version that produced the finding.
    pub detector_version: u32,
    /// Bounded confidence.
    pub confidence: FindingConfidence,
}

/// Stable classification summary keyed by the exact payload digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassificationSummary {
    /// Detector pack version.
    pub detector_pack_version: u32,
    /// SHA-256 of the exact input bytes.
    pub payload_hash: String,
    /// Unique, bounded metadata findings.
    pub findings: Vec<SensitiveFinding>,
    /// Derived minimum privacy required before route selection.
    pub required_privacy: String,
    /// Whether the detector completed without exceeding a bound.
    pub complete: bool,
}

/// Destination class used by the final egress decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationClass {
    /// Core-local processing.
    LocalTrusted,
    /// A configured provider or tool boundary.
    Provider,
    /// An external agent or remote service.
    External,
    /// A command process whose network isolation is not guaranteed.
    Command,
}

/// Terminal disposition of an exact-payload egress decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EgressDisposition {
    /// Exact payload may be dispatched.
    Allow,
    /// Caller must obtain an approval bound to all hashes.
    RequiresApproval,
    /// Payload was transformed and must be dispatched only in its new form.
    Transformed,
    /// Dispatch is denied.
    Blocked,
    /// Detector/policy data was unavailable or exceeded a bound.
    Unavailable,
    /// Previously scanned payload no longer matches.
    PayloadChanged,
}

/// Metadata-only binding for a final egress decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EgressDecision {
    /// Terminal disposition.
    pub disposition: EgressDisposition,
    /// Exact original payload hash.
    pub payload_hash: String,
    /// Hash of transformed payload, when applicable.
    pub transformed_payload_hash: Option<String>,
    /// Route/destination reference, never a credential or URL with credentials.
    pub destination: String,
    /// Policy and detector references.
    pub policy_hash: String,
    /// Detector pack version.
    pub detector_pack_version: u32,
    /// Entity categories and counts, without raw matches.
    pub findings: Vec<SensitiveFinding>,
    /// Stable reason code.
    pub reason_code: String,
}

/// Transformation applied when a detector finds a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Replaces the value with a rule-labelled marker.
    Redact,
    /// Preserves only a small portion of the value.
    Mask,
    /// Replaces the value with a one-way digest.
    Hash,
    /// Rejects the operation without returning the value.
    Block,
}

impl Action {
    fn rank(self) -> u8 {
        match self {
            Self::Block => 4,
            Self::Hash => 3,
            Self::Mask => 2,
            Self::Redact => 1,
        }
    }
}

/// Versioned rule binding one detector to one enforcement action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensitiveDataRule {
    /// Stable rule identifier included in redaction metadata.
    pub id: String,
    /// Rule schema version.
    pub version: u32,
    /// Sensitive-data pattern to detect.
    pub detector: Detector,
    /// Transformation or rejection to apply to a match.
    pub action: Action,
}

/// Destination-specific limits and rules for a redaction boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// Policy schema version.
    pub version: u32,
    /// Boundary receiving the transformed data.
    pub destination: String,
    /// Ordered detector rules applied to input values.
    pub rules: Vec<SensitiveDataRule>,
    /// Maximum input size in bytes.
    pub max_input_bytes: usize,
    /// Maximum structured JSON nesting depth.
    pub max_json_depth: usize,
    /// Maximum structured JSON nodes traversed.
    pub max_json_nodes: usize,
}

/// Validated policy paired with its stable integrity digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySnapshot {
    /// Policy enforced by the redaction operation.
    pub policy: Policy,
    /// Digest used to identify the exact policy revision.
    pub policy_hash: String,
}

/// Bounded audit metadata returned without exposing matched raw values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionMetadata {
    /// Guardrail contract version used for this operation.
    pub contract_version: u32,
    /// Digest of the policy applied to the value.
    pub policy_hash: String,
    /// Destination for which the value was prepared.
    pub destination: String,
    /// Strongest action applied to any match, if a match occurred.
    pub action: Option<Action>,
    /// Identifiers of rules that matched.
    pub rule_ids: Vec<String>,
    /// Number of non-overlapping matches transformed.
    pub match_count: usize,
    /// Whether policy blocked the value from being returned.
    pub blocked: bool,
    /// Encoded length of the transformed output.
    pub output_bytes: usize,
}

/// Transformed text and metadata describing the redaction performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionResult {
    /// Redacted, masked, or hashed output text.
    pub value: String,
    /// Policy and action metadata without raw matched values.
    pub metadata: RedactionMetadata,
}

/// Invalid policy, oversized input, malformed structure, or blocked data.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GuardrailError {
    /// Policy or metadata uses an unsupported guardrail version.
    #[error("unsupported guardrail contract version")]
    UnsupportedVersion,
    /// Policy fields or detector rules are invalid.
    #[error("invalid guardrail policy")]
    InvalidPolicy,
    /// Text or serialized structured input exceeds its configured bound.
    #[error("guardrail input exceeds bound")]
    InputTooLarge,
    /// JSON depth or node count exceeds its traversal bound.
    #[error("structured payload exceeds traversal bound")]
    StructuredPayloadTooLarge,
    /// A matched detector uses the blocking action.
    #[error("sensitive data blocked by policy")]
    Blocked(RedactionMetadata),
    /// Structured input could not be parsed or transformed.
    #[error("malformed structured payload")]
    MalformedStructuredPayload,
}

/// Creates the default detector set for a named destination.
pub fn default_policy(destination: impl Into<String>) -> PolicySnapshot {
    let policy = Policy {
        version: CONTRACT_VERSION,
        destination: destination.into(),
        rules: vec![
            SensitiveDataRule {
                id: "email".into(),
                version: 1,
                detector: Detector::Email,
                action: Action::Mask,
            },
            SensitiveDataRule {
                id: "secret_token".into(),
                version: 1,
                detector: Detector::SecretToken,
                action: Action::Hash,
            },
            SensitiveDataRule {
                id: "bearer_token".into(),
                version: 1,
                detector: Detector::BearerToken,
                action: Action::Redact,
            },
            SensitiveDataRule {
                id: "private_key".into(),
                version: 1,
                detector: Detector::PrivateKey,
                action: Action::Block,
            },
            SensitiveDataRule {
                id: "phone".into(),
                version: 1,
                detector: Detector::Phone,
                action: Action::Mask,
            },
            SensitiveDataRule {
                id: "payment_card".into(),
                version: 1,
                detector: Detector::PaymentCard,
                action: Action::Mask,
            },
            SensitiveDataRule {
                id: "iban".into(),
                version: 1,
                detector: Detector::Iban,
                action: Action::Mask,
            },
            SensitiveDataRule {
                id: "national_tax_id".into(),
                version: 1,
                detector: Detector::NationalTaxId,
                action: Action::Mask,
            },
        ],
        max_input_bytes: MAX_INPUT_BYTES,
        max_json_depth: MAX_JSON_DEPTH,
        max_json_nodes: MAX_JSON_NODES,
    };
    match snapshot(policy.clone()) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            tracing::error!(%error, "default guardrail policy is invalid");
            PolicySnapshot {
                policy,
                policy_hash: String::new(),
            }
        }
    }
}

/// Computes the immutable hash of the active detector pack.
pub fn detector_pack_hash() -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"evohime-sensitive-detector-pack-v1\0");
    for detector in [
        Detector::Email,
        Detector::SecretToken,
        Detector::BearerToken,
        Detector::PrivateKey,
        Detector::Phone,
        Detector::PaymentCard,
        Detector::Iban,
        Detector::NationalTaxId,
    ] {
        hasher.update(format!("{detector:?};").as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Classifies bounded text without returning values, offsets, or context.
///
/// The summary is safe to persist and is keyed by the exact payload hash. A
/// detector never authorizes a destructive transform unless its structural or
/// checksum evidence is sufficient.
pub fn classify_text(input: &str) -> Result<ClassificationSummary, GuardrailError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(GuardrailError::InputTooLarge);
    }
    let mut findings = Vec::new();
    let mut add = |kind: SensitiveEntityKind, confidence: FindingConfidence| {
        if !findings
            .iter()
            .any(|finding: &SensitiveFinding| finding.kind == kind)
        {
            findings.push(SensitiveFinding {
                kind,
                detector_version: DETECTOR_PACK_VERSION,
                confidence,
            });
        }
    };
    if !find_matches(input, Detector::Email).is_empty() {
        add(SensitiveEntityKind::Email, FindingConfidence::High);
    }
    if !find_matches(input, Detector::SecretToken).is_empty()
        || !find_matches(input, Detector::BearerToken).is_empty()
    {
        add(SensitiveEntityKind::Credential, FindingConfidence::High);
    }
    if !find_matches(input, Detector::PrivateKey).is_empty() {
        add(SensitiveEntityKind::PrivateKey, FindingConfidence::High);
    }
    if has_phone(input) {
        add(SensitiveEntityKind::Phone, FindingConfidence::High);
    }
    if has_payment_card(input) {
        add(SensitiveEntityKind::PaymentCard, FindingConfidence::High);
    }
    if has_iban(input) {
        add(SensitiveEntityKind::Iban, FindingConfidence::High);
    }
    if has_contextual_tax_id(input) {
        add(
            SensitiveEntityKind::NationalTaxId,
            FindingConfidence::Medium,
        );
    }
    let required_privacy = if findings.iter().any(|finding| {
        matches!(
            finding.kind,
            SensitiveEntityKind::Credential | SensitiveEntityKind::PrivateKey
        )
    }) {
        "restricted"
    } else if findings.is_empty() {
        "public"
    } else {
        "sensitive"
    };
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    Ok(ClassificationSummary {
        detector_pack_version: DETECTOR_PACK_VERSION,
        payload_hash: hex::encode(hasher.finalize()),
        findings,
        required_privacy: required_privacy.into(),
        complete: true,
    })
}

/// Raises an upstream model-gateway privacy requirement monotonically.
pub fn required_privacy(summary: &ClassificationSummary) -> evohime_model_gateway::PrivacyClass {
    match summary.required_privacy.as_str() {
        "restricted" => evohime_model_gateway::PrivacyClass::Restricted,
        "sensitive" => evohime_model_gateway::PrivacyClass::Sensitive,
        "internal" => evohime_model_gateway::PrivacyClass::Internal,
        _ => evohime_model_gateway::PrivacyClass::Public,
    }
}

/// Scans the exact serialized payload immediately before an external effect.
/// The returned binding changes whenever payload, policy, or detector output
/// changes; callers must not dispatch any other payload using this decision.
pub fn evaluate_egress(
    snapshot: &PolicySnapshot,
    destination: DestinationClass,
    route_ref: &str,
    payload: &str,
) -> Result<(EgressDecision, Option<String>), GuardrailError> {
    let summary = classify_text(payload)?;
    let result = match redact_text(snapshot, payload) {
        Ok(result) => result,
        Err(GuardrailError::Blocked(_metadata)) => {
            return Ok((
                decision(
                    &summary,
                    EgressDisposition::Blocked,
                    route_ref,
                    snapshot,
                    None,
                    "blocking_entity",
                ),
                None,
            ));
        }
        Err(error) => return Err(error),
    };
    let transformed = result.value != payload;
    let transformed_hash = if transformed {
        Some(sha256(&result.value))
    } else {
        None
    };
    let disposition = if destination == DestinationClass::Command && !summary.findings.is_empty() {
        EgressDisposition::RequiresApproval
    } else if destination == DestinationClass::External
        && summary
            .findings
            .iter()
            .any(|finding| finding.kind == SensitiveEntityKind::Credential)
    {
        EgressDisposition::RequiresApproval
    } else if transformed {
        EgressDisposition::Transformed
    } else {
        EgressDisposition::Allow
    };
    let decision = decision(
        &summary,
        disposition,
        route_ref,
        snapshot,
        transformed_hash,
        if transformed {
            "payload_transformed"
        } else {
            "no_finding"
        },
    );
    Ok((decision, transformed.then_some(result.value)))
}

fn decision(
    summary: &ClassificationSummary,
    disposition: EgressDisposition,
    destination: &str,
    snapshot: &PolicySnapshot,
    transformed_payload_hash: Option<String>,
    reason_code: &str,
) -> EgressDecision {
    EgressDecision {
        disposition,
        payload_hash: summary.payload_hash.clone(),
        transformed_payload_hash,
        destination: destination.to_owned(),
        policy_hash: snapshot.policy_hash.clone(),
        detector_pack_version: summary.detector_pack_version,
        findings: summary.findings.clone(),
        reason_code: reason_code.to_owned(),
    }
}

fn digits(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}

fn has_phone(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    let labelled = lower.contains("phone") || lower.contains("телефон") || lower.contains("mobile");
    for (start, character) in input.char_indices() {
        if character != '+' && !character.is_ascii_digit() {
            continue;
        }
        let tail = &input[start..];
        let candidate: String = tail
            .chars()
            .take_while(|value| {
                value.is_ascii_digit() || matches!(value, '+' | '-' | '(' | ')' | '.' | ' ' | '\t')
            })
            .collect();
        let number = digits(&candidate);
        if (10..=15).contains(&number.len())
            && (candidate.starts_with('+')
                || (labelled
                    && lower[..start]
                        .chars()
                        .rev()
                        .take(32)
                        .collect::<String>()
                        .contains("enohp"))
                || (labelled
                    && lower[..start]
                        .chars()
                        .rev()
                        .take(64)
                        .collect::<String>()
                        .contains("нофелет")))
        {
            return true;
        }
    }
    false
}

fn has_payment_card(input: &str) -> bool {
    for (start, character) in input.char_indices() {
        if !character.is_ascii_digit() {
            continue;
        }
        let candidate: String = input[start..]
            .chars()
            .take_while(|value| {
                value.is_ascii_digit() || matches!(value, '-' | ' ' | '\t' | '(' | ')')
            })
            .collect();
        let number = digits(&candidate);
        if (13..=19).contains(&number.len()) && luhn_valid(&number) {
            return true;
        }
    }
    false
}

fn luhn_valid(value: &str) -> bool {
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let mut sum = 0u32;
    let mut double = false;
    for digit in value.bytes().rev().map(|byte| byte - b'0') {
        let mut value = u32::from(digit);
        if double {
            value *= 2;
            if value > 9 {
                value -= 9;
            }
        }
        sum += value;
        double = !double;
    }
    value.len() >= 13 && sum % 10 == 0
}

fn has_iban(input: &str) -> bool {
    input
        .split(|character: char| {
            character.is_whitespace() || matches!(character, ',' | ';' | '(' | ')')
        })
        .any(|part| {
            let compact: String = part
                .chars()
                .filter(|character| character.is_ascii_alphanumeric())
                .collect();
            (15..=34).contains(&compact.len())
                && compact.len() >= 4
                && compact[..2]
                    .chars()
                    .all(|character| character.is_ascii_uppercase())
                && compact[2..4]
                    .chars()
                    .all(|character| character.is_ascii_digit())
                && iban_mod97(&compact) == 1
        })
}

fn iban_mod97(value: &str) -> u32 {
    let rotated = format!("{}{}", &value[4..], &value[..4]);
    rotated.chars().fold(0u32, |remainder, character| {
        let digits = if character.is_ascii_digit() {
            character.to_digit(10).unwrap_or(0)
        } else {
            u32::from(character as u8 - b'A') + 10
        };
        (remainder * 10 + digits) % 97
    })
}

fn has_contextual_tax_id(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    let labelled = ["tax", "vat", "inn", "tin", "налог", "инн"]
        .iter()
        .any(|label| lower.contains(label));
    labelled
        && input
            .split(|character: char| !character.is_ascii_digit())
            .any(|part| (8..=14).contains(&part.len()))
}

/// Validates a policy and computes its versioned integrity digest.
pub fn snapshot(policy: Policy) -> Result<PolicySnapshot, GuardrailError> {
    if policy.version != CONTRACT_VERSION
        || policy.destination.is_empty()
        || policy.destination.len() > 128
        || policy.rules.is_empty()
        || policy.rules.len() > 32
        || policy.max_input_bytes == 0
        || policy.max_input_bytes > MAX_INPUT_BYTES
        || policy.max_json_depth == 0
        || policy.max_json_depth > MAX_JSON_DEPTH
        || policy.max_json_nodes == 0
        || policy.max_json_nodes > MAX_JSON_NODES
        || policy.rules.iter().any(|rule| {
            rule.version != CONTRACT_VERSION || rule.id.is_empty() || rule.id.len() > 64
        })
    {
        return Err(GuardrailError::InvalidPolicy);
    }
    let bytes = serde_json::to_vec(&policy).map_err(|_| GuardrailError::InvalidPolicy)?;
    let mut hasher = Sha256::new();
    hasher.update(b"evohime-sensitive-guardrails-v1\0");
    hasher.update(bytes);
    Ok(PolicySnapshot {
        policy,
        policy_hash: hex::encode(hasher.finalize()),
    })
}

/// Applies text detectors and returns transformed content with bounded metadata.
pub fn redact_text(
    snapshot: &PolicySnapshot,
    input: &str,
) -> Result<RedactionResult, GuardrailError> {
    if input.len() > snapshot.policy.max_input_bytes {
        return Err(GuardrailError::InputTooLarge);
    }
    let mut matches = Vec::new();
    for rule in &snapshot.policy.rules {
        for (start, end) in find_matches(input, rule.detector) {
            matches.push((start, end, rule));
        }
    }
    matches.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| b.2.action.rank().cmp(&a.2.action.rank()))
    });
    let mut selected = Vec::new();
    for candidate in matches {
        if selected
            .iter()
            .any(|(start, end, _): &(usize, usize, &SensitiveDataRule)| {
                candidate.0 < *end && candidate.1 > *start
            })
        {
            continue;
        }
        selected.push(candidate);
    }
    selected.sort_by_key(|item| item.0);
    let action = selected
        .iter()
        .map(|item| item.2.action)
        .max_by_key(|action| action.rank());
    let rule_ids = selected
        .iter()
        .map(|item| item.2.id.clone())
        .collect::<Vec<_>>();
    let match_count = selected.len();
    let metadata = |blocked: bool, output_bytes: usize| RedactionMetadata {
        contract_version: CONTRACT_VERSION,
        policy_hash: snapshot.policy_hash.clone(),
        destination: snapshot.policy.destination.clone(),
        action,
        rule_ids: rule_ids.clone(),
        match_count,
        blocked,
        output_bytes,
    };
    if selected.iter().any(|item| item.2.action == Action::Block) {
        return Err(GuardrailError::Blocked(metadata(true, 0)));
    }
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    for (start, end, rule) in selected {
        output.push_str(&input[cursor..start]);
        let value = &input[start..end];
        match rule.action {
            Action::Redact => output.push_str(&format!("<redacted:{}>", rule.id)),
            Action::Mask => output.push_str(&mask(value)),
            Action::Hash => output.push_str(&format!("<sha256:{}>", sha256(value))),
            Action::Block => unreachable!(),
        }
        cursor = end;
    }
    output.push_str(&input[cursor..]);
    let output_bytes = output.len();
    let result = RedactionResult {
        value: output,
        metadata: metadata(false, output_bytes),
    };
    Ok(result)
}

/// Redacts text of any length by scanning it in bounded, line-aligned chunks.
///
/// [`redact_text`] refuses input larger than [`MAX_INPUT_BYTES`] so a single
/// scan stays bounded. Tool results and command output regularly exceed that
/// bound, and refusing them turned an ordinary large file read into a terminal
/// task failure instead of redacted content. This helper keeps every individual
/// scan bounded while covering the whole text: chunks split on line boundaries,
/// so a detector match (email, token, bearer header, private key) is never cut
/// between two scans.
///
/// # Errors
///
/// Returns [`GuardrailError::Blocked`] when a blocking rule matches,
/// [`GuardrailError::InputTooLarge`] when one line alone exceeds the chunk
/// bound, and [`GuardrailError::InvalidPolicy`] for an invalid policy.
pub fn redact_text_chunked(
    snapshot: &PolicySnapshot,
    input: &str,
) -> Result<RedactionResult, GuardrailError> {
    if input.len() <= snapshot.policy.max_input_bytes {
        return redact_text(snapshot, input);
    }
    // Каждый чанк укладывается в тот же bound, что проверяет redact_text:
    // одна строка до MAX_INPUT_BYTES сканируется целиком, более длинная —
    // по-прежнему fail closed.
    let chunk_limit = snapshot.policy.max_input_bytes;
    let mut value = String::with_capacity(input.len());
    let mut action: Option<Action> = None;
    let mut rule_ids: Vec<String> = Vec::new();
    let mut match_count = 0usize;
    let mut chunk = String::new();
    for segment in input.split_inclusive('\n') {
        if segment.len() > chunk_limit {
            return Err(GuardrailError::InputTooLarge);
        }
        if !chunk.is_empty() && chunk.len() + segment.len() > chunk_limit {
            merge_redaction(
                &mut value,
                &mut action,
                &mut rule_ids,
                &mut match_count,
                redact_text(snapshot, &chunk)?,
            );
            chunk.clear();
        }
        chunk.push_str(segment);
    }
    if !chunk.is_empty() {
        merge_redaction(
            &mut value,
            &mut action,
            &mut rule_ids,
            &mut match_count,
            redact_text(snapshot, &chunk)?,
        );
    }
    let output_bytes = value.len();
    Ok(RedactionResult {
        value,
        metadata: RedactionMetadata {
            contract_version: CONTRACT_VERSION,
            policy_hash: snapshot.policy_hash.clone(),
            destination: snapshot.policy.destination.clone(),
            action,
            rule_ids,
            match_count,
            blocked: false,
            output_bytes,
        },
    })
}

/// Appends one chunk result and folds its metadata into the chunked total.
fn merge_redaction(
    value: &mut String,
    action: &mut Option<Action>,
    rule_ids: &mut Vec<String>,
    match_count: &mut usize,
    result: RedactionResult,
) {
    value.push_str(&result.value);
    *match_count = match_count.saturating_add(result.metadata.match_count);
    for rule_id in result.metadata.rule_ids {
        if !rule_ids.contains(&rule_id) {
            rule_ids.push(rule_id);
        }
    }
    let strongest = match (*action, result.metadata.action) {
        (Some(current), Some(next)) => Some(if next.rank() > current.rank() {
            next
        } else {
            current
        }),
        (None, next) => next,
        (current, None) => current,
    };
    *action = strongest;
}

/// Recursively redacts string values in a bounded JSON value.
pub fn redact_json(
    snapshot: &PolicySnapshot,
    value: &serde_json::Value,
) -> Result<(serde_json::Value, RedactionMetadata), GuardrailError> {
    fn visit(
        snapshot: &PolicySnapshot,
        value: &serde_json::Value,
        depth: usize,
        nodes: &mut usize,
        metadata: &mut Vec<RedactionMetadata>,
    ) -> Result<serde_json::Value, GuardrailError> {
        *nodes += 1;
        if *nodes > snapshot.policy.max_json_nodes || depth > snapshot.policy.max_json_depth {
            return Err(GuardrailError::StructuredPayloadTooLarge);
        }
        match value {
            serde_json::Value::String(text) => {
                let result = redact_text(snapshot, text)?;
                metadata.push(result.metadata);
                Ok(serde_json::Value::String(result.value))
            }
            serde_json::Value::Array(items) => Ok(serde_json::Value::Array(
                items
                    .iter()
                    .map(|item| visit(snapshot, item, depth + 1, nodes, metadata))
                    .collect::<Result<_, _>>()?,
            )),
            serde_json::Value::Object(items) => Ok(serde_json::Value::Object(
                items
                    .iter()
                    .map(|(key, item)| {
                        Ok((
                            key.clone(),
                            visit(snapshot, item, depth + 1, nodes, metadata)?,
                        ))
                    })
                    .collect::<Result<_, GuardrailError>>()?,
            )),
            other => Ok(other.clone()),
        }
    }
    let mut nodes = 0;
    let mut metadata = Vec::new();
    let value = visit(snapshot, value, 0, &mut nodes, &mut metadata)?;
    let mut rule_ids = metadata
        .iter()
        .flat_map(|item| item.rule_ids.clone())
        .collect::<Vec<_>>();
    rule_ids.sort();
    rule_ids.dedup();
    let match_count = metadata.iter().map(|item| item.match_count).sum();
    let output_bytes = serde_json::to_vec(&value)
        .map(|bytes| bytes.len())
        .unwrap_or(0);
    Ok((
        value,
        RedactionMetadata {
            contract_version: CONTRACT_VERSION,
            policy_hash: snapshot.policy_hash.clone(),
            destination: snapshot.policy.destination.clone(),
            action: metadata
                .iter()
                .filter_map(|item| item.action)
                .max_by_key(|action| action.rank()),
            rule_ids,
            match_count,
            blocked: false,
            output_bytes,
        },
    ))
}

/// Incremental text redactor that retains a bounded suffix between chunks.
pub struct StreamingRedactor {
    snapshot: PolicySnapshot,
    carry: String,
}

impl StreamingRedactor {
    /// Creates a streaming redactor for a validated policy snapshot.
    pub fn new(snapshot: PolicySnapshot) -> Self {
        Self {
            snapshot,
            carry: String::new(),
        }
    }

    /// Adds a chunk and returns only the prefix safe from cross-chunk matches.
    pub fn push_chunk(&mut self, chunk: &str) -> Result<RedactionResult, GuardrailError> {
        if self.carry.len().saturating_add(chunk.len()) > self.snapshot.policy.max_input_bytes {
            return Err(GuardrailError::InputTooLarge);
        }
        self.carry.push_str(chunk);
        let has_sensitive_prefix = ["@", "sk-", "ghp_", "xoxb-", "AKIA", "Bearer ", "-----BEGIN"]
            .iter()
            .any(|prefix| self.carry.contains(prefix));
        if !has_sensitive_prefix {
            let result = redact_text(&self.snapshot, &self.carry)?;
            self.carry.clear();
            return Ok(result);
        }
        if self.carry.ends_with(char::is_whitespace) {
            let result = redact_text(&self.snapshot, &self.carry)?;
            self.carry.clear();
            return Ok(result);
        }
        if self.carry.chars().count() <= STREAM_CARRY_CHARS {
            return Ok(empty_result(&self.snapshot));
        }
        let split = self
            .carry
            .char_indices()
            .nth(self.carry.chars().count() - STREAM_CARRY_CHARS)
            .map(|(index, _)| index)
            .unwrap_or(0);
        let stable = self.carry[..split].to_owned();
        self.carry = self.carry[split..].to_owned();
        redact_text(&self.snapshot, &stable)
    }

    /// Redacts and returns the remaining buffered suffix.
    pub fn finish(mut self) -> Result<RedactionResult, GuardrailError> {
        let result = redact_text(&self.snapshot, &self.carry)?;
        self.carry.clear();
        Ok(result)
    }
}

fn empty_result(snapshot: &PolicySnapshot) -> RedactionResult {
    RedactionResult {
        value: String::new(),
        metadata: RedactionMetadata {
            contract_version: CONTRACT_VERSION,
            policy_hash: snapshot.policy_hash.clone(),
            destination: snapshot.policy.destination.clone(),
            action: None,
            rule_ids: Vec::new(),
            match_count: 0,
            blocked: false,
            output_bytes: 0,
        },
    }
}
fn sha256(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}
fn mask(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= 4 {
        return "****".into();
    }
    format!(
        "{}{}{}",
        chars[..2].iter().collect::<String>(),
        "*".repeat(chars.len().saturating_sub(4)),
        chars[chars.len() - 2..].iter().collect::<String>()
    )
}

fn find_matches(input: &str, detector: Detector) -> Vec<(usize, usize)> {
    match detector {
        Detector::PrivateKey => input
            .find("-----BEGIN")
            .and_then(|start| {
                input[start..]
                    .find("PRIVATE KEY-----")
                    .map(|end| (start, start + end + "PRIVATE KEY-----".len()))
            })
            .into_iter()
            .collect(),
        Detector::BearerToken => input
            .match_indices("Bearer ")
            .map(|(start, _)| {
                let end = input[start..]
                    .find(char::is_whitespace)
                    .map(|offset| start + offset)
                    .unwrap_or(input.len());
                (start, end)
            })
            .filter(|(start, end)| end.saturating_sub(*start) > 7)
            .collect(),
        Detector::SecretToken => ["sk-", "ghp_", "xoxb-", "AKIA"]
            .iter()
            .flat_map(|prefix| {
                input.match_indices(prefix).map(move |(start, _)| {
                    let end = input[start..]
                        .find(char::is_whitespace)
                        .map(|offset| start + offset)
                        .unwrap_or(input.len());
                    (start, end)
                })
            })
            .collect(),
        Detector::Email => input
            .match_indices('@')
            .filter_map(|(at, _)| {
                let left = input[..at]
                    .rfind(char::is_whitespace)
                    .map(|index| index + 1)
                    .unwrap_or(0);
                let right = input[at..]
                    .find(char::is_whitespace)
                    .map(|index| at + index)
                    .unwrap_or(input.len());
                if at > left && right > at + 1 && input[at + 1..right].contains('.') {
                    Some((left, right))
                } else {
                    None
                }
            })
            .collect(),
        Detector::Phone => token_matches(input, |token| {
            let number = digits(token);
            number.len() >= 10
                && number.len() <= 15
                && (token.starts_with('+') || input_context_has_phone_label(input, token))
        }),
        Detector::PaymentCard => token_matches(input, |token| {
            (13..=19).contains(&token.len()) && luhn_valid(token)
        }),
        Detector::Iban => token_matches(input, |token| {
            (15..=34).contains(&token.len())
                && token.len() >= 4
                && token[..2]
                    .chars()
                    .all(|character| character.is_ascii_uppercase())
                && token[2..4]
                    .chars()
                    .all(|character| character.is_ascii_digit())
                && iban_mod97(token) == 1
        }),
        Detector::NationalTaxId => token_matches(input, |token| {
            (8..=14).contains(&token.len()) && input_context_has_tax_label(input, token)
        }),
    }
}

fn token_matches<F>(input: &str, predicate: F) -> Vec<(usize, usize)>
where
    F: Fn(&str) -> bool,
{
    let mut result = Vec::new();
    let mut start = None;
    for (index, character) in input
        .char_indices()
        .chain(std::iter::once((input.len(), ' ')))
    {
        let allowed = character.is_ascii_alphanumeric() || character == '+';
        if allowed && start.is_none() {
            start = Some(index);
        } else if !allowed {
            if let Some(start) = start.take() {
                if predicate(&input[start..index]) {
                    result.push((start, index));
                }
            }
        }
    }
    result
}

fn input_context_has_phone_label(input: &str, token: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    let _ = token;
    lower.contains("phone") || lower.contains("телефон") || lower.contains("mobile")
}

fn input_context_has_tax_label(input: &str, token: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    let _ = token;
    ["tax", "vat", "inn", "tin", "налог", "инн"]
        .iter()
        .any(|label| lower.contains(label))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_hash_is_deterministic() {
        assert_eq!(
            default_policy("provider").policy_hash,
            default_policy("provider").policy_hash
        );
    }
    #[test]
    fn all_actions_are_supported() {
        let base = default_policy("test");
        for action in [Action::Redact, Action::Mask, Action::Hash] {
            let mut policy = base.policy.clone();
            policy.rules[0].action = action;
            let result = redact_text(&snapshot(policy).unwrap(), "mail user@example.com").unwrap();
            assert_ne!(result.value, "mail user@example.com");
        }
    }
    #[test]
    fn private_key_is_blocked() {
        let error =
            redact_text(&default_policy("provider"), "-----BEGIN PRIVATE KEY-----").unwrap_err();
        assert!(matches!(error, GuardrailError::Blocked(_)));
    }
    #[test]
    fn structured_payload_is_recursive() {
        let (value, metadata) = redact_json(
            &default_policy("provider"),
            &serde_json::json!({"nested":["user@example.com"]}),
        )
        .unwrap();
        assert_eq!(metadata.match_count, 1);
        assert_ne!(value["nested"][0], "user@example.com");
    }
    #[test]
    fn stream_detects_match_across_chunks() {
        let mut stream = StreamingRedactor::new(default_policy("provider"));
        assert!(stream.push_chunk("user@").unwrap().value.is_empty());
        let result = stream
            .push_chunk(&format!("{} ", "example.com".repeat(10)))
            .unwrap();
        let tail = stream.finish().unwrap();
        assert!(
            !result.value.contains("user@example.com") && !tail.value.contains("user@example.com")
        );
    }
    #[test]
    fn oversized_multiline_text_is_redacted_in_bounded_chunks() {
        let mut input = String::new();
        for index in 0..3_000 {
            input.push_str(&format!("row {index} contact user{index}@example.com\n"));
        }
        assert!(input.len() > MAX_INPUT_BYTES);
        let result = redact_text_chunked(&default_policy("tool"), &input).unwrap();
        assert_eq!(result.metadata.match_count, 3_000);
        assert!(result.metadata.rule_ids.contains(&"email".to_string()));
        assert!(!result.value.contains("@example.com"));
        assert_eq!(result.metadata.output_bytes, result.value.len());
        assert!(!result.metadata.blocked);
    }
    #[test]
    fn a_blocking_rule_still_blocks_chunked_input() {
        let mut input = "safe line\n".repeat(10_000);
        input.push_str("-----BEGIN PRIVATE KEY-----");
        assert!(matches!(
            redact_text_chunked(&default_policy("tool"), &input),
            Err(GuardrailError::Blocked(_))
        ));
    }
    #[test]
    fn oversized_single_line_fails_closed_instead_of_being_skipped() {
        let input = "a".repeat(MAX_INPUT_BYTES + 10);
        assert!(matches!(
            redact_text_chunked(&default_policy("tool"), &input),
            Err(GuardrailError::InputTooLarge)
        ));
    }

    #[test]
    fn pii_detectors_require_structure_or_context() {
        let summary =
            classify_text("phone +1 (415) 555-2671 card 4111 1111 1111 1111 tax 123456789")
                .unwrap();
        assert!(summary
            .findings
            .iter()
            .any(|finding| finding.kind == SensitiveEntityKind::Phone));
        assert!(summary
            .findings
            .iter()
            .any(|finding| finding.kind == SensitiveEntityKind::PaymentCard));
        assert!(summary
            .findings
            .iter()
            .any(|finding| finding.kind == SensitiveEntityKind::NationalTaxId));
        assert_eq!(summary.required_privacy, "sensitive");
        assert!(!classify_text("uuid 123e4567-e89b-12d3-a456-426614174000")
            .unwrap()
            .findings
            .iter()
            .any(|finding| finding.kind == SensitiveEntityKind::NationalTaxId));
    }

    #[test]
    fn final_gate_binds_exact_payload_and_blocks_private_key() {
        let snapshot = default_policy("provider");
        let (decision, transformed) = evaluate_egress(
            &snapshot,
            DestinationClass::Provider,
            "provider:test",
            "contact user@example.com",
        )
        .unwrap();
        assert_eq!(decision.disposition, EgressDisposition::Transformed);
        assert!(transformed.is_some());
        assert_ne!(
            decision.payload_hash,
            decision.transformed_payload_hash.unwrap_or_default()
        );
        let (blocked, _) = evaluate_egress(
            &snapshot,
            DestinationClass::External,
            "remote:test",
            "-----BEGIN PRIVATE KEY-----",
        )
        .unwrap();
        assert_eq!(blocked.disposition, EgressDisposition::Blocked);
    }
    #[test]
    fn restart_is_ephemeral() {
        let first = StreamingRedactor::new(default_policy("one"));
        drop(first);
        let second = StreamingRedactor::new(default_policy("one"));
        assert!(second.carry.is_empty());
    }
}
