use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde_json::{json, Value};
use std::time::Duration;

/// Registry identifier for structured memory search.
pub const NAME: &str = "memory.search";
/// Input and output summary exposed in tool catalogs.
pub const DESCRIPTION: &str =
    "Search structured agent memory (facts, constraints, experience, playbooks) by query. Input: { query, limit? }.";
/// Permission required before memory search may be requested.
pub const PERMISSIONS: &[Permission] = &[Permission::MemorySearch];
/// Maximum duration advertised to the tool registry.
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// Registry stub — real search runs in agent-runtime with DB access.
/// Registry placeholder; actual search is performed by the agent memory backend.
///
/// This function intentionally returns an execution error because the tool
/// runtime does not own the memory database.
pub async fn execute(_ctx: &ToolContext, _input: Value) -> Result<ToolResult, ToolError> {
    Err(ToolError::Execution(
        "memory.search is executed by the agent memory backend".into(),
    ))
}

/// Validates memory-search input and returns its normalized query and limit.
///
/// The query is trimmed and must be nonempty. Limits default to 10 and are
/// clamped to the inclusive range `1..=50`.
///
/// # Errors
///
/// Returns [`ToolError::InvalidInput`] when the query is absent or blank.
pub fn parse_input(input: &Value) -> Result<(String, usize), ToolError> {
    let query = input
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ToolError::InvalidInput {
            tool: NAME.to_string(),
            message: "query is required".into(),
        })?;
    let limit = input
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value.clamp(1, 50) as usize)
        .unwrap_or(10);
    Ok((query.to_string(), limit))
}

/// Formats memory matches as readable text and structured JSON.
///
/// Each entry contains its scope, kind, content, and relevance score; an empty
/// slice produces an explicit no-matches result.
pub fn format_results(query: &str, entries: &[(String, String, String, f64)]) -> ToolResult {
    if entries.is_empty() {
        return ToolResult {
            output: format!("memory.search: no matches for {query:?}"),
            structured: json!({
                "query": query,
                "count": 0,
                "matches": []
            }),
        };
    }
    let mut lines = vec![format!(
        "memory.search ({query}): {} match(es)",
        entries.len()
    )];
    let matches: Vec<Value> = entries
        .iter()
        .enumerate()
        .map(|(index, (scope, kind, content, score))| {
            lines.push(format!(
                "{}. [{scope}/{kind}] score={score:.2} {content}",
                index + 1
            ));
            json!({
                "scope": scope,
                "kind": kind,
                "content": content,
                "score": score,
            })
        })
        .collect();
    ToolResult {
        output: lines.join("\n"),
        structured: json!({
            "query": query,
            "count": entries.len(),
            "matches": matches,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_input_requires_query() {
        assert!(parse_input(&json!({})).is_err());
        let (query, limit) = parse_input(&json!({"query":" worktrees ", "limit": 3})).unwrap();
        assert_eq!(query, "worktrees");
        assert_eq!(limit, 3);
    }
}
