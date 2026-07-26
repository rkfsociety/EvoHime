//! Wave 3B golden tasks for extended reasoning evaluation.

use evohime_evals::{run_golden_task, GoldenTask, ScriptStep};
use serde_json::json;

#[tokio::test]
async fn thinking_simple_math_problem() {
    let task = GoldenTask {
        name: "math-with-thinking".to_string(),
        user_message: "What is 17 * 42? Show your reasoning.".to_string(),
        workspace_files: Default::default(),
        script: vec![
            ScriptStep::Reply {
                reply: "Let me calculate: 17 * 42.\n\n*thinking: I need to multiply 17 by 42. Let me break it down: 17 * 40 = 680, and 17 * 2 = 34. So 680 + 34 = 714.*\n\nThe answer is 714.".to_string(),
            },
        ],
        expect: evohime_evals::Expectations {
            final_message_contains: vec!["714".to_string()],
            files: vec![],
            thinking_contains: vec!["multiply 17 by 42".to_string()],
            min_thinking_tokens: Some(20), // At least ~80 chars of thinking
        },
        rubric: None,
    };

    let report = run_golden_task(&task).await;
    assert!(report.passed(), "failures: {:?}", report.failures);
}

#[tokio::test]
async fn thinking_absent_when_not_provided() {
    let task = GoldenTask {
        name: "no-thinking".to_string(),
        user_message: "Simple question: what is 2+2?".to_string(),
        workspace_files: Default::default(),
        script: vec![
            ScriptStep::Reply {
                reply: "The answer is 4.".to_string(),
            },
        ],
        expect: evohime_evals::Expectations {
            final_message_contains: vec!["4".to_string()],
            files: vec![],
            thinking_contains: vec![], // No thinking expected
            min_thinking_tokens: None,
        },
        rubric: None,
    };

    let report = run_golden_task(&task).await;
    assert!(report.passed(), "failures: {:?}", report.failures);
}

#[tokio::test]
async fn thinking_with_operator_normalization() {
    let task = GoldenTask {
        name: "thinking-math-operators".to_string(),
        user_message: "Calculate using mathematical operators".to_string(),
        workspace_files: Default::default(),
        script: vec![
            ScriptStep::Reply {
                reply: "Let me work through this.\n\n*thinking: I need to compute 15 ÷ 3 × 2. First: 15 ÷ 3 = 5. Then: 5 × 2 = 10. Also considering: 8 − 3 = 5.*\n\nResult is 10.".to_string(),
            },
        ],
        expect: evohime_evals::Expectations {
            final_message_contains: vec!["Result is 10".to_string()],
            files: vec![],
            // These should match after normalization (÷→/, ×→*, −→-)
            thinking_contains: vec!["15/3".to_string(), "5*2".to_string()],
            min_thinking_tokens: Some(30),
        },
        rubric: None,
    };

    let report = run_golden_task(&task).await;
    assert!(report.passed(), "failures: {:?}", report.failures);
}
