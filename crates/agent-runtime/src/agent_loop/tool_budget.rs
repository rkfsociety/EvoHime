//! Tool result size budget for prompts / replan observe (Stage 7.33).

use std::env;

/// Caps how much tool output is fed back into planning / final model context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolResultBudget {
    /// Max characters kept per individual tool result.
    pub per_result_chars: usize,
    /// Max characters for the combined tool-results block.
    pub total_chars: usize,
}

impl ToolResultBudget {
    pub fn from_env() -> Self {
        Self {
            per_result_chars: env_usize("EVOHIME_TOOL_RESULT_MAX_CHARS", 6_000).clamp(512, 200_000),
            total_chars: env_usize("EVOHIME_TOOL_RESULT_TOTAL_CHARS", 24_000).clamp(1_024, 500_000),
        }
    }
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// Keep head + tail when truncating so errors at the end of shell output survive.
pub fn truncate_tool_result(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if max_chars == 0 {
        return String::new();
    }
    if chars.len() <= max_chars {
        return text.to_string();
    }

    let omitted = chars.len().saturating_sub(max_chars);
    let marker = format!("\n…[truncated {omitted} chars]…\n");
    let marker_len = marker.chars().count();
    if marker_len >= max_chars {
        return marker.chars().take(max_chars).collect();
    }

    let keep = max_chars - marker_len;
    let head = keep * 70 / 100;
    let tail = keep - head;
    let mut out = String::with_capacity(max_chars);
    out.extend(chars.iter().take(head));
    out.push_str(&marker);
    if tail > 0 {
        out.extend(chars.iter().skip(chars.len() - tail));
    }
    out
}

/// Truncate each result, then enforce a total budget on the joined block.
pub fn budget_tool_results(outputs: &[String], budget: ToolResultBudget) -> String {
    if outputs.is_empty() {
        return String::new();
    }
    let per = outputs
        .iter()
        .map(|output| truncate_tool_result(output, budget.per_result_chars))
        .collect::<Vec<_>>();
    let joined = per.join("\n\n");
    truncate_tool_result(&joined, budget.total_chars)
}

/// Same as [`budget_tool_results`], but preserve list shape (for resume / observe).
pub fn budget_tool_result_list(outputs: &[String], budget: ToolResultBudget) -> Vec<String> {
    let per: Vec<String> = outputs
        .iter()
        .map(|output| truncate_tool_result(output, budget.per_result_chars))
        .collect();
    let mut total = 0usize;
    let mut out = Vec::with_capacity(per.len());
    // Prefer newest results when the total budget is exhausted.
    for item in per.into_iter().rev() {
        let next = total.saturating_add(item.chars().count());
        if next > budget.total_chars && !out.is_empty() {
            let remaining = budget.total_chars.saturating_sub(total);
            if remaining > 64 {
                out.push(truncate_tool_result(&item, remaining));
            }
            break;
        }
        total = next;
        out.push(item);
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_head_and_tail() {
        let text = "A".repeat(100) + "MID" + &"Z".repeat(100);
        let truncated = truncate_tool_result(&text, 80);
        assert!(truncated.contains("truncated"));
        assert!(truncated.starts_with('A'));
        assert!(truncated.ends_with('Z'));
        assert!(truncated.chars().count() <= 80);
    }

    #[test]
    fn short_text_untouched() {
        assert_eq!(truncate_tool_result("hello", 100), "hello");
    }

    #[test]
    fn total_budget_caps_joined_block() {
        let outputs = vec!["x".repeat(5_000), "y".repeat(5_000), "z".repeat(5_000)];
        let budget = ToolResultBudget {
            per_result_chars: 4_000,
            total_chars: 6_000,
        };
        let joined = budget_tool_results(&outputs, budget);
        assert!(joined.chars().count() <= 6_000);
        assert!(joined.contains("truncated") || joined.len() < 15_000);
    }

    #[test]
    fn list_budget_prefers_newest() {
        let outputs = vec![
            "old-".to_string() + &"a".repeat(2_000),
            "mid-".to_string() + &"b".repeat(2_000),
            "new-".to_string() + &"c".repeat(100),
        ];
        let budget = ToolResultBudget {
            per_result_chars: 3_000,
            total_chars: 1_500,
        };
        let list = budget_tool_result_list(&outputs, budget);
        assert!(!list.is_empty());
        assert!(
            list.last().unwrap().starts_with("new-") || list.iter().any(|s| s.contains("new-"))
        );
        let total: usize = list.iter().map(|s| s.chars().count()).sum();
        assert!(total <= 1_500 + 64);
    }
}
