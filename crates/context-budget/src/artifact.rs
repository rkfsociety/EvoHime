//! Task artifact store: контракт, квоты, вытеснение и проверка при чтении
//! (этап 01.2).
//!
//! Store общий на уровне Core, но пространство имён — per-task: дедупликация по
//! `content_hash` может переиспользовать содержимое между задачами, а доступ по
//! locator ограничен задачей-владельцем и её детьми.

use serde::{Deserialize, Serialize};

use crate::item::Privacy;

/// Версия контракта store. 01.3 и 05.3 зависят от описанного здесь поведения,
/// а не от конкретной реализации хранилища.
pub const ARTIFACT_STORE_CONTRACT_VERSION: u32 = 1;

/// Состояние ссылки на артефакт.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactRefStatus {
    /// Содержимое доступно.
    Live,
    /// Содержимое вытеснено, но hash и размер сохранены.
    Expired,
    /// Проверка hash при чтении не сошлась: повреждение или подмена.
    Invalid,
}

impl ArtifactRefStatus {
    /// Returns the stable serialized value for this artifact state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Expired => "expired",
            Self::Invalid => "invalid",
        }
    }

    /// Parses a stored status; unknown values currently fall back to `Live`.
    pub fn parse(value: &str) -> Self {
        match value {
            "expired" => Self::Expired,
            "invalid" => Self::Invalid,
            _ => Self::Live,
        }
    }
}

/// Ссылка задачи на артефакт. Locator выдаётся владельцу и его детям.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Opaque locator used to retrieve this artifact.
    pub locator: String,
    /// Normalized content digest used for integrity and deduplication.
    pub content_hash: String,
    /// Task that created the artifact.
    pub task_id: String,
    /// Задача-владелец: доступ наследуется только вниз, к дочерним задачам.
    pub owner_task_id: String,
    /// Stored content size in bytes.
    pub bytes: u64,
    /// Privacy label controlling whether content may be offloaded.
    pub privacy: Privacy,
    /// Current storage lifecycle state.
    pub status: ArtifactRefStatus,
    /// Unix timestamp in seconds when the artifact was created.
    pub created_at: i64,
    /// Unix timestamp in seconds when content was last accessed.
    pub last_access_at: i64,
    /// Optional lifetime from creation, in milliseconds.
    pub ttl_ms: Option<i64>,
    /// Bounded summary, остающийся в контексте вместо содержимого.
    pub summary: String,
}

impl ArtifactRef {
    /// Returns whether the content remains available for retrieval.
    pub fn is_readable(&self) -> bool {
        self.status == ArtifactRefStatus::Live
    }

    /// Returns whether this reference's configured TTL has elapsed.
    pub fn ttl_expired(&self, now: i64) -> bool {
        self.ttl_ms
            .is_some_and(|ttl| now > self.created_at.saturating_add(ttl))
    }
}

/// Квоты store: на задачу и на диск целиком.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactQuota {
    /// Maximum bytes attributed to one task.
    pub per_task_bytes: u64,
    /// Maximum bytes held by the artifact store.
    pub total_bytes: u64,
    /// TTL по умолчанию для нового артефакта.
    pub default_ttl_ms: i64,
}

impl Default for ArtifactQuota {
    fn default() -> Self {
        Self {
            per_task_bytes: 256 * 1024 * 1024,
            total_bytes: 2 * 1024 * 1024 * 1024,
            default_ttl_ms: 7 * 24 * 60 * 60 * 1000,
        }
    }
}

