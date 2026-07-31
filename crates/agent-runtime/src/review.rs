//! Multi-model review engine: sequential reviewers -> synthesizer -> reviser -> self-check.
//! See docs/superpowers/plans/2026-07-31T1600-multi-model-review-engine.md.

use evohime_model_gateway::providers::{ChatMessage, ChatRole, ProviderError};
use evohime_model_gateway::ModelGateway;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Spec,
    Plan,
}

impl ArtifactKind {
    pub fn label(self) -> &'static str {
        match self {
            ArtifactKind::Spec => "specification",
            ArtifactKind::Plan => "implementation plan",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReviewerRoute {
    pub route_name: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewerComment {
    pub route_name: String,
    pub comments: String,
    pub failed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ReviewEngineError {
    #[error("no reviewers configured")]
    NoReviewers,
    #[error("all reviewers failed for this round")]
    AllReviewersFailed,
    #[error("model gateway error: {0}")]
    Gateway(#[from] ProviderError),
    #[error("storage error: {0}")]
    Storage(#[from] evohime_storage::StorageError),
}

/// Calls one reviewer with the artifact, from scratch (no visibility into
/// other reviewers). Retries exactly once; on a second failure returns a
/// `failed: true` comment instead of propagating the error, so one bad
/// reviewer never aborts the round (Global Constraints).
pub(crate) async fn call_reviewer(
    gateway: &ModelGateway,
    reviewer: &ReviewerRoute,
    artifact_kind: ArtifactKind,
    content: &str,
) -> ReviewerComment {
    let messages = vec![
        ChatMessage::text(
            ChatRole::System,
            format!(
                "You are an independent reviewer critiquing a software {}. \
                 List concrete issues, gaps, and risks, one per line. \
                 If you find nothing wrong, reply with exactly: NO ISSUES.",
                artifact_kind.label()
            ),
        ),
        ChatMessage::text(ChatRole::User, content),
    ];

    for _attempt in 0..2 {
        if let Ok(result) = gateway
            .chat_with_tools_for_route(&reviewer.route_name, reviewer.model.as_deref(), &messages, &[])
            .await
        {
            return ReviewerComment {
                route_name: reviewer.route_name.clone(),
                comments: result.content,
                failed: false,
            };
        }
    }

    ReviewerComment {
        route_name: reviewer.route_name.clone(),
        comments: String::new(),
        failed: true,
    }
}

/// Runs every configured reviewer strictly sequentially (rate limits — Global
/// Constraints), each reviewing `content` from scratch.
pub(crate) async fn run_reviewers(
    gateway: &ModelGateway,
    reviewer_routes: &[ReviewerRoute],
    artifact_kind: ArtifactKind,
    content: &str,
) -> Result<Vec<ReviewerComment>, ReviewEngineError> {
    if reviewer_routes.is_empty() {
        return Err(ReviewEngineError::NoReviewers);
    }

    let mut comments = Vec::with_capacity(reviewer_routes.len());
    for reviewer in reviewer_routes {
        comments.push(call_reviewer(gateway, reviewer, artifact_kind, content).await);
    }

    if comments.iter().all(|comment| comment.failed) {
        return Err(ReviewEngineError::AllReviewersFailed);
    }

    Ok(comments)
}

use serde_json::json;

#[derive(Debug, Clone)]
pub struct ReviewEngineConfig {
    pub reviewer_routes: Vec<ReviewerRoute>,
    pub synthesizer_route: ReviewerRoute,
    /// Model used for the reviser + self-check steps (the main agent model).
    pub main_route: ReviewerRoute,
    /// Hard cap on self-check iterations — a stability safety valve, not a
    /// cost optimization (Global Constraints).
    pub max_self_check_iterations: u32,
}

#[derive(Debug, Clone)]
pub struct ReviewRoundResult {
    pub reviewer_comments: Vec<ReviewerComment>,
    pub synthesized_feedback: String,
    pub revised_content: String,
    pub self_check_iterations: u32,
}

struct SelfCheckDecision {
    complete: bool,
    content: String,
}

#[derive(Deserialize)]
struct SelfCheckArgs {
    complete: bool,
    content: String,
}

fn self_check_tool() -> evohime_model_gateway::ToolSpec {
    evohime_model_gateway::ToolSpec::function(
        "submit_self_check",
        "Report whether the revised artifact fully addresses the review report.",
        json!({
            "type": "object",
            "properties": {
                "complete": {
                    "type": "boolean",
                    "description": "true if every issue from the review report is addressed"
                },
                "content": {
                    "type": "string",
                    "description": "the (possibly further-revised) artifact content"
                }
            },
            "required": ["complete", "content"]
        }),
    )
}

async fn synthesize_feedback(
    gateway: &ModelGateway,
    config: &ReviewEngineConfig,
    artifact_kind: ArtifactKind,
    reviewer_comments: &[ReviewerComment],
) -> Result<String, ReviewEngineError> {
    let joined = reviewer_comments
        .iter()
        .filter(|comment| !comment.failed)
        .enumerate()
        .map(|(index, comment)| format!("Reviewer {}:\n{}", index + 1, comment.comments))
        .collect::<Vec<_>>()
        .join("\n\n");

    let messages = vec![
        ChatMessage::text(
            ChatRole::System,
            format!(
                "You merge multiple independent reviews of a software {} into one report. \
                 Deduplicate overlapping points and keep every substantive issue. \
                 If every reviewer found nothing wrong, reply with exactly: NO ISSUES.",
                artifact_kind.label()
            ),
        ),
        ChatMessage::text(ChatRole::User, joined),
    ];

    let result = gateway
        .chat_with_tools_for_route(
            &config.synthesizer_route.route_name,
            config.synthesizer_route.model.as_deref(),
            &messages,
            &[],
        )
        .await?;
    Ok(result.content)
}

async fn revise_artifact(
    gateway: &ModelGateway,
    config: &ReviewEngineConfig,
    artifact_kind: ArtifactKind,
    content: &str,
    synthesized_feedback: &str,
) -> Result<String, ReviewEngineError> {
    let messages = vec![
        ChatMessage::text(
            ChatRole::System,
            format!(
                "Revise the {label} below to address every issue in the review report. \
                 Reply with the complete revised {label} only — no preamble, no commentary.",
                label = artifact_kind.label()
            ),
        ),
        ChatMessage::text(
            ChatRole::User,
            format!("--- ORIGINAL ---\n{content}\n\n--- REVIEW REPORT ---\n{synthesized_feedback}"),
        ),
    ];

    let result = gateway
        .chat_with_tools_for_route(
            &config.main_route.route_name,
            config.main_route.model.as_deref(),
            &messages,
            &[],
        )
        .await?;
    Ok(result.content)
}

async fn self_check(
    gateway: &ModelGateway,
    config: &ReviewEngineConfig,
    artifact_kind: ArtifactKind,
    content: &str,
    synthesized_feedback: &str,
) -> Result<SelfCheckDecision, ReviewEngineError> {
    let tool = self_check_tool();
    let messages = vec![
        ChatMessage::text(
            ChatRole::System,
            format!(
                "Check your own revision of this {label} against the review report. \
                 If anything is still unaddressed, fix it silently and call submit_self_check \
                 with complete=false and the fixed content. If everything is addressed, call \
                 submit_self_check with complete=true and the content unchanged.",
                label = artifact_kind.label()
            ),
        ),
        ChatMessage::text(
            ChatRole::User,
            format!("--- REVISED ---\n{content}\n\n--- REVIEW REPORT ---\n{synthesized_feedback}"),
        ),
    ];

    let result = gateway
        .chat_with_tools_for_route(
            &config.main_route.route_name,
            config.main_route.model.as_deref(),
            &messages,
            std::slice::from_ref(&tool),
        )
        .await?;

    let call = result
        .tool_calls
        .first()
        .ok_or_else(|| ReviewEngineError::Gateway(ProviderError::Api(
            "self-check did not call submit_self_check".into(),
        )))?;
    let parsed: SelfCheckArgs = serde_json::from_str(&call.arguments)
        .map_err(|error| ReviewEngineError::Gateway(ProviderError::Api(error.to_string())))?;

    Ok(SelfCheckDecision { complete: parsed.complete, content: parsed.content })
}

/// Runs one full review round: sequential reviewers -> synthesizer -> reviser
/// -> bounded silent self-check. See Global Constraints for the ordering and
/// failure-handling rules this implements.
pub async fn run_review_round(
    gateway: &ModelGateway,
    config: &ReviewEngineConfig,
    artifact_kind: ArtifactKind,
    content: &str,
) -> Result<ReviewRoundResult, ReviewEngineError> {
    let reviewer_comments =
        run_reviewers(gateway, &config.reviewer_routes, artifact_kind, content).await?;

    let synthesized_feedback =
        synthesize_feedback(gateway, config, artifact_kind, &reviewer_comments).await?;

    let mut revised_content =
        revise_artifact(gateway, config, artifact_kind, content, &synthesized_feedback).await?;

    let mut self_check_iterations = 0;
    while self_check_iterations < config.max_self_check_iterations {
        // If the self-check call itself fails (gateway error, or the model
        // replies without calling submit_self_check), keep the last good
        // revision and stop instead of discarding the whole round — a
        // misbehaving self-check step must never lose already-good work
        // (Global Constraints: stability over cost/latency).
        let decision = match self_check(
            gateway,
            config,
            artifact_kind,
            &revised_content,
            &synthesized_feedback,
        )
        .await
        {
            Ok(decision) => decision,
            Err(_) => break,
        };
        self_check_iterations += 1;
        revised_content = decision.content;
        if decision.complete {
            break;
        }
    }

    Ok(ReviewRoundResult {
        reviewer_comments,
        synthesized_feedback,
        revised_content,
        self_check_iterations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_model_gateway::providers::mock::MockProvider;
    use evohime_model_gateway::tools::ChatResult;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn gateway_with_routes(routes: Vec<(&str, MockProvider)>) -> ModelGateway {
        let mut map: HashMap<String, Arc<dyn evohime_model_gateway::providers::ModelProvider>> =
            HashMap::new();
        for (name, provider) in routes {
            map.insert(name.to_string(), Arc::new(provider));
        }
        ModelGateway::from_routes("reviewer_0", map)
    }

    #[tokio::test]
    async fn collects_comments_from_all_reviewers() {
        let gateway = gateway_with_routes(vec![
            (
                "reviewer_0",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult {
                        content: "missing tests".into(),
                        ..Default::default()
                    }],
                ),
            ),
            (
                "reviewer_1",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult {
                        content: "NO ISSUES".into(),
                        ..Default::default()
                    }],
                ),
            ),
        ]);
        let routes = vec![
            ReviewerRoute { route_name: "reviewer_0".into(), model: None },
            ReviewerRoute { route_name: "reviewer_1".into(), model: None },
        ];

        let comments = run_reviewers(&gateway, &routes, ArtifactKind::Plan, "plan text")
            .await
            .expect("reviewers run");

        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].comments, "missing tests");
        assert!(!comments[0].failed);
        assert_eq!(comments[1].comments, "NO ISSUES");
    }

    #[tokio::test]
    async fn skips_a_reviewer_after_two_failed_attempts() {
        // Route "reviewer_0" has no provider registered at all, so every
        // chat_with_tools_for_route call errors with "unknown model route" —
        // this simulates a reviewer that fails both attempts.
        let gateway = gateway_with_routes(vec![(
            "reviewer_1",
            MockProvider::with_tool_call_sequence(
                "m",
                vec![ChatResult { content: "ok".into(), ..Default::default() }],
            ),
        )]);
        let routes = vec![
            ReviewerRoute { route_name: "reviewer_0".into(), model: None },
            ReviewerRoute { route_name: "reviewer_1".into(), model: None },
        ];

        let comments = run_reviewers(&gateway, &routes, ArtifactKind::Plan, "plan text")
            .await
            .expect("round still succeeds — one reviewer survives");

        assert!(comments[0].failed);
        assert!(!comments[1].failed);
    }

    #[tokio::test]
    async fn errors_when_every_reviewer_fails() {
        let gateway = gateway_with_routes(vec![]);
        let routes = vec![ReviewerRoute { route_name: "reviewer_0".into(), model: None }];

        let error = run_reviewers(&gateway, &routes, ArtifactKind::Plan, "plan text")
            .await
            .expect_err("all reviewers failed");

        assert!(matches!(error, ReviewEngineError::AllReviewersFailed));
    }

    #[tokio::test]
    async fn run_review_round_completes_self_check_on_first_pass() {
        let gateway = gateway_with_routes(vec![
            (
                "reviewer_0",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "add tests".into(), ..Default::default() }],
                ),
            ),
            (
                "synthesizer",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "Add tests for step 2.".into(), ..Default::default() }],
                ),
            ),
            (
                "main",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![
                        // 1st call: revise_artifact (plain content)
                        ChatResult { content: "step 1\nstep 2 (with tests)".into(), ..Default::default() },
                        // 2nd call: self_check tool call, complete=true
                        ChatResult {
                            content: String::new(),
                            tool_calls: vec![evohime_model_gateway::NativeToolCall {
                                id: "call_1".into(),
                                name: "submit_self_check".into(),
                                arguments: r#"{"complete":true,"content":"step 1\nstep 2 (with tests)"}"#.into(),
                            }],
                            ..Default::default()
                        },
                    ],
                ),
            ),
        ]);
        let config = ReviewEngineConfig {
            reviewer_routes: vec![ReviewerRoute { route_name: "reviewer_0".into(), model: None }],
            synthesizer_route: ReviewerRoute { route_name: "synthesizer".into(), model: None },
            main_route: ReviewerRoute { route_name: "main".into(), model: None },
            max_self_check_iterations: 5,
        };

        let result = run_review_round(&gateway, &config, ArtifactKind::Plan, "step 1\nstep 2")
            .await
            .expect("round succeeds");

        assert_eq!(result.self_check_iterations, 1);
        assert_eq!(result.revised_content, "step 1\nstep 2 (with tests)");
        assert_eq!(result.synthesized_feedback, "Add tests for step 2.");
    }

    #[tokio::test]
    async fn run_review_round_stops_at_max_self_check_iterations() {
        let never_complete = ChatResult {
            content: String::new(),
            tool_calls: vec![evohime_model_gateway::NativeToolCall {
                id: "call_1".into(),
                name: "submit_self_check".into(),
                arguments: r#"{"complete":false,"content":"still working"}"#.into(),
            }],
            ..Default::default()
        };
        let gateway = gateway_with_routes(vec![
            (
                "reviewer_0",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "issue".into(), ..Default::default() }],
                ),
            ),
            (
                "synthesizer",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "fix the issue".into(), ..Default::default() }],
                ),
            ),
            (
                "main",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![
                        ChatResult { content: "revised".into(), ..Default::default() },
                        never_complete.clone(),
                        never_complete,
                    ],
                ),
            ),
        ]);
        let config = ReviewEngineConfig {
            reviewer_routes: vec![ReviewerRoute { route_name: "reviewer_0".into(), model: None }],
            synthesizer_route: ReviewerRoute { route_name: "synthesizer".into(), model: None },
            main_route: ReviewerRoute { route_name: "main".into(), model: None },
            max_self_check_iterations: 2,
        };

        let result = run_review_round(&gateway, &config, ArtifactKind::Plan, "content")
            .await
            .expect("round still returns instead of looping forever");

        assert_eq!(result.self_check_iterations, 2);
    }

    #[tokio::test]
    async fn run_review_round_survives_self_check_not_calling_the_tool() {
        let gateway = gateway_with_routes(vec![
            (
                "reviewer_0",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "issue".into(), ..Default::default() }],
                ),
            ),
            (
                "synthesizer",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![ChatResult { content: "fix it".into(), ..Default::default() }],
                ),
            ),
            (
                "main",
                MockProvider::with_tool_call_sequence(
                    "m",
                    vec![
                        ChatResult { content: "revised once".into(), ..Default::default() },
                        // Self-check replies with plain text instead of calling
                        // submit_self_check — simulates a model that ignores tool_choice.
                        ChatResult { content: "looks fine to me".into(), ..Default::default() },
                    ],
                ),
            ),
        ]);
        let config = ReviewEngineConfig {
            reviewer_routes: vec![ReviewerRoute { route_name: "reviewer_0".into(), model: None }],
            synthesizer_route: ReviewerRoute { route_name: "synthesizer".into(), model: None },
            main_route: ReviewerRoute { route_name: "main".into(), model: None },
            max_self_check_iterations: 5,
        };

        let result = run_review_round(&gateway, &config, ArtifactKind::Plan, "content")
            .await
            .expect("round still succeeds even if self-check misbehaves");

        assert_eq!(result.self_check_iterations, 0);
        assert_eq!(result.revised_content, "revised once");
    }
}
