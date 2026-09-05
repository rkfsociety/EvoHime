/// Bounded лимит чтения: базовое значение 01.1 — не более 100 элементов.
pub(crate) fn bounded_limit(limit: u32) -> usize {
    let limit = if limit == 0 { 20 } else { limit as usize };
    limit.min(100)
}

/// Maps an IPC-layer scope kind + project/secondary id pair into the
/// `memory_domain::MemoryScope` used for validation and redaction.
pub(crate) fn memory_domain_scope(
    kind: &str,
    project_id: &str,
    secondary_id: &str,
) -> Result<crate::memory_domain::MemoryScope, String> {
    match kind {
        "project" => crate::memory_domain::MemoryScope::project(project_id)
            .map_err(|error| error.to_string()),
        "task" => crate::memory_domain::MemoryScope::task(project_id, secondary_id)
            .map_err(|error| error.to_string()),
        "workspace" => crate::memory_domain::MemoryScope::workspace(project_id, secondary_id)
            .map_err(|error| error.to_string()),
        other => Err(format!("unsupported memory scope kind: {other}")),
    }
}

/// Maps an IPC-layer scope kind into the `memory_store::MemoryScope` used by
/// the real `memory_entries` table.
pub(crate) fn memory_store_scope(
    kind: &str,
) -> Result<evohime_local_storage::memory_store::MemoryScope, String> {
    match kind {
        "project" => Ok(evohime_local_storage::memory_store::MemoryScope::Project),
        "task" => Ok(evohime_local_storage::memory_store::MemoryScope::Task),
        "workspace" => Ok(evohime_local_storage::memory_store::MemoryScope::Workspace),
        // Session-scoped memory exists only as a `memory_session_notes` row
        // with automatic expiry; it is addressable here so pending/conflict
        // listings can report it, but it never enters long-term retrieval.
        "session" => Ok(evohime_local_storage::memory_store::MemoryScope::Session),
        other => Err(format!("unsupported memory scope kind: {other}")),
    }
}

pub(crate) fn parse_memory_privacy(value: &str) -> Result<crate::memory_domain::PrivacyLabel, String> {
    match value {
        "public" => Ok(crate::memory_domain::PrivacyLabel::Public),
        "internal" | "" => Ok(crate::memory_domain::PrivacyLabel::Internal),
        "private" => Ok(crate::memory_domain::PrivacyLabel::Private),
        other => Err(format!(
            "unsupported memory privacy label: {other} (secret is not supported by persistent storage)"
        )),
    }
}

/// The persistent `memory_entries` table has no `secret` privacy label; the
/// domain-level `PrivacyLabel::Secret` is rejected before it ever reaches
/// storage (callers must not be able to persist a value they cannot express).
pub(crate) fn memory_store_privacy(
    label: crate::memory_domain::PrivacyLabel,
) -> Result<evohime_local_storage::memory_store::MemoryPrivacy, String> {
    match label {
        crate::memory_domain::PrivacyLabel::Public => {
            Ok(evohime_local_storage::memory_store::MemoryPrivacy::Public)
        }
        crate::memory_domain::PrivacyLabel::Internal => {
            Ok(evohime_local_storage::memory_store::MemoryPrivacy::Internal)
        }
        crate::memory_domain::PrivacyLabel::Private => {
            Ok(evohime_local_storage::memory_store::MemoryPrivacy::Private)
        }
        crate::memory_domain::PrivacyLabel::Secret => {
            Err("secret privacy is not supported by persistent memory storage".to_string())
        }
    }
}

