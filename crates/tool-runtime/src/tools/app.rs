//! Открытие приложений рабочего стола.
//!
//! Инструмент не принимает путь: на вход идёт название, а путь берётся из
//! каталога ([`crate::app_catalog`]). Это не удобство, а граница — иначе через
//! `app.open` можно было бы запустить любой файл на диске, обойдя и allow-list,
//! и разбор команды.

use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

pub const OPEN_NAME: &str = "app.open";
pub const OPEN_DESCRIPTION: &str =
    "Open a desktop application from the local application catalog by name";
pub const OPEN_PERMISSIONS: &[Permission] = &[Permission::ShellExecute];
pub const OPEN_TIMEOUT: Duration = Duration::from_secs(15);

pub const LIST_NAME: &str = "app.list";
pub const LIST_DESCRIPTION: &str = "List the applications that app.open can open";
pub const LIST_PERMISSIONS: &[Permission] = &[Permission::ShellExecute];
pub const LIST_TIMEOUT: Duration = Duration::from_secs(10);

/// Сколько записей возвращает `app.list` без явного запроса: каталог машины
/// может быть в сотни строк, а контекст модели — нет.
const DEFAULT_LIST_LIMIT: usize = 60;

#[derive(Debug, Deserialize)]
struct OpenInput {
    app: String,
}

#[derive(Debug, Default, Deserialize)]
struct ListInput {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

pub async fn open(_ctx: &ToolContext, value: serde_json::Value) -> Result<ToolResult, ToolError> {
    let input: OpenInput =
        serde_json::from_value(value).map_err(|error| ToolError::InvalidInput {
            tool: OPEN_NAME.to_string(),
            message: error.to_string(),
        })?;
    let catalog = crate::app_catalog::default_catalog();
    let entry = match catalog.resolve(&input.app) {
        crate::app_catalog::Resolution::Found(entry) => entry,
        crate::app_catalog::Resolution::Ambiguous(titles) => {
            return Err(ToolError::InvalidInput {
                tool: OPEN_NAME.to_string(),
                message: format!(
                    "название подходит нескольким приложениям: {}",
                    titles.join(", ")
                ),
            })
        }
        crate::app_catalog::Resolution::NotFound => {
            return Err(ToolError::InvalidInput {
                tool: OPEN_NAME.to_string(),
                message: format!(
                    "приложение «{}» не найдено в каталоге; доступные: {}",
                    input.app,
                    sample_titles(&catalog)
                ),
            })
        }
    };
    let launched = tokio::task::spawn_blocking({
        let entry = entry.clone();
        move || crate::app_catalog::launch(&entry)
    })
    .await
    .map_err(|error| ToolError::Execution(format!("launch task failed: {error}")))?;
    let pid = launched.map_err(|error| ToolError::Execution(format!("launch failed: {error}")))?;
    Ok(ToolResult {
        output: format!("Открыто: {}", entry.title),
        structured: json!({
            "app_id": entry.id,
            "title": entry.title,
            "path": entry.exec.display().to_string(),
            "pid": pid,
        }),
    })
}

pub async fn list(_ctx: &ToolContext, value: serde_json::Value) -> Result<ToolResult, ToolError> {
    let input: ListInput = if value.is_null() {
        ListInput::default()
    } else {
        serde_json::from_value(value).map_err(|error| ToolError::InvalidInput {
            tool: LIST_NAME.to_string(),
            message: error.to_string(),
        })?
    };
    let catalog = crate::app_catalog::default_catalog();
    let needle = input
        .query
        .as_deref()
        .map(crate::app_catalog::normalize)
        .filter(|value| !value.is_empty());
    let limit = input.limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, 200);
    let matched: Vec<_> = catalog
        .entries()
        .iter()
        .filter(|entry| match needle.as_deref() {
            None => true,
            Some(needle) => {
                crate::app_catalog::normalize(&entry.title).contains(needle)
                    || entry.id.contains(needle)
                    || entry
                        .aliases
                        .iter()
                        .any(|alias| crate::app_catalog::normalize(alias).contains(needle))
            }
        })
        .take(limit)
        .map(|entry| {
            json!({
                "app_id": entry.id,
                "title": entry.title,
                "aliases": entry.aliases,
            })
        })
        .collect();
    let output = if matched.is_empty() {
        "Каталог приложений пуст".to_string()
    } else {
        matched
            .iter()
            .filter_map(|entry| entry.get("title").and_then(serde_json::Value::as_str))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Ok(ToolResult {
        output,
        structured: json!({
            "apps": matched,
            "total": catalog.entries().len(),
        }),
    })
}

fn sample_titles(catalog: &crate::app_catalog::AppCatalog) -> String {
    catalog
        .entries()
        .iter()
        .take(12)
        .map(|entry| entry.title.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn context() -> ToolContext {
        ToolContext {
            workspace_root: std::env::temp_dir(),
            task_id: Uuid::new_v4(),
            session_id: None,
            progress_tx: None,
        }
    }

    #[tokio::test]
    async fn open_refuses_an_unknown_application_instead_of_guessing() {
        let error = open(&context(), json!({ "app": "холодильник-3000" }))
            .await
            .unwrap_err();
        match error {
            ToolError::InvalidInput { message, .. } => {
                assert!(message.contains("не найдено"), "{message}");
            }
            other => panic!("ожидался InvalidInput, получено {other:?}"),
        }
    }

    #[tokio::test]
    async fn open_refuses_a_raw_path() {
        // Путь — это не название: каталог его не знает, и запуск не состоится.
        let error = open(
            &context(),
            json!({ "app": "C:\\Windows\\System32\\cmd.exe" }),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ToolError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn list_returns_bounded_catalog() {
        let result = list(&context(), json!({ "limit": 3 })).await.unwrap();
        let apps = result.structured["apps"].as_array().unwrap().len();
        assert!(apps <= 3, "список обязан уважать limit, получено {apps}");
    }
}
