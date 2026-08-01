use evohime_protocol::{ErrorPattern, ReflectionAction, ReflectionAnalysis};

pub struct ReflectionEngine;

/// Silent-failure markers. The tool runtime reports its own failures in Russian
/// ("завершился с ошибкой"), so English-only markers would miss them.
const FAILURE_MARKERS: [&str; 6] = [
    "failed",
    "error",
    "ошиб",
    "не найден",
    "не удалось",
    "traceback",
];

/// Significant tokens of a failure-pattern phrase: short words ("the", "not", "is")
/// carry no signal and would make every pattern match every error.
fn significant_tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().count() >= 4)
        .map(str::to_string)
        .collect()
}

/// A remembered failure pattern matches an observation when at least 60% of its
/// significant tokens occur in the text. Substring equality is useless here:
/// experience memory stores whole sentences, not literal error strings.
fn pattern_matches(pattern: &str, haystack_lower: &str) -> bool {
    let tokens = significant_tokens(pattern);
    if tokens.is_empty() {
        return false;
    }
    let hits = tokens
        .iter()
        .filter(|token| haystack_lower.contains(token.as_str()))
        .count();
    hits * 5 >= tokens.len() * 3
}

fn matched_patterns(
    known_failure_patterns: &[(String, String, f64)],
    haystack: &str,
) -> Vec<ErrorPattern> {
    let haystack_lower = haystack.to_lowercase();
    known_failure_patterns
        .iter()
        .filter(|(_, pattern_name, _)| pattern_matches(pattern_name, &haystack_lower))
        .map(|(pattern_id, pattern_name, base_conf)| ErrorPattern {
            pattern_id: pattern_id.clone(),
            pattern_name: pattern_name.clone(),
            confidence: base_conf.clamp(0.0, 1.0),
            source: "experience_memory".to_string(),
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct ToolOutputContext {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_output: String,
    pub tool_error: Option<String>,
    pub expected_outcome: Option<String>,
}

impl ReflectionEngine {
    pub fn analyze_tool_output(
        context: &ToolOutputContext,
        known_failure_patterns: Vec<(String, String, f64)>,
    ) -> (ReflectionAnalysis, ReflectionAction) {
        let error_patterns;
        let mut success_score = 1.0f64;
        let mut reasoning = String::new();

        if let Some(err) = &context.tool_error {
            success_score = 0.0;
            reasoning.push_str(&format!("Tool error: {}", err));
            error_patterns = matched_patterns(&known_failure_patterns, err);
        } else {
            let output_lower = context.tool_output.to_lowercase();

            if output_lower.is_empty()
                || FAILURE_MARKERS
                    .iter()
                    .any(|marker| output_lower.contains(marker))
            {
                success_score *= 0.5;
                reasoning.push_str("Output contains failure indicators or is empty. ");
            }

            // Check failure patterns against output as well: silent failures never
            // surface as `tool_error`.
            error_patterns = matched_patterns(&known_failure_patterns, &context.tool_output);
            if !error_patterns.is_empty() {
                success_score *= 0.5;
                reasoning.push_str("Output matches a remembered failure pattern. ");
            }

            if let Some(expected) = &context.expected_outcome {
                if !context.tool_output.contains(expected) {
                    success_score *= 0.7;
                    reasoning.push_str(&format!("Output doesn't match expected: {}. ", expected));
                }
            }

            match context.tool_name.as_str() {
                "filesystem.read" => {
                    if context.tool_output.is_empty() {
                        success_score *= 0.3;
                        reasoning.push_str("Read returned empty content. ");
                    }
                }
                "shell.execute" => {
                    if context.tool_output.contains("not found")
                        || context.tool_output.contains("No such")
                    {
                        success_score *= 0.2;
                        reasoning.push_str("Shell: command not found or missing file. ");
                    }
                }
                "git.commit" if context.tool_output.contains("nothing to commit") => {
                    success_score *= 0.5;
                    reasoning.push_str("Git: nothing to commit. ");
                }
                _ => {}
            }
        }

        // A remembered pattern is direct evidence about this observation, so it raises
        // how sure the verdict is (never above the pattern's own confidence).
        let base_confidence = if success_score > 0.7 { 0.9 } else { 0.6 };
        let confidence = error_patterns
            .iter()
            .map(|pattern| pattern.confidence)
            .fold(base_confidence, f64::max)
            .clamp(0.0, 1.0);

        let analysis = ReflectionAnalysis {
            success_score: success_score.clamp(0.0, 1.0),
            error_patterns,
            confidence,
            reasoning: if reasoning.is_empty() {
                "Tool executed successfully".to_string()
            } else {
                reasoning
            },
        };

        let action = if success_score >= 0.8 {
            ReflectionAction::Proceed
        } else if success_score < 0.3 {
            ReflectionAction::RetryTool
        } else {
            ReflectionAction::AskUser
        };

        (analysis, action)
    }

    /// Short, model-facing hint appended to the tool observation. `None` for
    /// `Proceed` — a healthy step must not spend context on reflection noise.
    pub fn recommendation(
        action: &ReflectionAction,
        analysis: &ReflectionAnalysis,
        tool_name: &str,
    ) -> Option<String> {
        let patterns = analysis
            .error_patterns
            .iter()
            .map(|pattern| pattern.pattern_name.trim())
            .filter(|name| !name.is_empty())
            .take(2)
            .collect::<Vec<_>>()
            .join("; ");
        let known = if patterns.is_empty() {
            String::new()
        } else {
            format!(" Known failure pattern: {patterns}.")
        };
        match action {
            ReflectionAction::Proceed => None,
            ReflectionAction::RetryTool => Some(format!(
                "Reflection: `{tool_name}` looks failed (score {:.2}). {}{} Fix the arguments or pick another tool before repeating it.",
                analysis.success_score, analysis.reasoning.trim(), known
            )),
            ReflectionAction::AskUser => Some(format!(
                "Reflection: `{tool_name}` result is doubtful (score {:.2}). {}{} Verify the result before building on it; ask the user if it stays unclear.",
                analysis.success_score, analysis.reasoning.trim(), known
            )),
            ReflectionAction::RevisePlan => Some(format!(
                "Reflection: repeated failures around `{tool_name}`. {}{} Stop retrying this approach and revise the plan.",
                analysis.reasoning.trim(), known
            )),
            ReflectionAction::Escalate => Some(format!(
                "Reflection: `{tool_name}` failed critically. {}{} Report the blocker instead of continuing.",
                analysis.reasoning.trim(), known
            )),
        }
    }

    pub fn should_revise_plan(action: &ReflectionAction, consecutive_failures: usize) -> bool {
        matches!(action, ReflectionAction::RevisePlan) || consecutive_failures >= 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_explicit_error() {
        let context = ToolOutputContext {
            tool_name: "filesystem.read".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: "".to_string(),
            tool_error: Some("File not found: /nonexistent".to_string()),
            expected_outcome: None,
        };

        let (analysis, action) = ReflectionEngine::analyze_tool_output(
            &context,
            vec![("E001".to_string(), "not found".to_string(), 0.8)],
        );

        assert_eq!(analysis.success_score, 0.0);
        assert!(!analysis.error_patterns.is_empty());
        assert_eq!(action, ReflectionAction::RetryTool);
    }

    #[test]
    fn unrelated_remembered_pattern_does_not_match() {
        let context = ToolOutputContext {
            tool_name: "shell.execute".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: String::new(),
            tool_error: Some("connection refused by database".to_string()),
            expected_outcome: None,
        };

        let (analysis, _) = ReflectionEngine::analyze_tool_output(
            &context,
            vec![(
                "M1".to_string(),
                "migration applied before backup existed".to_string(),
                0.9,
            )],
        );

        assert!(analysis.error_patterns.is_empty());
    }

    #[test]
    fn remembered_pattern_downgrades_silent_success() {
        let context = ToolOutputContext {
            tool_name: "git.commit".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: "detached HEAD state, commit is unreachable".to_string(),
            tool_error: None,
            expected_outcome: None,
        };

        let (analysis, action) = ReflectionEngine::analyze_tool_output(
            &context,
            vec![(
                "M2".to_string(),
                "commit in detached HEAD state becomes unreachable".to_string(),
                0.85,
            )],
        );

        assert_eq!(analysis.error_patterns.len(), 1);
        assert!(analysis.success_score < 0.8);
        assert!(analysis.confidence >= 0.85);
        assert_ne!(action, ReflectionAction::Proceed);
        assert!(ReflectionEngine::recommendation(&action, &analysis, "git.commit").is_some());
    }

    #[test]
    fn test_analyze_successful_output() {
        let context = ToolOutputContext {
            tool_name: "filesystem.read".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: "file contents here".to_string(),
            tool_error: None,
            expected_outcome: Some("file contents".to_string()),
        };

        let (analysis, action) = ReflectionEngine::analyze_tool_output(&context, vec![]);

        assert!(analysis.success_score > 0.8);
        assert_eq!(action, ReflectionAction::Proceed);
    }

    #[test]
    fn test_analyze_silent_failure() {
        let context = ToolOutputContext {
            tool_name: "shell.execute".to_string(),
            tool_input: serde_json::json!({}),
            tool_output: "command not found".to_string(),
            tool_error: None,
            expected_outcome: Some("success".to_string()),
        };

        let (analysis, action) = ReflectionEngine::analyze_tool_output(&context, vec![]);

        assert!(analysis.success_score < 0.3);
        assert_eq!(action, ReflectionAction::RetryTool);
    }
}