/// Encodes a project/secondary id pair into the single `scope_id` column the
/// `memory_entries` table stores. Project scope uses the project id alone;
/// task/workspace scope appends the secondary id after a `:` separator so
/// list/search can still target one exact scope.
/// System prompt of the bounded extractor. It describes the structured
/// contract only: the model proposes candidates, it never decides whether
/// something becomes memory — that is `memory_extraction::evaluate`'s job.
/// System prompt for the bounded, policy-controlled memory extractor.
pub(crate) const MEMORY_EXTRACTION_PROMPT: &str = "\
Ты — извлекатель кандидатов в память. Ты НЕ решаешь, что запомнить: решение \
принимает policy на стороне Core. Верни ТОЛЬКО JSON вида \
{\"candidates\":[...]} без markdown и пояснений. Каждый кандидат: \
{\"kind\":\"preference|constraint|decision|entity|lesson|session_summary\", \
\"statement\":\"...\",\"scope\":\"task|project|workspace|session\", \
\"canonical_subject\":\"...\",\"model_confidence\":0.0..1.0, \
\"verification_confidence\":0.0,\"reason\":\"...\", \
\"evidence_locator\":{\"message_id\":\"...\",\"task_id\":\"...\", \
\"tool_call_id\":\"...\",\"file_path\":\"...\",\"content_hash\":\"...\", \
\"line_start\":0,\"line_end\":0},\"privacy\":\"normal|sensitive\", \
\"source_trust\":\"user|tool_output|document|model_inference\", \
\"suggested_ttl_ms\":0}. Не более 5 кандидатов. Никогда не включай пароли, \
токены, ключи и другие секреты. Неизвестные поля запрещены. Если запоминать \
нечего — верни {\"candidates\":[]}.";

/// System prompt of the ambient extractor (04.6). It differs from the dialog
/// one in what it may propose at all: `constraint` and `decision` are refused
/// outright, and the evidence locator carries the episode instead of a
/// message. `source_trust` is not negotiable either — Core overwrites it with
/// `ambient` regardless of what the model claims.
/// System prompt for extracting non-authoritative ambient-memory candidates.
pub(crate) const AMBIENT_MEMORY_EXTRACTION_PROMPT: &str = "\
Ты — извлекатель кандидатов в память из расшифровки услышанной речи. \
Говорящий НЕ подтверждён: это может быть не пользователь. Ты НЕ решаешь, \
что запомнить: решение принимает policy на стороне Core. Верни ТОЛЬКО \
JSON вида {\"candidates\":[...]} без markdown и пояснений. Каждый \
кандидат: {\"kind\":\"preference|entity|lesson\",\"statement\":\"...\", \
\"scope\":\"workspace\",\"canonical_subject\":\"...\", \
\"model_confidence\":0.0..1.0,\"verification_confidence\":0.0, \
\"reason\":\"...\",\"evidence_locator\":{\"episode_id\":\"<эпизод>\"}, \
\"privacy\":\"normal|sensitive\",\"source_trust\":\"ambient\", \
\"suggested_ttl_ms\":0}. Не предлагай ограничений и решений: такие kind \
запрещены. Не более 5 кандидатов. Никогда не включай пароли, токены, ключи \
и другие секреты. Неизвестные поля запрещены. Если запоминать нечего — \
верни {\"candidates\":[]}.";

/// Scope id under which ambient candidates live.
///
/// Речь у стола не принадлежит ни одному репозиторию, поэтому привязывать её
/// к рабочему каталогу было бы выдумкой. Собственный scope делает связь
/// честной, а очередь подтверждения дополняется ambient-кандидатами явно, а
/// не тем, что они притворились записями текущего воркспейса.
/// Стабильный scope ID для извлечённых из ambient-событий записей памяти.
pub const AMBIENT_MEMORY_SCOPE_ID: &str = "ambient";

/// Какие услышанные утверждения становятся ограниченным предложением (04.7).
///
/// Ровно те два вида, которые 04.6 отказывается делать памятью, потому что
/// они влияют на действия: решение («сделаю X») предлагается задачей,
/// ограничение («не забыть про X») — неисполняемым напоминанием. Всё
/// остальное остаётся кандидатом в память и предложением не становится:
/// предпочтение или факт не требуют действия.
pub fn ambient_proposal_kind(
    kind: crate::memory_extraction::MemoryKind,
) -> Option<evohime_listener_contract::ProposalKind> {
    match kind {
        crate::memory_extraction::MemoryKind::Decision => {
            Some(evohime_listener_contract::ProposalKind::Suggestion)
        }
        crate::memory_extraction::MemoryKind::Constraint => {
            Some(evohime_listener_contract::ProposalKind::Reminder)
        }
        _ => None,
    }
}

