//! GitHub auth and pull-request HTTP API (via local `gh`).
use crate::app::AppState;
use crate::ApiError;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, serde::Serialize)]
pub(crate) struct GithubAuthResponse {
    authenticated: bool,
    login: Option<String>,
    source: &'static str,
}

pub(crate) async fn github_auth() -> Json<GithubAuthResponse> {
    let login = std::process::Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    Json(GithubAuthResponse {
        authenticated: login.is_some(),
        login,
        source: "gh",
    })
}

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PullRequestScope {
    All,
    Created,
    ReviewRequested,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct GithubPullRequestUser {
    login: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GithubPullRequestSummary {
    number: u64,
    title: String,
    url: String,
    state: String,
    author: Option<GithubPullRequestUser>,
    head_ref_name: String,
    base_ref_name: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct GithubPullRequestQuery {
    #[serde(default)]
    scope: Option<PullRequestScope>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GithubPullRequestComment {
    author: Option<GithubPullRequestUser>,
    body: String,
    created_at: Option<String>,
    url: Option<String>,
    state: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GithubCheck {
    name: String,
    status: Option<String>,
    conclusion: Option<String>,
    details_url: Option<String>,
    workflow_name: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GithubPullRequestDetail {
    #[serde(flatten)]
    summary: GithubPullRequestSummary,
    body: String,
    is_draft: bool,
    merge_state_status: Option<String>,
    diff: String,
    comments: Vec<GithubPullRequestComment>,
    reviews: Vec<GithubPullRequestComment>,
    checks: Vec<GithubCheck>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct GithubCreatePullRequestRequest {
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    base: Option<String>,
    #[serde(default)]
    head: Option<String>,
}

pub(crate) async fn list_pull_requests(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GithubPullRequestQuery>,
) -> Result<Json<Vec<GithubPullRequestSummary>>, ApiError> {
    let workspace_root = state.workspace_root.clone();
    let scope = query.scope.unwrap_or(PullRequestScope::All);
    let result = tokio::task::spawn_blocking(move || {
        let mut command = std::process::Command::new("gh");
        command.current_dir(&workspace_root).args([
            "pr",
            "list",
            "--state",
            "all",
            "--limit",
            "40",
            "--json",
            "number,title,url,state,author,headRefName,baseRefName,createdAt,updatedAt",
        ]);

        match scope {
            PullRequestScope::All => {}
            PullRequestScope::Created => {
                command.args(["--search", "author:@me"]);
            }
            PullRequestScope::ReviewRequested => {
                command.args(["--search", "review-requested:@me"]);
            }
        }

        let output = command.output().map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }

        let prs = serde_json::from_slice::<Vec<GithubPullRequestSummary>>(&output.stdout)
            .map_err(|error| error.to_string())?;
        Ok::<_, String>(prs)
    })
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?;

    let prs = result.map_err(ApiError::Internal)?;
    Ok(Json(prs))
}

pub(crate) async fn get_pull_request(
    State(state): State<Arc<AppState>>,
    Path(number): Path<u64>,
) -> Result<Json<GithubPullRequestDetail>, ApiError> {
    let workspace_root = state.workspace_root.clone();
    let detail =
        tokio::task::spawn_blocking(move || load_pull_request_detail(&workspace_root, number))
            .await
            .map_err(|error| ApiError::Internal(error.to_string()))?
            .map_err(ApiError::Internal)?;
    Ok(Json(detail))
}

pub(crate) async fn create_pull_request(
    State(state): State<Arc<AppState>>,
    Json(request): Json<GithubCreatePullRequestRequest>,
) -> Result<Json<GithubPullRequestDetail>, ApiError> {
    let title = request.title.trim().to_string();
    if title.is_empty() {
        return Err(ApiError::BadRequest(
            "pull request title is required".to_string(),
        ));
    }

    let workspace_root = state.workspace_root.clone();
    let detail = tokio::task::spawn_blocking(move || {
        let mut args = vec![
            "pr".to_string(),
            "create".to_string(),
            "--title".to_string(),
            title,
            "--body".to_string(),
            request.body,
        ];
        if let Some(base) = request.base.filter(|value| !value.trim().is_empty()) {
            args.extend(["--base".to_string(), base]);
        }
        if let Some(head) = request.head.filter(|value| !value.trim().is_empty()) {
            args.extend(["--head".to_string(), head]);
        }

        let output = run_gh_command(&workspace_root, &args)?;
        let url = output
            .lines()
            .rev()
            .find(|line| line.trim().starts_with("http"))
            .map(str::trim)
            .ok_or_else(|| "gh pr create did not return a pull request URL".to_string())?;
        let created_number = run_gh_command(
            &workspace_root,
            &[
                "pr".to_string(),
                "view".to_string(),
                url.to_string(),
                "--json".to_string(),
                "number".to_string(),
            ],
        )?;
        let number = serde_json::from_str::<Value>(&created_number)
            .ok()
            .and_then(|value| value.get("number").and_then(Value::as_u64))
            .ok_or_else(|| {
                "gh pr view did not return the created pull request number".to_string()
            })?;
        load_pull_request_detail(&workspace_root, number)
    })
    .await
    .map_err(|error| ApiError::Internal(error.to_string()))?
    .map_err(ApiError::Internal)?;

    Ok(Json(detail))
}

pub(crate) fn load_pull_request_detail(
    workspace_root: &std::path::Path,
    number: u64,
) -> Result<GithubPullRequestDetail, String> {
    let json_output = run_gh_command(
        workspace_root,
        &[
            "pr".to_string(),
            "view".to_string(),
            number.to_string(),
            "--json".to_string(),
            "number,title,url,state,author,headRefName,baseRefName,createdAt,updatedAt,body,isDraft,mergeStateStatus,comments,reviews,statusCheckRollup".to_string(),
        ],
    )?;
    let value = serde_json::from_str::<Value>(&json_output).map_err(|error| error.to_string())?;
    let summary = serde_json::from_value::<GithubPullRequestSummary>(value.clone())
        .map_err(|error| error.to_string())?;
    let diff = run_gh_command(
        workspace_root,
        &["pr".to_string(), "diff".to_string(), number.to_string()],
    )?;

    Ok(GithubPullRequestDetail {
        summary,
        body: value
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        is_draft: value
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        merge_state_status: value
            .get("mergeStateStatus")
            .and_then(Value::as_str)
            .map(str::to_string),
        diff,
        comments: parse_pull_request_comments(value.get("comments")),
        reviews: parse_pull_request_comments(value.get("reviews")),
        checks: parse_checks(value.get("statusCheckRollup")),
    })
}

pub(crate) fn run_gh_command(workspace_root: &std::path::Path, args: &[String]) -> Result<String, String> {
    let output = std::process::Command::new("gh")
        .current_dir(workspace_root)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(crate) fn parse_pull_request_comments(value: Option<&Value>) -> Vec<GithubPullRequestComment> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| GithubPullRequestComment {
            author: item
                .get("author")
                .and_then(|author| serde_json::from_value(author.clone()).ok()),
            body: item
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            created_at: item
                .get("createdAt")
                .and_then(Value::as_str)
                .or_else(|| item.get("submittedAt").and_then(Value::as_str))
                .map(str::to_string),
            url: item.get("url").and_then(Value::as_str).map(str::to_string),
            state: item
                .get("state")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect()
}

pub(crate) fn parse_checks(value: Option<&Value>) -> Vec<GithubCheck> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|item| GithubCheck {
            name: item
                .get("name")
                .and_then(Value::as_str)
                .or_else(|| item.get("context").and_then(Value::as_str))
                .unwrap_or("check")
                .to_string(),
            status: item
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_string),
            conclusion: item
                .get("conclusion")
                .and_then(Value::as_str)
                .map(str::to_string),
            details_url: item
                .get("detailsUrl")
                .and_then(Value::as_str)
                .or_else(|| item.get("details_url").and_then(Value::as_str))
                .map(str::to_string),
            workflow_name: item
                .get("workflowName")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
        .collect()
}

