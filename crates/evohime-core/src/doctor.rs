// Bounded Core Doctor contract.
//
// The contract deliberately does not perform I/O. Core adapters can collect
// safe probe facts and turn them into a bounded, actionable report without
// exposing provider credentials, tokens, or raw error payloads.

use serde::{Deserialize, Serialize};

pub const MAX_CHECKS: usize = 16;
pub const MAX_TEXT_CHARS: usize = 256;
pub const MAX_DETAILS_CHARS: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub id: String,
    pub status: CheckStatus,
    pub summary: String,
    pub action: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    pub contract_version: u16,
    pub bounded: bool,
    pub checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorError {
    TooManyChecks,
    TextTooLong(&'static str),
    InvalidProbe(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorSnapshot {
    pub storage: StorageProbe,
    pub pipe: PipeProbe,
    pub provider: ProviderProbe,
    pub recovery: RecoveryProbe,
    pub permissions: PermissionsProbe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageProbe {
    pub path_label: String,
    pub exists: bool,
    pub writable: bool,
    pub schema_version: Option<u32>,
    pub expected_schema_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipeProbe {
    pub pipe_label: String,
    pub reachable: bool,
    pub protocol_major: Option<u32>,
    pub expected_protocol_major: u32,
}

/// Provider facts are intentionally metadata-only. A key value or token is
/// not accepted by this contract at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderProbe {
    pub provider_id: String,
    pub model_id: String,
    pub configured: bool,
    pub key_present: bool,
    pub metadata_valid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryProbe {
    pub state: String,
    pub unknown_effects: u32,
    pub lease_expired: bool,
    pub resumable_runs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionsProbe {
    pub workspace_readable: bool,
    pub workspace_writable: bool,
    pub protected_paths_intact: bool,
    pub approval_required: bool,
}

impl DoctorReport {
    pub fn from_snapshot(snapshot: &DoctorSnapshot) -> Result<Self, DoctorError> {
        validate_snapshot(snapshot)?;
        let checks = vec![
            storage_check(&snapshot.storage),
            pipe_check(&snapshot.pipe),
            provider_check(&snapshot.provider),
            recovery_check(&snapshot.recovery),
            permissions_check(&snapshot.permissions),
        ];
        Self::new(checks)
    }

    pub fn new(checks: Vec<DoctorCheck>) -> Result<Self, DoctorError> {
        if checks.len() > MAX_CHECKS {
            return Err(DoctorError::TooManyChecks);
        }
        for check in &checks {
            validate_text("check.id", &check.id, MAX_TEXT_CHARS)?;
            validate_text("check.summary", &check.summary, MAX_TEXT_CHARS)?;
            validate_text("check.action", &check.action, MAX_TEXT_CHARS)?;
            if let Some(details) = &check.details {
                validate_text("check.details", details, MAX_DETAILS_CHARS)?;
            }
        }
        Ok(Self {
            contract_version: 1,
            bounded: true,
            checks,
        })
    }

    pub fn is_actionable(&self) -> bool {
        self.checks
            .iter()
            .any(|check| !matches!(check.status, CheckStatus::Ok))
    }

    pub fn to_bounded_json(&self) -> String {
        serde_json::to_string(self).expect("DoctorReport is serializable")
    }
}

fn storage_check(probe: &StorageProbe) -> DoctorCheck {
    if !probe.exists {
        return check(
            "storage",
            CheckStatus::Fail,
            "Хранилище не найдено",
            "Проверь путь данных и запусти Core повторно",
            Some(&probe.path_label),
        );
    }
    if !probe.writable {
        return check(
            "storage",
            CheckStatus::Blocked,
            "Хранилище доступно только для чтения",
            "Проверь права каталога данных",
            Some(&probe.path_label),
        );
    }
    if probe.schema_version != Some(probe.expected_schema_version) {
        return check(
            "storage",
            CheckStatus::Fail,
            "Версия схемы SQLite не совпадает",
            "Выполни штатную миграцию Core и повтори проверку",
            Some(&format!(
                "schema={:?}, expected={}",
                probe.schema_version, probe.expected_schema_version
            )),
        );
    }
    check(
        "storage",
        CheckStatus::Ok,
        "Хранилище и схема доступны",
        "Действий не требуется",
        None,
    )
}

fn pipe_check(probe: &PipeProbe) -> DoctorCheck {
    if !probe.reachable {
        return check(
            "pipe",
            CheckStatus::Fail,
            "Core pipe недоступен",
            "Перезапусти supervisor и проверь журнал Core",
            Some(&probe.pipe_label),
        );
    }
    if probe.protocol_major != Some(probe.expected_protocol_major) {
        return check(
            "pipe",
            CheckStatus::Blocked,
            "Версия IPC несовместима",
            "Обнови UI и Core до одной major-версии",
            Some(&format!(
                "protocol={:?}, expected={}",
                probe.protocol_major, probe.expected_protocol_major
            )),
        );
    }
    check(
        "pipe",
        CheckStatus::Ok,
        "Core pipe доступен",
        "Действий не требуется",
        None,
    )
}

fn provider_check(probe: &ProviderProbe) -> DoctorCheck {
    if !probe.configured || !probe.key_present {
        return check(
            "provider",
            CheckStatus::Blocked,
            "Провайдер не настроен",
            "Сохрани ключ провайдера в штатных настройках",
            None,
        );
    }
    if !probe.metadata_valid {
        return check(
            "provider",
            CheckStatus::Warn,
            "Метаданные модели неполные",
            "Обнови каталог моделей и повтори проверку",
            None,
        );
    }
    check(
        "provider",
        CheckStatus::Ok,
        "Метаданные провайдера доступны",
        "Действий не требуется",
        None,
    )
}

fn recovery_check(probe: &RecoveryProbe) -> DoctorCheck {
    if probe.unknown_effects > 0 || probe.lease_expired {
        return check(
            "recovery",
            CheckStatus::Blocked,
            "Восстановление требует решения",
            "Открой историю запуска и подтверди resume или rollback",
            Some(&format!(
                "state={}, unknown_effects={}, lease_expired={}",
                probe.state, probe.unknown_effects, probe.lease_expired
            )),
        );
    }
    if probe.resumable_runs > 0 {
        return check(
            "recovery",
            CheckStatus::Warn,
            "Есть возобновляемые запуски",
            "Проверь историю и продолжи нужный запуск",
            Some(&format!(
                "state={}, resumable={}",
                probe.state, probe.resumable_runs
            )),
        );
    }
    check(
        "recovery",
        CheckStatus::Ok,
        "Состояние восстановления чистое",
        "Действий не требуется",
        Some(&format!("state={}", probe.state)),
    )
}

fn permissions_check(probe: &PermissionsProbe) -> DoctorCheck {
    if !probe.workspace_readable || !probe.workspace_writable {
        return check(
            "permissions",
            CheckStatus::Fail,
            "Workspace недоступен для записи",
            "Проверь права текущего пользователя и путь workspace",
            None,
        );
    }
    if !probe.protected_paths_intact {
        return check(
            "permissions",
            CheckStatus::Blocked,
            "Защищённые пути требуют проверки",
            "Останови Build и восстанови защищённые файлы из snapshot",
            None,
        );
    }
    if probe.approval_required {
        return check(
            "permissions",
            CheckStatus::Warn,
            "Для опасных операций требуется approval",
            "Подтверждай mutation-операции только после просмотра diff",
            None,
        );
    }
    check(
        "permissions",
        CheckStatus::Ok,
        "Права workspace соответствуют политике",
        "Действий не требуется",
        None,
    )
}

fn check(
    id: &str,
    status: CheckStatus,
    summary: &str,
    action: &str,
    details: Option<&str>,
) -> DoctorCheck {
    DoctorCheck {
        id: id.to_owned(),
        status,
        summary: bounded_text(summary, MAX_TEXT_CHARS),
        action: bounded_text(action, MAX_TEXT_CHARS),
        details: details.map(|value| bounded_text(value, MAX_DETAILS_CHARS)),
    }
}

fn validate_snapshot(snapshot: &DoctorSnapshot) -> Result<(), DoctorError> {
    validate_text(
        "storage.path_label",
        &snapshot.storage.path_label,
        MAX_TEXT_CHARS,
    )?;
    validate_text("pipe.pipe_label", &snapshot.pipe.pipe_label, MAX_TEXT_CHARS)?;
    validate_text(
        "provider.provider_id",
        &snapshot.provider.provider_id,
        MAX_TEXT_CHARS,
    )?;
    validate_text(
        "provider.model_id",
        &snapshot.provider.model_id,
        MAX_TEXT_CHARS,
    )?;
    validate_text("recovery.state", &snapshot.recovery.state, MAX_TEXT_CHARS)?;
    if snapshot.storage.expected_schema_version == 0 {
        return Err(DoctorError::InvalidProbe("expected schema version"));
    }
    if snapshot.pipe.expected_protocol_major == 0 {
        return Err(DoctorError::InvalidProbe("expected protocol major"));
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), DoctorError> {
    if value.chars().count() > max {
        Err(DoctorError::TextTooLong(field))
    } else {
        Ok(())
    }
}

fn bounded_text(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> DoctorSnapshot {
        DoctorSnapshot {
            storage: StorageProbe {
                path_label: "data-dir".into(),
                exists: true,
                writable: true,
                schema_version: Some(5),
                expected_schema_version: 5,
            },
            pipe: PipeProbe {
                pipe_label: "core-v1".into(),
                reachable: true,
                protocol_major: Some(1),
                expected_protocol_major: 1,
            },
            provider: ProviderProbe {
                provider_id: "local-provider".into(),
                model_id: "model-a".into(),
                configured: true,
                key_present: true,
                metadata_valid: true,
            },
            recovery: RecoveryProbe {
                state: "CLEAN".into(),
                unknown_effects: 0,
                lease_expired: false,
                resumable_runs: 0,
            },
            permissions: PermissionsProbe {
                workspace_readable: true,
                workspace_writable: true,
                protected_paths_intact: true,
                approval_required: false,
            },
        }
    }

    #[test]
    fn healthy_snapshot_is_serializable_and_bounded() {
        let report = DoctorReport::from_snapshot(&snapshot()).unwrap();
        assert_eq!(report.checks.len(), 5);
        assert!(!report.is_actionable());
        let json = report.to_bounded_json();
        assert!(json.contains("\"bounded\":true"));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn failures_are_actionable_without_secret_values() {
        let mut value = snapshot();
        value.provider.configured = false;
        value.provider.provider_id = "provider-with-secret-looking-id".into();
        value.recovery.unknown_effects = 1;
        let report = DoctorReport::from_snapshot(&value).unwrap();
        assert!(report.is_actionable());
        assert_eq!(report.checks[2].status, CheckStatus::Blocked);
        assert_eq!(report.checks[3].status, CheckStatus::Blocked);
        assert!(!report
            .to_bounded_json()
            .contains("provider-with-secret-looking-id"));
        assert!(report.checks.iter().all(|check| {
            check.details.as_deref().unwrap_or_default().chars().count() <= MAX_DETAILS_CHARS
        }));
    }

    #[test]
    fn report_rejects_unbounded_input_and_check_count() {
        let mut value = snapshot();
        value.storage.path_label = "x".repeat(MAX_TEXT_CHARS + 1);
        assert_eq!(
            DoctorReport::from_snapshot(&value),
            Err(DoctorError::TextTooLong("storage.path_label"))
        );
        let checks = (0..=MAX_CHECKS)
            .map(|_| DoctorCheck {
                id: "id".into(),
                status: CheckStatus::Ok,
                summary: "summary".into(),
                action: "action".into(),
                details: None,
            })
            .collect();
        assert_eq!(DoctorReport::new(checks), Err(DoctorError::TooManyChecks));
    }

    #[test]
    fn every_probe_has_a_specific_actionable_status() {
        let mut value = snapshot();
        value.storage.writable = false;
        value.pipe.reachable = false;
        value.permissions.workspace_writable = false;
        let report = DoctorReport::from_snapshot(&value).unwrap();
        for check in report.checks {
            assert!(!check.id.is_empty());
            assert!(!check.summary.is_empty());
            assert!(!check.action.is_empty());
        }
    }
}
