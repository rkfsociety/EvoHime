use crate::research::sha256_hex;
use evohime_local_storage::memory_store::{MemoryPrivacy, MemoryRecord, MemoryScope};
use evohime_local_storage::ToolMetricRecord;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const LESSON_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1000;

pub fn workspace_scope_id(workspace_root: &Path) -> String {
    let normalized = workspace_root
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase();
    format!("workspace-{}", sha256_hex(normalized.as_bytes()))
}

/// Derive a deterministic scope ID from workspace root path string.
/// Used to isolate memory entries between different projects.
/// Handles case-insensitivity and path separator normalization.
///
/// Returns a hex string of 32 characters (SHA-256 of first 16 bytes).
pub fn project_scope_id(workspace_root: &str) -> String {
    // 1. Нормализировать путь: привести к lowercase, заменить \\ на /
    let normalized = workspace_root.to_lowercase().replace('\\', "/");

    // 2. Вычислить SHA-256 от нормализованного пути
    sha256_hex(normalized.as_bytes())
}

pub fn build_lesson(
    task_id: &str,
    workspace_root: &Path,
    metrics: &[ToolMetricRecord],
) -> Option<MemoryRecord> {
    let failures = metrics
        .iter()
        .filter_map(|metric| metric.failure_kind.as_deref())
        .collect::<Vec<_>>();
    if failures.is_empty() {
        return None;
    }
    let mut failure_names = failures.to_vec();
    failure_names.sort_unstable();
    failure_names.dedup();
    let mut tools = metrics
        .iter()
        .map(|metric| metric.tool_name.as_str())
        .collect::<Vec<_>>();
    tools.sort_unstable();
    tools.dedup();
    let lesson_key =
        sha256_hex(format!("{}|{}", tools.join(","), failure_names.join(",")).as_bytes());
    let now = now_millis();
    let content = format!(
        "Инструменты: {}. Классы провалов: {}. Повторяй только после проверки аргументов и фактического состояния workspace.",
        tools.join(", "),
        failure_names.join(", "),
    );
    let mut record = MemoryRecord::new(
        format!("lesson-{task_id}-{lesson_key}"),
        MemoryScope::Project,
        workspace_scope_id(workspace_root),
        "Подтверждённый урок восстановления инструмента",
        content,
        format!("task:{task_id}"),
        MemoryPrivacy::Private,
        now.to_string(),
        Some((now + LESSON_TTL_MS).to_string()),
    )
    .ok()?;
    record.lesson_key = Some(lesson_key);
    Some(record)
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_is_stable_for_windows_path_spelling() {
        assert_eq!(
            workspace_scope_id(Path::new("C:\\github\\EvoHime")),
            workspace_scope_id(Path::new("c:/github/evohime/"))
        );
    }

    #[test]
    fn test_project_scope_id_identical_paths_same_case_and_separator() {
        let path = "C:\\Users\\roman\\repo";
        let scope1 = project_scope_id(path);
        let scope2 = project_scope_id(path);

        assert_eq!(scope1, scope2);
        assert_eq!(scope1.len(), 64); // SHA-256 = 32 bytes * 2 hex chars
        assert!(scope1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_project_scope_id_different_case_same_scope() {
        let path_lower = "c:\\users\\roman\\repo";
        let path_upper = "C:\\USERS\\ROMAN\\REPO";
        let path_mixed = "C:\\Users\\Roman\\Repo";

        let scope_lower = project_scope_id(path_lower);
        let scope_upper = project_scope_id(path_upper);
        let scope_mixed = project_scope_id(path_mixed);

        assert_eq!(scope_lower, scope_upper);
        assert_eq!(scope_lower, scope_mixed);
    }

    #[test]
    fn test_project_scope_id_different_separators_same_scope() {
        let path_backslash = "C:\\Users\\roman\\repo";
        let path_forward = "C:/Users/roman/repo";
        let path_mixed = "C:\\Users/roman\\repo";

        let scope_backslash = project_scope_id(path_backslash);
        let scope_forward = project_scope_id(path_forward);
        let scope_mixed = project_scope_id(path_mixed);

        assert_eq!(scope_backslash, scope_forward);
        assert_eq!(scope_backslash, scope_mixed);
    }

    #[test]
    fn test_project_scope_id_different_paths_different_scope() {
        let path1 = "C:\\Users\\roman\\repo1";
        let path2 = "C:\\Users\\roman\\repo2";
        let path3 = "/home/alice/project";

        let scope1 = project_scope_id(path1);
        let scope2 = project_scope_id(path2);
        let scope3 = project_scope_id(path3);

        assert_ne!(scope1, scope2);
        assert_ne!(scope1, scope3);
        assert_ne!(scope2, scope3);
    }

    #[test]
    fn test_project_scope_id_deterministic_hash() {
        let path = "G:\\github\\EvoHime";

        let mut scopes = Vec::new();
        for _ in 0..10 {
            scopes.push(project_scope_id(path));
        }

        // Все должны быть одинаковыми
        for scope in &scopes[1..] {
            assert_eq!(scope, &scopes[0]);
        }
    }

    #[test]
    fn test_project_scope_id_format() {
        let path = "/var/workspace";
        let scope = project_scope_id(path);

        // Должен быть hex-строка длины 64 (32 байта SHA-256)
        assert_eq!(scope.len(), 64);
        assert!(scope.chars().all(|c| c.is_ascii_hexdigit()));
        // Проверим, что это lowercase
        assert_eq!(scope, scope.to_lowercase());
    }

    #[test]
    fn test_project_scope_id_empty_string_produces_scope() {
        let scope = project_scope_id("");
        assert_eq!(scope.len(), 64);
        assert!(scope.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_project_scope_id_unicode_paths() {
        let path_cyrillic = "C:\\Users\\пользователь\\проект";
        let path_cyrillic_upper = "C:\\USERS\\ПОЛЬЗОВАТЕЛЬ\\ПРОЕКТ";

        let scope_lower = project_scope_id(path_cyrillic);
        let scope_upper = project_scope_id(path_cyrillic_upper);

        assert_eq!(scope_lower, scope_upper);
        assert_eq!(scope_lower.len(), 64);
    }
}