/// Ambient extraction mode for this process. Отсутствие переменной — это
/// `pending`; мусор в ней — `off`, а не молчаливое включение.
pub(crate) fn ambient_memory_mode() -> crate::memory_extraction::AmbientMemoryMode {
    crate::memory_extraction::AmbientMemoryMode::parse(
        std::env::var("EVOHIME_AMBIENT_MEMORY").ok().as_deref(),
    )
}

/// Extraction mode for this process. The user can switch automatic
/// extraction off entirely; explicit "запомни" triggers keep working because
/// `check_can_extract` allows a manual trigger even when disabled.
pub(crate) fn memory_extraction_mode() -> crate::memory_extraction::ExtractionMode {
    std::env::var("EVOHIME_MEMORY_EXTRACTION")
        .ok()
        .and_then(|value| {
            crate::memory_extraction::ExtractionMode::parse(value.trim().to_lowercase().as_str())
        })
        .unwrap_or(crate::memory_extraction::ExtractionMode::Strict)
}

pub(crate) fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }
    value.chars().take(max_chars).collect()
}

pub(crate) fn context_token_estimate(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .map(|message| message.content.chars().count().div_ceil(4))
        .sum()
}

/// The durable half of the evidence locator, indexed so provenance can be
/// traced back without storing any body.
pub(crate) fn memory_provenance_source_id(
    evidence: &crate::memory_extraction::RawEvidenceLocator,
) -> Option<String> {
    // Эпизод проверяется первым: связь «кандидат ↔ эпизод» существует ради
    // удаления, и именно по этому значению `ambient_store` находит своих
    // кандидатов, чтобы отклонить их причиной `source_deleted`.
    for value in [
        &evidence.episode_id,
        &evidence.message_id,
        &evidence.tool_call_id,
        &evidence.task_id,
        &evidence.file_path,
    ] {
        if !value.trim().is_empty() {
            return Some(value.trim().to_owned());
        }
    }
    None
}

/// Projects a stored record into the comparison shape used by
/// `memory_extraction::detect_conflict`. Records whose enums no longer parse
/// are skipped rather than silently treated as a different kind.
pub(crate) fn memory_active_summary(
    record: &evohime_local_storage::memory_store::MemoryRecord,
) -> Option<crate::memory_extraction::ActiveMemorySummary> {
    Some(crate::memory_extraction::ActiveMemorySummary {
        id: record.id.clone(),
        kind: crate::memory_extraction::MemoryKind::parse(&record.extraction.kind)?,
        canonical_subject: memory_conflict_subject(record),
        scope: crate::memory_extraction::MemoryScopeLevel::parse(record.scope.as_str())?,
        statement: record.content.clone(),
        state: crate::memory_extraction::ConfirmationState::parse(
            &record.extraction.confirmation_state,
        )?,
    })
}

/// Bounded batch size for `ConfirmMemory`/`RejectMemory`, so one IPC call
/// cannot walk the whole pending queue in a single transaction.
/// Maximum number of memory candidates accepted in one batch.
pub(crate) const MAX_MEMORY_BATCH: usize = 64;
/// Maximum idempotency-key length for memory commands.
pub(crate) const MAX_MEMORY_IDEMPOTENCY_KEY_CHARS: usize = 128;

pub(crate) fn memory_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// The idempotency key is caller-supplied proof that a repeat is a repeat.
/// It is bounded and audited; the actual replay safety comes from the
/// storage-level state transition, which never applies a second time.
pub(crate) fn validate_memory_idempotency_key(key: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("idempotency_key is required".to_string());
    }
    if key.chars().count() > MAX_MEMORY_IDEMPOTENCY_KEY_CHARS {
        return Err(format!(
            "idempotency_key exceeds {MAX_MEMORY_IDEMPOTENCY_KEY_CHARS} characters"
        ));
    }
    Ok(())
}

