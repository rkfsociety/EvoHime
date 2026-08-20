//! Read-only multi-model review of a Markdown implementation plan, plus the
//! single-model revision that folds a finished review back into that plan.
//!
//! This module deliberately has no tools or workspace access. It owns only
//! bounded validation, prompt construction and provider fan-out; callers own
//! persistence and UI events. The revision half returns the rewritten plan as
//! a string for exactly that reason: writing it to disk belongs to the caller.

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

// Разделители, которыми промпты отбивают документы друг от друга. Модель
// нередко повторяет их в ответе, поэтому срез правки берёт те же константы:
// разъехавшись, они пустили бы служебную строку прямо в файл плана.
const PLAN_OPEN: &str = "--- ПЛАН ---";
const PLAN_CLOSE: &str = "--- КОНЕЦ ПЛАНА ---";
const REVIEW_OPEN: &str = "--- РЕВЬЮ ---";
const REVIEW_CLOSE: &str = "--- КОНЕЦ РЕВЬЮ ---";
const CONTEXT_OPEN: &str = "--- СОСЕДНИЕ ПЛАНЫ ---";
const CONTEXT_CLOSE: &str = "--- КОНЕЦ СОСЕДНИХ ПЛАНОВ ---";

/// Сколько связанных планов и сколько их текста уходит в промпт. Планы этапа
/// ссылаются друг на друга, и инвариант соседнего этапа сплошь и рядом не
/// повторён в самом файле: без него редактор уверенно перепишет план так, что
/// он начнёт противоречить соседям. Потолки держат промпт в пределах окна
/// модели — контекст обрезается, а не роняет правку.
pub const MAX_CONTEXT_DOCUMENTS: usize = 8;
pub const MAX_CONTEXT_BYTES: usize = 192 * 1024;
pub const MAX_CONTEXT_DEPTH: usize = 2;

