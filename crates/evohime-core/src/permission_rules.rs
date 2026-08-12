use evohime_permissions::{PermissionEngine, PolicyRule, PolicyRuleSet};
use std::path::Path;

/// Загружает rules из permissions.json файла в data_dir.
///
/// Логика:
/// 1. Если файл не существует → вернуть Ok(PolicyRuleSet::defaults())
/// 2. Если файл пуст → вернуть Ok(PolicyRuleSet::defaults())
/// 3. Если файл содержит `[]` (пустой JSON массив) → вернуть Ok(PolicyRuleSet::new(vec![]))
/// 4. Если файл битый JSON → залогировать и вернуть Ok(PolicyRuleSet::defaults())
/// 5. Если JSON валиден → десериализовать в Vec<PolicyRule>, вернуть Ok(PolicyRuleSet::new(...))
pub fn load_rules_from(path: &Path) -> Result<PolicyRuleSet, String> {
    let rules_path = path.join("permissions.json");

    // Если файл не существует → дефолты
    if !rules_path.exists() {
        return Ok(PolicyRuleSet::defaults());
    }

    // Прочитаем файл
    let content = match std::fs::read_to_string(&rules_path) {
        Ok(content) => content,
        Err(error) => {
            return Err(format!("failed to read permissions.json: {error}"));
        }
    };

    // Если файл пуст → дефолты
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(PolicyRuleSet::defaults());
    }

    // Парсим JSON
    let rules: Vec<PolicyRule> = match serde_json::from_str(trimmed) {
        Ok(rules) => rules,
        Err(error) => {
            return Err(format!("invalid JSON in permissions.json: {error}"));
        }
    };

    // Если пустой массив [] → осознанное отключение дефолтов
    if rules.is_empty() && trimmed == "[]" {
        return Ok(PolicyRuleSet::new(vec![]));
    }

    // Валидный набор правил
    Ok(PolicyRuleSet::new(rules))
}

/// Применяет загруженные rules к PermissionEngine при старте.
/// При ошибке логирует в core.jsonl и использует дефолты.
pub async fn apply_rules(permissions: &PermissionEngine, data_dir: &Path) {
    let path = data_dir.join("permissions.json");

    // Пытаемся загрузить правила
    let result = load_rules_from(data_dir);

    let (rules, error) = match result {
        Ok(rules) => (rules, None),
        Err(error) => (PolicyRuleSet::defaults(), Some(error)),
    };

    permissions.set_policy_rules(rules).await;

    // Логируем ошибку, если она была
    if let Some(error) = error {
        if let Ok(logger) = StructuredLogger::open(data_dir.join("logs/core.jsonl")) {
            let _ = logger.write(
                "error",
                "permissions.load_failed",
                serde_json::json!({"path": path, "error": error, "fallback": "defaults"}),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn create_temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "evohime-test-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = fs::create_dir_all(&path);
        path
    }

    fn cleanup_temp_dir(path: &PathBuf) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn load_nonexistent_file_returns_defaults() {
        let temp_dir = create_temp_dir();
        let result = load_rules_from(&temp_dir).unwrap();
        let defaults = PolicyRuleSet::defaults();
        assert_eq!(result.rules().len(), defaults.rules().len());
        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn load_empty_file_returns_defaults() {
        let temp_dir = create_temp_dir();
        fs::write(temp_dir.join("permissions.json"), "").unwrap();
        let result = load_rules_from(&temp_dir).unwrap();
        let defaults = PolicyRuleSet::defaults();
        assert_eq!(result.rules().len(), defaults.rules().len());
        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn load_empty_array_returns_empty_set() {
        let temp_dir = create_temp_dir();
        fs::write(temp_dir.join("permissions.json"), "[]").unwrap();
        let result = load_rules_from(&temp_dir).unwrap();
        assert!(result.is_empty());
        assert_eq!(result.rules().len(), 0);
        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn load_invalid_json_returns_error() {
        let temp_dir = create_temp_dir();
        fs::write(temp_dir.join("permissions.json"), "{invalid json").unwrap();
        let result = load_rules_from(&temp_dir);
        assert!(result.is_err());
        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn load_valid_rules() {
        let temp_dir = create_temp_dir();
        let rules_json = r#"[
            {
                "permission": "shell_execute",
                "pattern": "cargo *",
                "mode": "allow"
            },
            {
                "permission": "shell_execute",
                "pattern": "rm *",
                "mode": "deny"
            }
        ]"#;
        fs::write(temp_dir.join("permissions.json"), rules_json).unwrap();
        let result = load_rules_from(&temp_dir).unwrap();
        assert_eq!(result.rules().len(), 2);
        assert_eq!(result.rules()[0].pattern, "cargo *");
        assert_eq!(result.rules()[1].pattern, "rm *");
        cleanup_temp_dir(&temp_dir);
    }

    #[tokio::test]
    async fn apply_rules_sets_engine_rules() {
        let temp_dir = create_temp_dir();
        let rules_json = r#"[
            {
                "permission": "filesystem_read",
                "pattern": "*.secret",
                "mode": "deny"
            }
        ]"#;
        fs::write(temp_dir.join("permissions.json"), rules_json).unwrap();

        let engine = PermissionEngine::new();
        apply_rules(&engine, &temp_dir).await;

        let loaded_rules = engine.policy_rules().await;
        assert_eq!(loaded_rules.rules().len(), 1);
        assert_eq!(loaded_rules.rules()[0].pattern, "*.secret");
        cleanup_temp_dir(&temp_dir);
    }
}

pub use crate::StructuredLogger;