/// Canonical subject of a stored record. Legacy rows have none, so the title
/// stands in and gets normalized by the same versioned normalizer.
pub(crate) fn memory_conflict_subject(record: &evohime_local_storage::memory_store::MemoryRecord) -> String {
    match crate::memory_extraction::normalize_subject(record.subject_for_conflict()) {
        Ok(subject) => subject,
        Err(error) => {
            tracing::debug!(%error, "memory subject normalization failed; preserving original subject");
            record.subject_for_conflict().to_owned()
        }
    }
}

/// Finds the active record a pending candidate conflicts with:
/// same `kind + canonical_subject + scope`, incompatible statements.
/// Equivalent statements are duplicates, not conflicts.
pub(crate) fn memory_conflicting_record<'a>(
    candidate: &evohime_local_storage::memory_store::MemoryRecord,
    active: &'a [evohime_local_storage::memory_store::MemoryRecord],
) -> Option<&'a evohime_local_storage::memory_store::MemoryRecord> {
    let subject = memory_conflict_subject(candidate);
    let statement = crate::memory_extraction::normalize_subject(&candidate.content).ok();
    active.iter().find(|existing| {
        existing.id != candidate.id
            && existing.extraction.kind == candidate.extraction.kind
            && existing.scope == candidate.scope
            && memory_conflict_subject(existing) == subject
            && crate::memory_extraction::normalize_subject(&existing.content).ok() != statement
    })
}

/// Scope id for memory reads. A workspace path takes precedence because
/// memory extraction stores records under `task_memory::workspace_scope_id`,
/// which the shell cannot reproduce on its own.
pub(crate) fn memory_scope_id(workspace_path: &str, project_id: &str, secondary_id: &str) -> String {
    if workspace_path.trim().is_empty() {
        encode_memory_scope_id(project_id, secondary_id)
    } else {
        task_memory::workspace_scope_id(std::path::Path::new(workspace_path))
    }
}

pub(crate) fn encode_memory_scope_id(project_id: &str, secondary_id: &str) -> String {
    if secondary_id.trim().is_empty() {
        project_id.to_string()
    } else {
        format!("{project_id}:{secondary_id}")
    }
}

pub(crate) fn decode_memory_scope_id(scope_id: &str) -> (String, String) {
    match scope_id.split_once(':') {
        Some((project_id, secondary_id)) => (project_id.to_string(), secondary_id.to_string()),
        None => (scope_id.to_string(), String::new()),
    }
}

/// Renders a stored `memory_store::MemoryRecord` back into the JSON shape
/// returned over IPC, decoding the scope id and parsing the provenance JSON
/// that was serialized at create time.
pub(crate) fn memory_record_to_json(
    record: &evohime_local_storage::memory_store::MemoryRecord,
) -> Result<serde_json::Value, String> {
    let (project_id, secondary_id) = decode_memory_scope_id(&record.scope_id);
    let provenance: serde_json::Value = if record.provenance.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&record.provenance).unwrap_or(serde_json::Value::Null)
    };
    let scope_kind = record.scope.as_str();
    let privacy = match record.privacy {
        evohime_local_storage::memory_store::MemoryPrivacy::Public => "public",
        evohime_local_storage::memory_store::MemoryPrivacy::Internal => "internal",
        evohime_local_storage::memory_store::MemoryPrivacy::Private => "private",
    };
    // Metadata-only projection. `ListMemory`/`SearchMemory` never carry the
    // statement or the provenance body: those are reachable only through an
    // explicit `GetMemory`, and even there `sensitive` records are redacted.
    let extraction = &record.extraction;
    Ok(serde_json::json!({
        "id": record.id,
        "scope_kind": scope_kind,
        "project_id": project_id,
        "secondary_id": secondary_id,
        "title": record.title,
        "privacy": privacy,
        "created_at_ms": record.created_at,
        "expires_at_ms": record.expires_at,
        "archived": record.archived,
        "forgotten": record.forgotten,
        "kind": extraction.kind,
        "canonical_subject": extraction.canonical_subject,
        "confirmation_state": extraction.confirmation_state,
        "model_confidence": extraction.model_confidence,
        "verification_confidence": extraction.verification_confidence,
        "privacy_class": extraction.privacy_class,
        "source_trust": extraction.source_trust,
        "supersedes": extraction.supersedes,
        "superseded_by": extraction.superseded_by,
        "supersession_reason": extraction.supersession_reason,
        "extractor_version": extraction.extractor_version,
        "policy_version": extraction.policy_version,
        "validation_status": extraction.validation_status,
        "validated_at": extraction.validated_at,
         "provenance_source_id": extraction.provenance_source_id,
         "authority": extraction.authority,
         "durability": extraction.durability,
         "confidence": extraction.confidence,
         "statement_chars": record.content.chars().count(),
        "has_provenance": !provenance.is_null(),
    }))
}