/// Соседний план, приложенный к промпту только для сверки.
///
/// Ни ревью, ни правка не могут его изменить: он не попадает ни в ответ
/// модели, ни в файл — только в контекст запроса.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextDocument {
    pub file_name: String,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRequest {
    pub review_id: String,
    pub file_name: String,
    pub file_names: Vec<String>,
    pub source_markdown: String,
    pub reviewer_models: Vec<String>,
    pub synthesis_model: String,
    /// Планы, на которые ссылается проверяемый: рецензент видит их только для
    /// сверки. Пустой список — обычное дело для одиночного плана.
    pub context_documents: Vec<ContextDocument>,
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
    #[serde(default)]
    pub file_names: Vec<String>,
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
    #[error("review text is empty")]
    EmptyReview,
    #[error("Markdown plan exceeds {MAX_PLAN_BYTES} bytes")]
    PlanTooLarge,
    #[error("linked plans exceed {MAX_CONTEXT_DOCUMENTS} documents or {MAX_CONTEXT_BYTES} bytes")]
    ContextTooLarge,
    #[error("review requires between {MIN_REVIEWERS} and {MAX_REVIEWERS} unique models")]
    InvalidReviewerCount,
    #[error("model identifier is invalid")]
    InvalidModel,
    #[error("review was cancelled")]
    Cancelled,
    #[error("provider error: {0}")]
    Provider(String),
    #[error(
        "review cannot be synthesized: {failed} of {total} reviewers failed or were cancelled ({reason})"
    )]
    IncompleteReviewers {
        failed: usize,
        total: usize,
        /// Why the first reviewer gave up. Without it the user is told that a
        /// review failed but not whether to retry, switch models or wait.
        reason: String,
    },
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
        validate_context(&self.context_documents)?;
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
    // Reviewers run one at a time on purpose. Firing every model at once is
    // faster but walks straight into per-key rate limits, and a review that
    // fails halfway is worth less than one that takes longer.
    let mut reviewers = Vec::with_capacity(total);
    for model in request.reviewer_models.iter().cloned() {
        if cancellation.is_cancelled() {
            let completed = completed_reviewers.fetch_add(1, Ordering::Relaxed) + 1;
            progress(ReviewProgress {
                review_id: request.review_id.clone(),
                stage: "reviewers".into(),
                status: "cancelled".into(),
                model: Some(model.clone()),
                completed,
                total,
            });
            reviewers.push(ReviewerResult {
                model,
                status: "cancelled".into(),
                content: String::new(),
                error: Some(ReviewError::Cancelled.to_string()),
            });
            continue;
        }
        progress(ReviewProgress {
            review_id: request.review_id.clone(),
            stage: "reviewers".into(),
            status: "working".into(),
            model: Some(model.clone()),
            completed: completed_reviewers.load(Ordering::Relaxed),
            total,
        });
        let reviewer = match collect_model_response(
            Arc::clone(&gateway),
            &model,
            reviewer_messages(&source, &request.context_documents),
            cancellation.clone(),
        )
        .await
        {
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
            review_id: request.review_id.clone(),
            stage: "reviewers".into(),
            status: reviewer.status.clone(),
            model: Some(reviewer.model.clone()),
            completed,
            total,
        });
        reviewers.push(reviewer);
        if reviewers
            .last()
            .is_some_and(|review| review.status != "completed")
        {
            // A synthesis requires every reviewer. Once one provider fails,
            // starting later models only delays the same terminal failure and
            // makes the UI look as if the review is still progressing.
            for remaining_model in request.reviewer_models.iter().skip(reviewers.len()) {
                let completed = completed_reviewers.fetch_add(1, Ordering::Relaxed) + 1;
                progress(ReviewProgress {
                    review_id: request.review_id.clone(),
                    stage: "reviewers".into(),
                    status: "cancelled".into(),
                    model: Some(remaining_model.clone()),
                    completed,
                    total,
                });
                reviewers.push(ReviewerResult {
                    model: remaining_model.clone(),
                    status: "cancelled".into(),
                    content: String::new(),
                    error: Some(ReviewError::Cancelled.to_string()),
                });
            }
            break;
        }
    }
    if cancellation.is_cancelled() {
        return Err(ReviewError::Cancelled);
    }

    let failed = reviewers
        .iter()
        .filter(|review| review.status != "completed")
        .count();
    if failed > 0 {
        let reason = reviewers
            .iter()
            .find(|review| review.status != "completed")
            .map(|review| {
                format!(
                    "{}: {}",
                    review.model,
                    review.error.as_deref().unwrap_or("причина не сообщена")
                )
            })
            .unwrap_or_else(|| "причина не сообщена".to_string());
        return Err(ReviewError::IncompleteReviewers {
            failed,
            total,
            reason,
        });
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
    let synthesized_markdown = collect_model_response(
        gateway,
        &request.synthesis_model,
        synthesis_messages(&source, &synthesis_input),
        cancellation,
    )
    .await?;
    let final_markdown = format_review_markdown(
        &request.file_names,
        &request.reviewer_models,
        &request.synthesis_model,
        &synthesized_markdown,
    );

    Ok(ReviewResult {
        review_id: request.review_id,
        file_name: request.file_name,
        file_names: request.file_names,
        synthesis_model: request.synthesis_model,
        reviewers,
        final_markdown,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionRequest {
    pub revision_id: String,
    pub review_id: String,
    pub file_name: String,
    pub source_markdown: String,
    pub review_markdown: String,
    pub model: String,
    /// Те же соседние планы, что видел рецензент. Правка без них — главный
    /// способ получить внутренне складный план, противоречащий соседям.
    pub context_documents: Vec<ContextDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RevisionResult {
    pub revision_id: String,
    pub review_id: String,
    pub file_name: String,
    pub model: String,
    pub revised_markdown: String,
    /// С чем правка сверялась. Пользователь должен видеть это до сохранения:
    /// пустой список означает, что редактор работал вслепую по одному файлу.
    /// `default` — записи правок, сделанных до появления контекста, лежат в
    /// журнале без этого поля.
    #[serde(default)]
    pub context_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionProgress {
    pub revision_id: String,
    pub status: String,
    pub model: String,
}

impl RevisionRequest {
    pub fn validate(&self) -> Result<(), ReviewError> {
        if self.revision_id.trim().is_empty() || self.review_id.trim().is_empty() {
            return Err(ReviewError::EmptyReviewId);
        }
        if self.source_markdown.trim().is_empty() {
            return Err(ReviewError::EmptyPlan);
        }
        if self.source_markdown.len() > MAX_PLAN_BYTES {
            return Err(ReviewError::PlanTooLarge);
        }
        if self.review_markdown.trim().is_empty() {
            return Err(ReviewError::EmptyReview);
        }
        if !valid_model(&self.model) {
            return Err(ReviewError::InvalidModel);
        }
        validate_context(&self.context_documents)?;
        Ok(())
    }
}

/// Rewrites the plan the review was made for and returns it whole.
///
/// A diff would be cheaper to transfer, but models are far more reliable at
/// reproducing a document than at addressing hunks, and the caller shows the
/// result before anything touches the original file.
pub async fn run_revision(
    gateway: Arc<ModelGateway>,
    request: RevisionRequest,
    cancellation: CancellationToken,
    progress: Arc<dyn Fn(RevisionProgress) + Send + Sync>,
) -> Result<RevisionResult, ReviewError> {
    request.validate()?;
    // Checked before the request rather than only inside the stream: a revision
    // cancelled while it was still queued must not spend a provider call.
    if cancellation.is_cancelled() {
        return Err(ReviewError::Cancelled);
    }
    progress(RevisionProgress {
        revision_id: request.revision_id.clone(),
        status: "working".into(),
        model: request.model.clone(),
    });
    let revised = collect_model_response(
        gateway,
        &request.model,
        revision_messages(
            &request.source_markdown,
            &request.review_markdown,
            &request.context_documents,
        ),
        cancellation,
    )
    .await
    .inspect_err(|_| {
        progress(RevisionProgress {
            revision_id: request.revision_id.clone(),
            status: "failed".into(),
            model: request.model.clone(),
        })
    })?;
    progress(RevisionProgress {
        revision_id: request.revision_id.clone(),
        status: "completed".into(),
        model: request.model.clone(),
    });
    Ok(RevisionResult {
        revision_id: request.revision_id,
        review_id: request.review_id,
        file_name: request.file_name,
        model: request.model,
        revised_markdown: match_line_endings(&request.source_markdown, &as_plan_file(&revised)),
        context_files: request
            .context_documents
            .iter()
            .map(|document| document.file_name.clone())
            .collect(),
    })
}

/// Prepares a model answer to become the whole content of a plan file: the
/// stray ``` fence comes off, a prompt separator the model echoed back is cut
/// away with everything after it, and the trailing newline goes back on, so
/// that saving the revision does not leave the file without its final break.
fn as_plan_file(value: &str) -> String {
    let stripped = strip_markdown_fence(value);
    let end = [
        PLAN_CLOSE,
        PLAN_OPEN,
        REVIEW_OPEN,
        REVIEW_CLOSE,
        CONTEXT_OPEN,
        CONTEXT_CLOSE,
    ]
    .iter()
    .filter_map(|marker| stripped.find(marker))
    .min()
    .unwrap_or(stripped.len());
    let mut plan = stripped[..end].trim_end().to_string();
    if !plan.is_empty() && !plan.ends_with('\n') {
        plan.push('\n');
    }
    plan
}

/// Models routinely wrap a whole-document answer in a ``` fence despite being
/// told not to. Saving that verbatim would corrupt the plan, so it is peeled
/// off here rather than left for the user to notice.
fn strip_markdown_fence(value: &str) -> String {
    let trimmed = value.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed.to_string();
    };
    let Some((first_line, body)) = rest.split_once('\n') else {
        return trimmed.to_string();
    };
    // Only a bare fence or a language tag may precede the document itself.
    if !first_line.trim().chars().all(char::is_alphanumeric) {
        return trimmed.to_string();
    }
    match body.trim_end().strip_suffix("```") {
        Some(inner) => inner.trim_end().to_string(),
        None => trimmed.to_string(),
    }
}

fn format_review_markdown(
    file_names: &[String],
    reviewer_models: &[String],
    synthesis_model: &str,
    final_markdown: &str,
) -> String {
    let names = if file_names.is_empty() {
        vec!["(имя файла не передано)".to_string()]
    } else {
        file_names
            .iter()
            .map(|name| format!("- `{}`", safe_markdown_value(name)))
            .collect()
    };
    let reviewers = reviewer_models
        .iter()
        .map(|model| format!("- `{}`", safe_markdown_value(model)))
        .collect::<Vec<_>>();
    format!(
        "<!-- Контекст EvoHime: это ревью сделано по указанным файлам. -->\n\n## Модели, использованные для ревью\n\n- **Рецензенты:**\n{}\n- **Главная модель-синтезатор:** `{}`\n\n## Файлы, которые проверялись\n\n{}\n\n---\n\n{}",
        reviewers.join("\n"),
        safe_markdown_value(synthesis_model),
        names.join("\n"),
        final_markdown.trim_start()
    )
}

fn safe_markdown_value(value: &str) -> String {
    value.replace(['`', '\r', '\n'], " ")
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
            // Reasoning traces are not part of the review answer.
            ChatStreamItem::Thinking(_) => {}
            ChatStreamItem::Delta(delta) => {
                output.push_str(&delta);
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

/// Контекст режется по числу документов и по суммарному объёму, а не по факту
/// «влезло в окно»: провайдер отвечает на переполненный промпт отказом, и
/// правка падала бы тем позже, чем больше планов ссылаются друг на друга.
fn validate_context(context: &[ContextDocument]) -> Result<(), ReviewError> {
    if context.len() > MAX_CONTEXT_DOCUMENTS {
        return Err(ReviewError::ContextTooLarge);
    }
    let total: usize = context
        .iter()
        .map(|document| document.file_name.len() + document.markdown.len())
        .sum();
    if total > MAX_CONTEXT_BYTES {
        return Err(ReviewError::ContextTooLarge);
    }
    Ok(())
}

/// Имена файлов, на которые план ссылается Markdown-ссылкой.
///
/// Берутся только относительные ссылки на `.md` внутри каталога плана: путь с
/// `..`, абсолютный путь, диск и схема отбрасываются здесь, а не в вызывающем
/// коде, потому что именно этот список решает, какие файлы ядро откроет с
/// диска. Порядок появления сохраняется, повторы снимаются: первый упомянутый
/// сосед и есть самый близкий по смыслу.
pub fn linked_plan_names(markdown: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut index = 0;
    while let Some(found) = markdown[index..].find("](") {
        let start = index + found + 2;
        let Some(end) = markdown[start..].find(')') else {
            break;
        };
        let end = start + end;
        index = end + 1;
        let target = markdown[start..end].trim();
        let target = target.split('#').next().unwrap_or(target);
        let target = target.split(' ').next().unwrap_or(target);
        if !is_relative_markdown_link(target) {
            continue;
        }
        let name = target.replace('\\', "/");
        if !names.iter().any(|existing| existing == &name) {
            names.push(name);
        }
    }
    names
}

fn is_relative_markdown_link(target: &str) -> bool {
    if target.is_empty() || target.len() > 256 {
        return false;
    }
    if !target.to_ascii_lowercase().ends_with(".md") {
        return false;
    }
    if target.contains("://") || target.starts_with('/') || target.starts_with('\\') {
        return false;
    }
    // `C:\plans\x.md` уводит за пределы каталога плана так же, как `..`.
    if target.chars().nth(1) == Some(':') {
        return false;
    }
    !target
        .split(['/', '\\'])
        .any(|segment| segment == ".." || segment.is_empty())
}

/// Приводит перевод строки ответа к тому, что был в исходном файле.
///
/// Модель отвечает через LF всегда. Записать такой ответ поверх файла с CRLF
/// значит показать в git переписанным весь файл целиком, и настоящая правка
/// утонет в различиях, которых никто не вносил.
fn match_line_endings(source: &str, revised: &str) -> String {
    let normalized = revised.replace("\r\n", "\n");
    if source.contains("\r\n") {
        return normalized.replace('\n', "\r\n");
    }
    normalized
}

/// Указание про соседние планы добавляется только когда они есть: упоминание
/// блока, которого в промпте нет, модель принимает за потерянный ввод и
/// начинает его додумывать.
fn context_instruction(context: &[ContextDocument]) -> &'static str {
    if context.is_empty() {
        ""
    } else {
        " Ниже приложены планы, на которые проверяемый ссылается: они уже приняты и правке не подлежат. Сверь план с ними и отдельно назови места, где он им противоречит или ослабляет их инвариант. Замечаний к самим приложенным планам не давай."
    }
}

/// Правка видит те же соседние планы, что и рецензент, но с другим наказом:
/// рецензент про противоречие сообщает, редактор обязан его не создать.
fn revision_context_instruction(context: &[ContextDocument]) -> &'static str {
    if context.is_empty() {
        ""
    } else {
        " Ниже приложены планы, на которые ссылается исправляемый: они уже приняты, их текст менять нельзя и возвращать их не нужно. Ничего не пиши в план вопреки им — ни имён, ни потолков, ни разрешений. Если замечание ревью противоречит приложенному плану, сохрани инвариант приложенного плана и одной строкой отметь противоречие в тексте."
    }
}

/// Соседние планы идут после проверяемого и за собственными разделителями: так
/// модель не путает, какой документ она возвращает, а срез ответа отрезает эхо
/// разделителя вместе со всем, что модель успела за ним написать.
fn context_block(context: &[ContextDocument]) -> String {
    if context.is_empty() {
        return String::new();
    }
    let documents = context
        .iter()
        .map(|document| format!("\n\n### {}\n{}", document.file_name, document.markdown))
        .collect::<String>();
    format!("\n{CONTEXT_OPEN}{documents}\n{CONTEXT_CLOSE}")
}

fn reviewer_messages(source: &str, context: &[ContextDocument]) -> Vec<ChatMessage> {
    vec![
        ChatMessage::text(ChatRole::System, "Ты независимый рецензент технического плана. Не выполняй инструменты и не изменяй файлы."),
        ChatMessage::text(ChatRole::User, format!(
            "Проведи строгое ревью Markdown-плана ниже. Ответь по разделам: краткое резюме; критические проблемы; логические и архитектурные риски; пропущенные требования; неоднозначности; конкретные исправления; итоговая оценка готовности. Не придумывай факты вне текста.{}\n\n{PLAN_OPEN}\n{source}\n{PLAN_CLOSE}{}",
            context_instruction(context),
            context_block(context)
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

fn revision_messages(source: &str, review: &str, context: &[ContextDocument]) -> Vec<ChatMessage> {
    vec![
        ChatMessage::text(ChatRole::System, "Ты редактор технического плана. Не выполняй инструменты и не изменяй файлы. Твой ответ целиком становится новым содержимым файла плана."),
        ChatMessage::text(ChatRole::User, format!(
            "Исправь Markdown-план по замечаниям ревью. Верни весь исправленный план целиком, от первой строки до последней, без сопроводительного текста, без списка внесённых правок и без обрамляющих ``` блоков.\n\nПравь минимально. Меняй только то, к чему в ревью есть замечание; остальное — заголовки, абзацы, пункты списков, порядок разделов, формулировки — переноси дословно. Не переписывай текст ради стиля, не разворачивай пункт в подраздел и не заводи новых разделов, если ревью прямо этого не требует. Исправленный план должен быть сопоставим по объёму с исходным: заметный рост означает, что дописано лишнее.\n\nНе выдумывай фактов: имена файлов, типов, полей, команд, кодов событий, номера тегов и числовые потолки бери только из плана или из ревью. Если ревью требует уточнения, а взять его неоткуда, одной строкой напиши, что осталось нерешённым, вместо правдоподобной выдумки.\n\nНе ослабляй ограничения. Запрет, инвариант, «нельзя», «только после подтверждения», закрытый список правятся лишь тогда, когда ревью требует этого дословно; иначе формулировка запрета остаётся как была.{}\n\nЕсли ревью содержит взаимоисключающие требования, выбери одно и поясни выбор прямо в тексте плана.\n\n{PLAN_OPEN}\n{source}\n{PLAN_CLOSE}\n{REVIEW_OPEN}\n{review}\n{REVIEW_CLOSE}{}",
            revision_context_instruction(context),
            context_block(context)
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
            file_names: vec!["plan.md".into()],
            source_markdown: "# Plan\n\nDo the thing".into(),
            reviewer_models: vec!["one".into(), "two".into()],
            synthesis_model: "main".into(),
            context_documents: Vec::new(),
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
        assert!(result
            .final_markdown
            .contains("## Файлы, которые проверялись"));
        assert!(result.final_markdown.contains("`plan.md`"));
        assert!(result
            .final_markdown
            .contains("## Модели, использованные для ревью"));
        assert!(result.final_markdown.contains("`one`"));
        assert!(result.final_markdown.contains("`main`"));
    }

    /// Reviewers share one provider key, so they are queued rather than fanned
    /// out: the next model must not be asked until the previous one is done.
    #[tokio::test]
    async fn queues_reviewers_one_at_a_time() {
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
        let position = |model: &str, status: &str| {
            events
                .iter()
                .position(|event| event.model.as_deref() == Some(model) && event.status == status)
                .unwrap_or_else(|| panic!("{model} must report {status}"))
        };
        assert!(
            position("two", "working") > position("one", "completed"),
            "the second reviewer must start only after the first one finished"
        );
    }

    #[tokio::test]
    async fn reports_reviewer_and_synthesis_progress() {
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

    #[tokio::test]
    async fn cancels_reviewers_after_the_first_provider_failure() {
        let gateway = Arc::new(mock_gateway(Vec::new()));
        let progress_events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let collected = Arc::clone(&progress_events);
        let result = run_review_with_progress(
            gateway,
            request(),
            CancellationToken::new(),
            Arc::new(move |progress| collected.lock().unwrap().push(progress)),
        )
        .await;

        assert!(matches!(
            result,
            Err(ReviewError::IncompleteReviewers {
                failed: 2,
                total: 2,
                ..
            })
        ));
        let events = progress_events.lock().unwrap();
        assert!(!events
            .iter()
            .any(|event| event.model.as_deref() == Some("two") && event.status == "working"));
        assert!(events
            .iter()
            .any(|event| event.model.as_deref() == Some("two") && event.status == "cancelled"));
    }

    #[test]
    fn refuses_to_synthesize_when_a_reviewer_failed() {
        let reviewers = [
            ReviewerResult {
                model: "ok".into(),
                status: "completed".into(),
                content: "review".into(),
                error: None,
            },
            ReviewerResult {
                model: "offline".into(),
                status: "failed".into(),
                content: String::new(),
                error: Some("network timeout".into()),
            },
        ];

        let failed = reviewers
            .iter()
            .filter(|review| review.status != "completed")
            .count();
        assert_eq!(
            ReviewError::IncompleteReviewers {
                failed,
                total: reviewers.len(),
                reason: "offline: network timeout".into()
            },
            ReviewError::IncompleteReviewers {
                failed: 1,
                total: 2,
                reason: "offline: network timeout".into()
            }
        );
    }

    fn revision() -> RevisionRequest {
        RevisionRequest {
            revision_id: "revision-1".into(),
            review_id: "review-1".into(),
            file_name: "plan.md".into(),
            source_markdown: "# Plan\n\nDo the thing".into(),
            review_markdown: "Раздел про откат отсутствует.".into(),
            model: "main".into(),
            context_documents: Vec::new(),
        }
    }

    #[test]
    fn revision_requires_a_plan_a_review_and_a_model() {
        assert!(revision().validate().is_ok());
        let mut invalid = revision();
        invalid.review_markdown = " \n".into();
        assert_eq!(invalid.validate(), Err(ReviewError::EmptyReview));
        invalid = revision();
        invalid.source_markdown = "x".repeat(MAX_PLAN_BYTES + 1);
        assert_eq!(invalid.validate(), Err(ReviewError::PlanTooLarge));
        invalid = revision();
        invalid.model = "two words".into();
        assert_eq!(invalid.validate(), Err(ReviewError::InvalidModel));
        invalid = revision();
        invalid.revision_id = "  ".into();
        assert_eq!(invalid.validate(), Err(ReviewError::EmptyReviewId));
    }

    #[tokio::test]
    async fn revision_returns_the_whole_plan_and_reports_progress() {
        let gateway = Arc::new(mock_gateway(vec![
            "# Plan\n\nDo the thing\n\n## Откат".into()
        ]));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let result = run_revision(
            gateway,
            revision(),
            CancellationToken::new(),
            Arc::new(move |progress: RevisionProgress| {
                sink.lock().expect("progress lock").push(progress.status)
            }),
        )
        .await
        .expect("revision completes");

        assert_eq!(result.revision_id, "revision-1");
        assert_eq!(result.review_id, "review-1");
        assert_eq!(result.file_name, "plan.md");
        assert!(result.revised_markdown.starts_with("# Plan"));
        assert!(result.revised_markdown.contains("## Откат"));
        // Файл плана обязан заканчиваться переводом строки, как и исходный.
        assert!(result.revised_markdown.ends_with("\n"));
        assert_eq!(
            *seen.lock().expect("progress lock"),
            vec!["working".to_string(), "completed".to_string()]
        );
    }

    #[tokio::test]
    async fn revision_stops_when_cancelled() {
        let gateway = Arc::new(mock_gateway(vec!["# Plan".into()]));
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let error = run_revision(gateway, revision(), cancellation, Arc::new(|_| {}))
            .await
            .expect_err("cancelled revision fails");

        assert_eq!(error, ReviewError::Cancelled);
    }

    /// Регресс с живого прогона: модель повторила разделитель промпта, и
    /// строка `--- КОНЕЦ ПЛАНА ---` уехала в сохранённый файл плана.
    #[test]
    fn revision_cuts_off_an_echoed_prompt_separator() {
        assert_eq!(
            as_plan_file("# Plan\n\nДело\n\n--- КОНЕЦ ПЛАНА ---"),
            "# Plan\n\nДело\n"
        );
        // Ответ без разделителей не трогается, кроме финального перевода строки.
        assert_eq!(as_plan_file("# Plan"), "# Plan\n");
    }

    fn context(file_name: &str, markdown: &str) -> ContextDocument {
        ContextDocument {
            file_name: file_name.into(),
            markdown: markdown.into(),
        }
    }

    /// Список решает, какие файлы ядро откроет с диска, поэтому всё, что уводит
    /// за пределы каталога плана, отсеивается здесь.
    #[test]
    fn linked_names_take_relative_markdown_links_only() {
        let markdown = concat!(
            "Этап плана [04](04-0-ambient.md).\n",
            "Ещё раз [тот же](04-0-ambient.md) и [сосед](sub/04-1-contract.md).\n",
            "[вверх](../other/plan.md) [корень](/etc/plan.md) [сеть](https://example.com/plan.md)\n",
            "[диск](C:\\plans\\plan.md) [не план](notes.txt) [якорь](04-2-store.md#retention)\n"
        );

        assert_eq!(
            linked_plan_names(markdown),
            vec![
                "04-0-ambient.md".to_string(),
                "sub/04-1-contract.md".to_string(),
                "04-2-store.md".to_string(),
            ]
        );
    }

    #[test]
    fn context_is_bounded_by_documents_and_bytes() {
        let mut invalid = revision();
        invalid.context_documents = (0..MAX_CONTEXT_DOCUMENTS + 1)
            .map(|index| context(&format!("plan-{index}.md"), "x"))
            .collect();
        assert_eq!(invalid.validate(), Err(ReviewError::ContextTooLarge));

        invalid = revision();
        invalid.context_documents = vec![context("plan.md", &"x".repeat(MAX_CONTEXT_BYTES))];
        assert_eq!(invalid.validate(), Err(ReviewError::ContextTooLarge));

        let mut review = request();
        review.context_documents = vec![context("plan.md", &"x".repeat(MAX_CONTEXT_BYTES))];
        assert_eq!(review.validate(), Err(ReviewError::ContextTooLarge));
    }

    /// Соседний план едет в промпт целиком и с наказом не противоречить ему:
    /// без этого редактор уверенно переписывает план вразрез с соседями, а
    /// проверить это на живом провайдере нечем.
    #[test]
    fn prompts_carry_linked_plans_and_their_instruction() {
        let documents = vec![context("04-1-contract.md", "Хеш текста в лог не попадает.")];
        let prompt = revision_messages("# Plan", "Замечание", &documents)[1]
            .content
            .clone();
        assert!(prompt.contains(CONTEXT_OPEN));
        assert!(prompt.contains("04-1-contract.md"));
        assert!(prompt.contains("Хеш текста в лог не попадает."));
        assert!(prompt.contains("сохрани инвариант приложенного плана"));

        let reviewer_prompt = reviewer_messages("# Plan", &documents)[1].content.clone();
        assert!(reviewer_prompt.contains(CONTEXT_OPEN));
        assert!(reviewer_prompt.contains("Замечаний к самим приложенным планам не давай."));
    }

    /// Без соседей промпт не должен упоминать блок, которого в нём нет.
    #[test]
    fn prompts_stay_silent_about_context_when_there_is_none() {
        let prompt = revision_messages("# Plan", "Замечание", &[])[1]
            .content
            .clone();
        assert!(!prompt.contains(CONTEXT_OPEN));
        assert!(!prompt.contains("приложены планы"));
    }

    /// Модель отвечает через LF всегда, а план на диске — с CRLF: без
    /// выравнивания git показывает переписанным весь файл.
    #[tokio::test]
    async fn revision_keeps_the_line_endings_of_the_source_file() {
        let gateway = Arc::new(mock_gateway(vec!["# Plan\n\nДело\n\n## Откат".into()]));
        let mut request = revision();
        request.source_markdown = "# Plan\r\n\r\nДело\r\n".into();
        request.context_documents = vec![context("04-1-contract.md", "Инвариант.")];
        let result = run_revision(gateway, request, CancellationToken::new(), Arc::new(|_| {}))
            .await
            .expect("revision completes");

        assert!(!result.revised_markdown.contains("\n\n\r"));
        assert_eq!(result.revised_markdown.matches("\r\n").count(), 5);
        assert!(result.revised_markdown.ends_with("\r\n"));
        // Пользователь должен видеть, с чем сверялась правка.
        assert_eq!(result.context_files, vec!["04-1-contract.md".to_string()]);
    }

    /// План с LF остаётся с LF: выравнивание не должно вносить CR туда, где
    /// его не было.
    #[tokio::test]
    async fn revision_leaves_lf_sources_alone() {
        let gateway = Arc::new(mock_gateway(vec!["# Plan\r\n\r\nДело".into()]));
        let result = run_revision(
            gateway,
            revision(),
            CancellationToken::new(),
            Arc::new(|_| {}),
        )
        .await
        .expect("revision completes");

        assert!(!result.revised_markdown.contains('\r'));
        assert!(result.context_files.is_empty());
    }

    #[test]
    fn revision_peels_off_a_fenced_answer() {
        assert_eq!(strip_markdown_fence("```markdown\n# Plan\n```"), "# Plan");
        assert_eq!(strip_markdown_fence("```\n# Plan\n```"), "# Plan");
        // An unfenced document that merely contains a code block stays intact.
        assert_eq!(
            strip_markdown_fence("# Plan\n\n```bash\nls\n```"),
            "# Plan\n\n```bash\nls\n```"
        );
    }
}
