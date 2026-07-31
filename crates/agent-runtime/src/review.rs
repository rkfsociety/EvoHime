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
}