/// Full projection including the statement and provenance body, used only by
/// the explicit `GetMemory` path. `sensitive` and forgotten records never
/// return their body: the metadata still explains what exists and why.
pub(crate) fn memory_record_body_json(
    record: &evohime_local_storage::memory_store::MemoryRecord,
) -> Result<serde_json::Value, String> {
    let mut value = memory_record_to_json(record)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "memory metadata must be an object".to_string())?;
    let redacted = record.extraction.privacy_class != "normal"
        || record.forgotten
        || record.content.is_empty();
    if redacted {
        object.insert("body_redacted".to_owned(), serde_json::Value::Bool(true));
        return Ok(value);
    }
    let provenance: serde_json::Value = if record.provenance.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&record.provenance).unwrap_or(serde_json::Value::Null)
    };
    object.insert("body_redacted".to_owned(), serde_json::Value::Bool(false));
    object.insert(
        "statement".to_owned(),
        serde_json::Value::String(record.content.clone()),
    );
    object.insert("provenance".to_owned(), provenance);
    Ok(value)
}

/// Cheap listing classification derived from which of a manifest's
/// `roles`/`skills` lists are non-empty; see
/// `capability_store::ManifestKind` for why this is store-layer only.
pub(crate) fn capability_manifest_kind(
    manifest: &crate::capability_registry::CapabilityManifest,
) -> evohime_local_storage::capability_store::ManifestKind {
    match (!manifest.roles.is_empty(), !manifest.skills.is_empty()) {
        (true, false) => evohime_local_storage::capability_store::ManifestKind::Role,
        (false, true) => evohime_local_storage::capability_store::ManifestKind::Skill,
        _ => evohime_local_storage::capability_store::ManifestKind::Mixed,
    }
}

/// Maximum archive size accepted by capability import.
pub(crate) const MAX_CAPABILITY_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
/// Maximum duration of one capability archive operation.
pub(crate) const CAPABILITY_ARCHIVE_TIMEOUT_MS: u64 = 30_000;

