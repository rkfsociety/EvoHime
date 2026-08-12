use evohime_permissions::{PermissionEngine, PolicyRuleSet};
use std::path::Path;

pub fn load_rules_from(path: &Path) -> Result<PolicyRuleSet, String> {
    let content = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    if content.trim().is_empty() {
        return Ok(PolicyRuleSet::defaults());
    }
    serde_json::from_str(&content).map_err(|error| error.to_string())
}

pub async fn apply_rules(permissions: &PermissionEngine, data_dir: &Path) {
    let path = data_dir.join("permissions.json");
    let result = if path.exists() {
        load_rules_from(&path)
    } else {
        Ok(PolicyRuleSet::defaults())
    };
    let (rules, error) = match result {
        Ok(rules) => (rules, None),
        Err(error) => (PolicyRuleSet::defaults(), Some(error)),
    };
    permissions.set_policy_rules(rules).await;
    if let Some(error) = error {
        if let Ok(logger) = crate::StructuredLogger::open(data_dir.join("logs/core.jsonl")) {
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
    use evohime_permissions::{Permission, PermissionMode};
    use std::fs;

    #[test]
    fn reads_rules_and_keeps_explicit_empty_array_empty() {
        let path =
            std::env::temp_dir().join(format!("evohime-permissions-{}.json", std::process::id()));
        fs::write(
            &path,
            r#"[{"permission":"git_write","pattern":"git push*","mode":"deny"}]"#,
        )
        .unwrap();
        assert_eq!(
            load_rules_from(&path)
                .unwrap()
                .resolve(Permission::GitWrite, "git push"),
            Some(PermissionMode::Deny)
        );
        fs::write(&path, "[]").unwrap();
        assert!(load_rules_from(&path).unwrap().is_empty());
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn missing_file_applies_defaults() {
        let dir =
            std::env::temp_dir().join(format!("evohime-permissions-dir-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let permissions = PermissionEngine::new();
        apply_rules(&permissions, &dir).await;
        assert_eq!(permissions.policy_rules().await.rules().len(), 2);
        let _ = fs::remove_dir_all(dir);
    }
}
