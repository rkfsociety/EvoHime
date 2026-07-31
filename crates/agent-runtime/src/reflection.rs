use evohime_protocol::{ErrorPattern, ReflectionAction, ReflectionAnalysis};

pub struct ReflectionEngine;

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
        let mut error_patterns = Vec::new();
        let mut success_score = 1.0f64;
        let mut reasoning = String::new();

        if let Some(err) = &context.tool_error {
            success_score = 0.0;
            reasoning.push_str(&format!("Tool error: {}", err));

            for (pattern_id, pattern_name, base_conf) in &known_failure_patterns {
                if err.contains(pattern_name) || pattern_name.to_lowercase().contains("error") {
                    error_patterns.push(ErrorPattern {
                        pattern_id: pattern_id.clone(),
                        pattern_name: pattern_name.clone(),
                        confidence: *base_conf,
                        source: "experience_memory".to_string(),
                    });
                }
            }
        } else {
            let output_lower = context.tool_output.to_lowercase();

            if output_lower.contains("failed") || output_lower.contains("error") || output_lower.is_empty() {
                success_score *= 0.5;
                reasoning.push_str("Output contains failure indicators or is empty. ");
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
                    if context.tool_output.contains("not found") || context.tool_output.contains("No such") {
                        success_score *= 0.2;
                        reasoning.push_str("Shell: command not found or missing file. ");
                    }
                }
                "git.commit" => {
                    if context.tool_output.contains("nothing to commit") {
                        success_score *= 0.5;
                        reasoning.push_str("Git: nothing to commit. ");
                    }
                }
                _ => {}
            }
        }

        let confidence = if success_score > 0.7 { 0.9 } else { 0.6 };

        let analysis = ReflectionAnalysis {
            success_score: success_score.max(0.0).min(1.0),
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

        let (analysis, action) = ReflectionEngine::analyze_tool_output(&context, vec![
            ("E001".to_string(), "not found".to_string(), 0.8),
        ]);

        assert_eq!(analysis.success_score, 0.0);
        assert!(!analysis.error_patterns.is_empty());
        assert_eq!(action, ReflectionAction::RetryTool);
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