/// Downloads one capability archive into bounded memory solely for integrity
/// verification. The archive is deliberately not persisted by this command;
/// the catalog write below records only the already-validated manifest.
pub(crate) async fn verify_https_capability_archive(
    source_url: &str,
    expected_content_hash: &str,
) -> Result<(), String> {
    if expected_content_hash.len() != 64
        || !expected_content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("expected_content_hash must be a 64-character SHA-256 digest".to_string());
    }
    let url = reqwest::Url::parse(source_url).map_err(|error| error.to_string())?;
    if url.scheme() != "https" {
        return Err("https_archive source_path must use HTTPS".to_string());
    }
    evohime_tool_runtime::assert_safe_http_url(&url)
        .map_err(|message| format!("ssrf blocked capability archive: {message}"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(
            CAPABILITY_ARCHIVE_TIMEOUT_MS,
        ))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.url().scheme() == "https"
                && evohime_tool_runtime::assert_safe_http_url(attempt.url()).is_ok()
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .user_agent("EvoHime/0.1 capability-installer")
        .build()
        .map_err(|error| format!("capability archive client setup failed: {error}"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("capability archive download failed: {error}"))?;
    if response.url().scheme() != "https" {
        return Err("capability archive redirect left HTTPS".to_string());
    }
    evohime_tool_runtime::assert_safe_http_url(response.url())
        .map_err(|message| format!("ssrf blocked capability archive redirect: {message}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "capability archive endpoint returned {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_CAPABILITY_ARCHIVE_BYTES)
    {
        return Err(format!(
            "capability archive exceeds {MAX_CAPABILITY_ARCHIVE_BYTES} byte limit"
        ));
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("failed to read capability archive: {error}"))?;
        body.extend_from_slice(&chunk);
        if body.len() as u64 > MAX_CAPABILITY_ARCHIVE_BYTES {
            return Err(format!(
                "capability archive exceeds {MAX_CAPABILITY_ARCHIVE_BYTES} byte limit"
            ));
        }
    }
    verify_capability_archive_hash(&body, expected_content_hash)
}

pub(crate) fn verify_capability_archive_hash(bytes: &[u8], expected_content_hash: &str) -> Result<(), String> {
    let observed = crate::research::sha256_hex(bytes);
    if !observed.eq_ignore_ascii_case(expected_content_hash) {
        return Err(format!(
            "capability archive SHA-256 mismatch: expected {expected_content_hash}, observed {observed}"
        ));
    }
    Ok(())
}

pub(crate) fn capability_risk_class_str(risk: crate::capability_registry::RiskClass) -> &'static str {
    match risk {
        crate::capability_registry::RiskClass::Low => "low",
        crate::capability_registry::RiskClass::Medium => "medium",
        crate::capability_registry::RiskClass::High => "high",
    }
}

pub(crate) fn capability_selection_origin_to_store(
    origin: crate::capability_selection::SelectionOrigin,
) -> evohime_local_storage::capability_selection_store::SelectionOrigin {
    match origin {
        crate::capability_selection::SelectionOrigin::Auto => {
            evohime_local_storage::capability_selection_store::SelectionOrigin::Auto
        }
        crate::capability_selection::SelectionOrigin::Pinned => {
            evohime_local_storage::capability_selection_store::SelectionOrigin::Pinned
        }
        crate::capability_selection::SelectionOrigin::Replaced => {
            evohime_local_storage::capability_selection_store::SelectionOrigin::Replaced
        }
    }
}

pub(crate) fn parse_capability_risk_class(
    value: &str,
) -> Result<crate::capability_registry::RiskClass, String> {
    match value {
        "low" => Ok(crate::capability_registry::RiskClass::Low),
        "medium" | "" => Ok(crate::capability_registry::RiskClass::Medium),
        "high" => Ok(crate::capability_registry::RiskClass::High),
        other => Err(format!("unsupported requested_risk: {other}")),
    }
}

pub(crate) fn handoff_kind_from_str(value: &str) -> Result<crate::child_roles::HandoffKind, String> {
    match value {
        "delegate" => Ok(crate::child_roles::HandoffKind::Delegate),
        "return_result" => Ok(crate::child_roles::HandoffKind::ReturnResult),
        "request_review" => Ok(crate::child_roles::HandoffKind::RequestReview),
        "request_retry" => Ok(crate::child_roles::HandoffKind::RequestRetry),
        other => Err(format!("unsupported handoff kind: {other}")),
    }
}

pub(crate) fn handoff_kind_str(kind: crate::child_roles::HandoffKind) -> &'static str {
    match kind {
        crate::child_roles::HandoffKind::Delegate => "delegate",
        crate::child_roles::HandoffKind::ReturnResult => "return_result",
        crate::child_roles::HandoffKind::RequestReview => "request_review",
        crate::child_roles::HandoffKind::RequestRetry => "request_retry",
    }
}

pub(crate) fn handoff_status_str(status: crate::child_roles::HandoffStatus) -> &'static str {
    match status {
        crate::child_roles::HandoffStatus::Pending => "pending",
        crate::child_roles::HandoffStatus::Accepted => "accepted",
        crate::child_roles::HandoffStatus::Rejected => "rejected",
        crate::child_roles::HandoffStatus::Completed => "completed",
    }
}

