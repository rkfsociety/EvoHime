//! Tool result size budget for prompts / replan observe (Stage 7.33).

use std::env;

/// Caps how much tool output is fed back into planning / final model context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolResultBudget {
    /// Max characters kept per individual tool result.
    pub per_result_chars: usize,
}

impl ToolResultBudget {
    pub fn from_env() -> Self {
        Self {
            per_result_chars: env_usize("EVOHIME_TOOL_RESULT_MAX_CHARS", 6_000).clamp(512, 200_000),
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
}
