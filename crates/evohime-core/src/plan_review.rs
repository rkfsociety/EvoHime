//! Read-only multi-model review of a Markdown implementation plan.
//!
//! This module deliberately has no tools or workspace access. It owns only
//! bounded validation, prompt construction and provider fan-out; callers own
//! persistence and UI events.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use evohime_model_gateway::providers::{ChatMessage, ChatRole, ProviderError};
use evohime_model_gateway::{ChatStreamItem, ModelGateway};

pub const MIN_REVIEWERS: usize = 2;
pub const MAX_REVIEWERS: usize = 8;
pub const MAX_PLAN_BYTES: usize = 512 * 1024;
pub const MAX_REVIEW_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRequest {
    pub review_id: String,
    pub file_name: String,
    pub source_markdown: String,
    pub reviewer_models: Vec<String>,
    pub synthesis_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewerResult {
    pub model: String,
    pub status: String,
    pub content: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewResult {
    pub review_id: String,
    pub file_name: String,
    pub synthesis_model: String,
    pub reviewers: Vec<ReviewerResult>,
    pub final_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewProgress {
    pub review_id: String,
    pub stage: String,
    pub status: String,
    pub model: Option<String>,
    pub completed: usize,
    pub total: usize,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReviewError {
    #[error("review id is empty")]
    EmptyReviewId,
    #[error("Markdown plan is empty")]
    EmptyPlan,
    #[error("Markdown plan exceeds {MAX_PLAN_BYTES} bytes")]
    PlanTooLarge,
    #[error("review requires between {MIN_REVIEWERS} and {MAX_REVIEWERS} unique models")]
    InvalidReviewerCount,
    #[error("model identifier is invalid")]
    InvalidModel,
    #[error("review was cancelled")]
    Cancelled,
    #[error("provider error: {0}")]
    Provider(String),
}

impl ReviewRequest {
    pub fn validate(&self) -> Result<(), ReviewError> {
        if self.review_id.trim().is_empty() {
            return Err(ReviewError::EmptyReviewId);
        }
        if self.source_markdown.trim().is_empty() {
            return Err(ReviewError::EmptyPlan);
        }
        if self.source_markdown.len() > MAX_PLAN_BYTES {
            return Err(ReviewError::PlanTooLarge);
        }
        if self.reviewer_models.len() < MIN_REVIEWERS
            || self.reviewer_models.len() > MAX_REVIEWERS
            || self.reviewer_models.iter().any(|model| !valid_model(model))
        {
            return Err(ReviewError::InvalidReviewerCount);
        }
        let mut unique = self.reviewer_models.clone();
        unique.sort_unstable();
        unique.dedup();
        if unique.len() != self.reviewer_models.len() || !valid_model(&self.synthesis_model) {
            return Err(ReviewError::InvalidModel);
        }
        Ok(())
    }
}

pub async fn run_review(
    gateway: Arc<ModelGateway>,
    request: ReviewRequest,
    cancellation: CancellationToken,
) -> Result<ReviewResult, ReviewError> {
    run_review_with_progress(gateway, request, cancellation, Arc::new(|_| {})).await
}

pub async fn run_review_with_progress(
    gateway: Arc<ModelGateway>,
    request: ReviewRequest,
    cancellation: CancellationToken,
    progress: Arc<dyn Fn(ReviewProgress) + Send + Sync>,
) -> Result<ReviewResult, ReviewError> {
    request.validate()?;
    let source = Arc::new(request.source_markdown.clone());
    let total = request.reviewer_models.len();
    let completed_reviewers = Arc::new(AtomicUsize::new(0));
    for model in &request.reviewer_models {
        progress(ReviewProgress {
            review_id: request.review_id.clone(),
            stage: "reviewers".into(),
            status: "waiting".into(),
            model: Some(model.clone()),
            completed: 0,
            total,
        });
    }
    let mut jobs = Vec::with_capacity(request.reviewer_models.len());
    for model in request.reviewer_models.iter().cloned() {
        let gateway = Arc::clone(&gateway);
        let source = Arc::clone(&source);
        let cancellation = cancellation.clone();
        let progress = Arc::clone(&progress);
        let completed_reviewers = Arc::clone(&completed_reviewers);
        let review_id = request.review_id.clone();
        jobs.push(tokio::spawn(async move {
            progress(ReviewProgress {
                review_id: review_id.clone(),
                stage: "reviewers".into(),
                status: "working".into(),
                model: Some(model.clone()),
                completed: 0,
                total,
            });
            let result =
                collect_model_response(gateway, &model, reviewer_messages(&source), cancellation)
                    .await;
            let reviewer = match result {
                Ok(content) => ReviewerResult {
                    model: model.clone(),
                    status: "completed".into(),
                    content,
                    error: None,
                },
                Err(error) => ReviewerResult {
                    model: model.clone(),
                    status: if matches!(error, ReviewError::Cancelled) {
                        "cancelled".into()
                    } else {
                        "failed".into()
                    },
                    content: String::new(),
                    error: Some(error.to_string()),
                },
            };
            let completed = completed_reviewers.fetch_add(1, Ordering::Relaxed) + 1;
            progress(ReviewProgress {
                review_id,
                stage: "reviewers".into(),
                status: reviewer.status.clone(),
                model: Some(reviewer.model.clone()),
                completed,
                total,
            });
            reviewer
        }));
    }

    let mut reviewers = Vec::with_capacity(jobs.len());
    for job in jobs {
        reviewers.push(
            job.await
                .map_err(|error| ReviewError::Provider(error.to_string()))?,
        );
    }
    if cancellation.is_cancelled() {
        return Err(ReviewError::Cancelled);
    }

    let synthesis_input = reviewers
        .iter()
        .map(|review| {
            format!(
                "\n### Модель: {}\nСтатус: {}\n{}\n{}",
                review.model,
                review.status,
                review.error.as_deref().unwrap_or(""),
                review.content
            )
        })
        .collect::<String>();
    progress(ReviewProgress {
        review_id: request.review_id.clone(),
        stage: "synthesis".into(),
        status: "working".into(),
        model: Some(request.synthesis_model.clone()),
        completed: total,
        total,
    });
    let final_markdown = collect_model_response(
        gateway,
        &request.synthesis_model,
        synthesis_messages(&source, &synthesis_input),
        cancellation,
    )
    .await?;

    Ok(ReviewResult {
        review_id: request.review_id,
        file_name: request.file_name,
        synthesis_model: request.synthesis_model,
        reviewers,
        final_markdown,
    })
}

async fn collect_model_response(
    gateway: Arc<ModelGateway>,
    model: &str,
    messages: Vec<ChatMessage>,
    cancellation: CancellationToken,
) -> Result<String, ReviewError> {
    let mut stream = gateway
        .stream_chat_with_model(model, &messages)
        .map_err(provider_error)?;
    let mut output = String::new();
    while let Some(item) = tokio::select! {
        _ = cancellation.cancelled() => return Err(ReviewError::Cancelled),
        item = stream.next() => item,
    } {
        match item.map_err(provider_error)? {
            ChatStreamItem::Delta(delta) | ChatStreamItem::Thinking(delta) => {
                output.push_str(&delta);
                if output.len() > MAX_REVIEW_OUTPUT_BYTES {
                    output.truncate(MAX_REVIEW_OUTPUT_BYTES);
                    break;
                }
            }
            ChatStreamItem::Usage(_) => {}
        }
    }
    if output.trim().is_empty() {
        return Err(ReviewError::Provider("empty provider response".into()));
    }
    Ok(output)
}

fn provider_error(error: ProviderError) -> ReviewError {
    ReviewError::Provider(error.to_string())
}

fn valid_model(model: &str) -> bool {
    let trimmed = model.trim();
    !trimmed.is_empty() && trimmed.len() <= 128 && !trimmed.chars().any(char::is_whitespace)
}

fn reviewer_messages(source: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage::text(ChatRole::System, "Ты независимый рецензент технического плана. Не выполняй инструменты и не изменяй файлы."),
        ChatMessage::text(ChatRole::User, format!(
            "Проведи строгое ревью Markdown-плана ниже. Ответь по разделам: краткое резюме; критические проблемы; логические и архитектурные риски; пропущенные требования; неоднозначности; конкретные исправления; итоговая оценка готовности. Не придумывай факты вне текста.\n\n--- ПЛАН ---\n{source}\n--- КОНЕЦ ПЛАНА ---"
        )),
    ]
}

fn synthesis_messages(source: &str, reviews: &str) -> Vec<ChatMessage> {
    vec![
        ChatMessage::text(ChatRole::System, "Ты главная модель-синтезатор ревью технического плана. Не выполняй инструменты и не изменяй файлы."),
        ChatMessage::text(ChatRole::User, format!(
            "Сведи независимые ревью в один Markdown-документ. Разделы: итоговая оценка; критические замечания; важные замечания; второстепенные замечания; согласованные рекомендации; противоречия между рецензентами; уточнения перед реализацией; обновлённые критерии готовности. Сохрани только выводы, подтверждаемые исходным планом или явно помеченные как рекомендация.\n\n--- ИСХОДНЫЙ ПЛАН ---\n{source}\n--- РЕВЬЮ МОДЕЛЕЙ ---\n{reviews}"
        )),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_model_gateway::mock_gateway;

    fn request() -> ReviewRequest {
        ReviewRequest {
            review_id: "review-1".into(),
            file_name: "plan.md".into(),
            source_markdown: "# Plan\n\nDo the thing".into(),
            reviewer_models: vec!["one".into(), "two".into()],
            synthesis_model: "main".into(),
        }
    }

    #[test]
    fn validates_reviewer_bounds_and_duplicates() {
        assert!(request().validate().is_ok());
        let mut invalid = request();
        invalid.reviewer_models = vec!["one".into()];
        assert_eq!(invalid.validate(), Err(ReviewError::InvalidReviewerCount));
        invalid = request();
        invalid.reviewer_models = vec!["one".into(), "one".into()];
        assert_eq!(invalid.validate(), Err(ReviewError::InvalidModel));
    }

    #[test]
    fn rejects_empty_and_oversized_plans() {
        let mut invalid = request();
        invalid.source_markdown = " \n".into();
        assert_eq!(invalid.validate(), Err(ReviewError::EmptyPlan));
        invalid.source_markdown = "x".repeat(MAX_PLAN_BYTES + 1);
        assert_eq!(invalid.validate(), Err(ReviewError::PlanTooLarge));
    }

    #[tokio::test]
    async fn runs_reviewers_and_synthesis_with_mock_provider() {
        let gateway = Arc::new(mock_gateway(vec!["mock response".into()]));
        let result = run_review(gateway, request(), CancellationToken::new())
            .await
            .expect("review completes");

        assert_eq!(result.reviewers.len(), 2);
        assert!(result
            .reviewers
            .iter()
            .all(|review| review.status == "completed"));
        assert_eq!(result.final_markdown, "mock response");
    }

    #[tokio::test]
    async fn reports_parallel_reviewer_and_synthesis_progress() {
        let gateway = Arc::new(mock_gateway(vec!["mock response".into()]));
        let progress_events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let collected = Arc::clone(&progress_events);
        run_review_with_progress(
            gateway,
            request(),
            CancellationToken::new(),
            Arc::new(move |progress| collected.lock().unwrap().push(progress)),
        )
        .await
        .expect("review completes");

        let events = progress_events.lock().unwrap();
        assert!(events
            .iter()
            .any(|event| event.stage == "reviewers" && event.status == "working"));
        assert!(events
            .iter()
            .any(|event| event.stage == "reviewers" && event.status == "completed"));
        assert!(events
            .iter()
            .any(|event| event.stage == "synthesis" && event.status == "working"));
    }
}