/// Ошибка операции store.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArtifactError {
    /// The item's privacy classification forbids offloading its content.
    #[error("privacy label {0} forbids offload")]
    PrivacyForbidsOffload(&'static str),
    /// Caller task is outside the artifact owner's task subtree.
    #[error("artifact {locator} is not accessible from task {task_id}")]
    AccessDenied {
        /// Artifact locator requested by the caller.
        locator: String,
        /// Caller task identifier that failed access validation.
        task_id: String,
    },
    /// Artifact reference exists but is expired or otherwise unreadable.
    #[error("artifact {locator} is {status}")]
    NotReadable {
        /// Artifact locator whose content is unavailable.
        locator: String,
        /// Current status that prevents reading the artifact.
        status: String,
    },
    /// Retrieved bytes did not match the stored content digest.
    #[error("artifact {locator} failed the hash check: expected {expected}, got {actual}")]
    HashMismatch {
        /// Locator whose data failed verification.
        locator: String,
        /// Digest stored in the artifact reference.
        expected: String,
        /// Digest computed from the retrieved content.
        actual: String,
    },
    /// Storage request exceeds the per-task or whole-store quota.
    #[error("artifact quota exceeded: {scope} needs {needed} bytes, {available} available")]
    QuotaExceeded {
        /// Quota scope that was exceeded.
        scope: &'static str,
        /// Additional bytes required by the request.
        needed: u64,
        /// Bytes still available within the quota.
        available: u64,
    },
    /// Underlying storage backend failed.
    #[error("artifact store failed: {0}")]
    Backend(String),
}

/// Правило доступа: locator ограничен задачей-владельцем и её детьми.
pub fn access_allowed(reference: &ArtifactRef, task_id: &str, parent_chain: &[String]) -> bool {
    reference.owner_task_id == task_id
        || parent_chain
            .iter()
            .any(|ancestor| ancestor == &reference.owner_task_id)
}

/// Решение о вытеснении. Артефакт, на который ссылается живой ledger entry или
/// confirmed запись scratchpad, не удаляется молча: ссылка помечается `expired`
/// с сохранением hash и размера.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvictionPlan {
    /// Locator'ы, содержимое которых удаляется.
    pub evicted: Vec<String>,
    /// Locator'ы, ссылки на которые помечаются `expired`.
    pub marked_expired: Vec<String>,
    /// Number of content bytes selected for removal.
    pub freed_bytes: u64,
}

/// Кандидат вытеснения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvictionCandidate {
    /// Locator to consider for eviction.
    pub locator: String,
    /// Stored content size in bytes.
    pub bytes: u64,
    /// Last-access timestamp used to break eviction ties.
    pub last_access_at: i64,
    /// Whether the artifact TTL has elapsed.
    pub ttl_expired: bool,
    /// Ссылается ли на артефакт живой ledger entry или confirmed scratchpad.
    pub referenced: bool,
}

/// Планирование вытеснения: сначала истёкшие по TTL, затем по последнему
/// обращению. Возвращает план, освобождающий не меньше `needed_bytes`, либо всё,
/// что удалось освободить.
pub fn plan_eviction(candidates: &[EvictionCandidate], needed_bytes: u64) -> EvictionPlan {
    let mut ordered: Vec<&EvictionCandidate> = candidates.iter().collect();
    ordered.sort_by(|left, right| {
        right
            .ttl_expired
            .cmp(&left.ttl_expired)
            .then_with(|| left.last_access_at.cmp(&right.last_access_at))
            .then_with(|| left.locator.cmp(&right.locator))
    });
    let mut plan = EvictionPlan {
        evicted: Vec::new(),
        marked_expired: Vec::new(),
        freed_bytes: 0,
    };
    for candidate in ordered {
        if plan.freed_bytes >= needed_bytes {
            break;
        }
        plan.evicted.push(candidate.locator.clone());
        plan.freed_bytes = plan.freed_bytes.saturating_add(candidate.bytes);
        if candidate.referenced {
            plan.marked_expired.push(candidate.locator.clone());
        }
    }
    plan
}

/// Tombstone: после удаления содержимого hash сохраняется только для аудита и
/// не считается доступным dedup-hit для нового offload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactTombstone {
    /// Digest retained for audit after content removal.
    pub content_hash: String,
    /// Size of the removed content in bytes.
    pub bytes: u64,
    /// Unix timestamp in seconds when the content was removed.
    pub removed_at: i64,
    /// Stable reason for removing the content.
    pub reason: String,
}

/// Может ли существующая запись быть использована как dedup-hit.
pub fn dedup_hit_allowed(existing_status: ArtifactRefStatus, tombstoned: bool) -> bool {
    !tombstoned && existing_status == ArtifactRefStatus::Live
}