pub(crate) fn child_role_from_str(value: &str) -> Result<crate::child_roles::ChildRole, String> {
    match value {
        "coordinator" => Ok(crate::child_roles::ChildRole::Coordinator),
        "researcher" => Ok(crate::child_roles::ChildRole::Researcher),
        "planner" => Ok(crate::child_roles::ChildRole::Planner),
        "implementer" => Ok(crate::child_roles::ChildRole::Implementer),
        "reviewer" => Ok(crate::child_roles::ChildRole::Reviewer),
        "tester" => Ok(crate::child_roles::ChildRole::Tester),
        "custom" => Ok(crate::child_roles::ChildRole::Custom),
        other => Err(format!("unsupported child role: {other}")),
    }
}

/// Builds a `RoleIdentity` from the wire's separate role/name fields. A
/// "custom" role requires a bounded, validated name; a built-in role
/// carries no name.
pub(crate) fn role_identity_from_parts(
    role: &str,
    name: &str,
) -> Result<crate::child_roles::RoleIdentity, String> {
    let parsed_role = child_role_from_str(role)?;
    if parsed_role == crate::child_roles::ChildRole::Custom {
        crate::child_roles::RoleIdentity::custom(name).map_err(|error| error.to_string())
    } else {
        Ok(crate::child_roles::RoleIdentity::builtin(parsed_role))
    }
}

/// Cheap display form of a `RoleIdentity` for the store's denormalized
/// listing columns only; the full identity survives in the envelope JSON.
pub(crate) fn role_identity_display(identity: &crate::child_roles::RoleIdentity) -> String {
    match &identity.name {
        Some(name) => format!("custom:{name}"),
        None => format!("{:?}", identity.role).to_ascii_lowercase(),
    }
}

pub(crate) fn child_task_kind_from_str(value: &str) -> Result<crate::child_runtime::ChildTaskKind, String> {
    match value {
        "code_search" => Ok(crate::child_runtime::ChildTaskKind::CodeSearch),
        "threat_model_review" => Ok(crate::child_runtime::ChildTaskKind::ThreatModelReview),
        "test_plan_review" => Ok(crate::child_runtime::ChildTaskKind::TestPlanReview),
        "documentation" => Ok(crate::child_runtime::ChildTaskKind::Documentation),
        "onboarding" => Ok(crate::child_runtime::ChildTaskKind::Onboarding),
        other => Err(format!("unsupported child task kind: {other}")),
    }
}

pub(crate) fn child_task_kind_str(kind: crate::child_runtime::ChildTaskKind) -> &'static str {
    match kind {
        crate::child_runtime::ChildTaskKind::CodeSearch => "code_search",
        crate::child_runtime::ChildTaskKind::ThreatModelReview => "threat_model_review",
        crate::child_runtime::ChildTaskKind::TestPlanReview => "test_plan_review",
        crate::child_runtime::ChildTaskKind::Documentation => "documentation",
        crate::child_runtime::ChildTaskKind::Onboarding => "onboarding",
    }
}

pub(crate) fn child_report_status_from_str(
    value: &str,
) -> Result<crate::child_runtime::ChildReportStatus, String> {
    match value {
        "complete" => Ok(crate::child_runtime::ChildReportStatus::Complete),
        "partial" => Ok(crate::child_runtime::ChildReportStatus::Partial),
        "rejected" => Ok(crate::child_runtime::ChildReportStatus::Rejected),
        other => Err(format!("unsupported child report status: {other}")),
    }
}

pub(crate) fn child_report_status_str(status: crate::child_runtime::ChildReportStatus) -> &'static str {
    match status {
        crate::child_runtime::ChildReportStatus::Complete => "complete",
        crate::child_runtime::ChildReportStatus::Partial => "partial",
        crate::child_runtime::ChildReportStatus::Rejected => "rejected",
    }
}

/// Fail-closed permissions probe used when the doctor cannot ground its
/// permissions check in a real, resolved workspace (no project supplied or
/// the project was not found). This intentionally does not claim health.
pub(crate) fn unresolved_permissions_probe(approval_required: bool) -> crate::doctor::PermissionsProbe {
    crate::doctor::PermissionsProbe {
        workspace_readable: false,
        workspace_writable: false,
        protected_paths_intact: false,
        approval_required,
    }
}
