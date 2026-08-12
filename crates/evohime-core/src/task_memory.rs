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
    let lesson_key = sha256_hex(
        format!("{}|{}", tools.join(","), failure_names.join(",")).as_bytes(),
    );
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
}