/// Bounded summary содержимого для контекста: первые строки без усечения
/// середины сообщения, с явной пометкой количества опущенных строк.
pub fn bounded_summary(content: &str, max_chars: usize, max_lines: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut summary = String::new();
    let mut used_lines = 0_usize;
    for line in lines.iter().take(max_lines) {
        if summary.chars().count() + line.chars().count() + 1 > max_chars {
            break;
        }
        if !summary.is_empty() {
            summary.push('\n');
        }
        summary.push_str(line);
        used_lines += 1;
    }
    let omitted = lines.len().saturating_sub(used_lines);
    if omitted > 0 {
        summary.push_str(&format!("\n… ещё {omitted} строк в артефакте"));
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(locator: &str, owner: &str) -> ArtifactRef {
        ArtifactRef {
            locator: locator.to_string(),
            content_hash: "hash".to_string(),
            task_id: owner.to_string(),
            owner_task_id: owner.to_string(),
            bytes: 100,
            privacy: Privacy::Workspace,
            status: ArtifactRefStatus::Live,
            created_at: 0,
            last_access_at: 0,
            ttl_ms: Some(1_000),
            summary: String::new(),
        }
    }

    #[test]
    fn locator_access_is_limited_to_the_owner_and_its_children() {
        let artifact = reference("artifact://a", "parent");
        assert!(access_allowed(&artifact, "parent", &[]));
        assert!(access_allowed(&artifact, "child", &["parent".to_string()]));
        assert!(!access_allowed(&artifact, "stranger", &[]));
        assert!(!access_allowed(
            &artifact,
            "stranger",
            &["other".to_string()]
        ));
    }

    #[test]
    fn eviction_prefers_ttl_expired_then_least_recently_used() {
        let candidates = vec![
            EvictionCandidate {
                locator: "fresh".to_string(),
                bytes: 100,
                last_access_at: 500,
                ttl_expired: false,
                referenced: false,
            },
            EvictionCandidate {
                locator: "old".to_string(),
                bytes: 100,
                last_access_at: 10,
                ttl_expired: false,
                referenced: false,
            },
            EvictionCandidate {
                locator: "expired".to_string(),
                bytes: 100,
                last_access_at: 900,
                ttl_expired: true,
                referenced: false,
            },
        ];
        let plan = plan_eviction(&candidates, 200);
        assert_eq!(plan.evicted, vec!["expired".to_string(), "old".to_string()]);
        assert_eq!(plan.freed_bytes, 200);
    }

    #[test]
    fn referenced_artifacts_are_marked_expired_instead_of_silently_lost() {
        let candidates = vec![EvictionCandidate {
            locator: "referenced".to_string(),
            bytes: 100,
            last_access_at: 0,
            ttl_expired: true,
            referenced: true,
        }];
        let plan = plan_eviction(&candidates, 50);
        assert_eq!(plan.marked_expired, vec!["referenced".to_string()]);
    }

    #[test]
    fn tombstoned_content_is_not_a_dedup_hit() {
        assert!(dedup_hit_allowed(ArtifactRefStatus::Live, false));
        assert!(!dedup_hit_allowed(ArtifactRefStatus::Live, true));
        assert!(!dedup_hit_allowed(ArtifactRefStatus::Expired, false));
        assert!(!dedup_hit_allowed(ArtifactRefStatus::Invalid, false));
    }

    #[test]
    fn expired_and_invalid_references_are_not_readable() {
        let mut artifact = reference("artifact://a", "task");
        assert!(artifact.is_readable());
        artifact.status = ArtifactRefStatus::Expired;
        assert!(!artifact.is_readable());
        artifact.status = ArtifactRefStatus::Invalid;
        assert!(!artifact.is_readable());
    }

    #[test]
    fn bounded_summary_never_cuts_the_middle_of_a_line() {
        let content = (1..=20)
            .map(|index| format!("строка {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let summary = bounded_summary(&content, 200, 3);
        assert!(summary.starts_with("строка 1\nстрока 2\nстрока 3"));
        assert!(summary.contains("ещё 17 строк"));
    }

    #[test]
    fn default_quota_covers_both_task_and_disk_scopes() {
        let quota = ArtifactQuota::default();
        assert!(quota.per_task_bytes < quota.total_bytes);
        assert!(quota.default_ttl_ms > 0);
    }
}
