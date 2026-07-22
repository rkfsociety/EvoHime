//! Structured HTTP error taxonomy (Stage 7.27).
//!
//! Error JSON shape (backward compatible `error` string + taxonomy fields):
//! ```json
//! {
//!   "error": "human message",
//!   "code": "bad_request",
//!   "retryable": false
//! }
//! ```

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    #[error("approval required for {tool}: {approval_id}")]
    ApprovalRequired { tool: String, approval_id: Uuid },
    #[error("{0}")]
    TooManyRequests(String),
    #[error("{0}")]
    Internal(String),
    #[error("service unavailable: {0}")]
    Unavailable(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
}

impl From<(Uuid, ApiError)> for ApiError {
    fn from((_, error): (Uuid, ApiError)) -> Self {
        error
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadRequest,
    Conflict,
    ApprovalRequired,
    RateLimited,
    Internal,
    Unavailable,
    Unauthorized,
    Forbidden,
    NotFound,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BadRequest => "bad_request",
            Self::Conflict => "conflict",
            Self::ApprovalRequired => "approval_required",
            Self::RateLimited => "rate_limited",
            Self::Internal => "internal",
            Self::Unavailable => "unavailable",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    /// Legacy human-readable message (also duplicated in `message` for clarity).
    pub error: String,
    pub code: ErrorCode,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<Uuid>,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        let error = message.into();
        Self {
            message: Some(error.clone()),
            error,
            code,
            retryable,
            tool: None,
            approval_id: None,
        }
    }

    pub fn to_json(&self) -> Value {
        json!(self)
    }
}

impl ApiError {
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::BadRequest(_) => ErrorCode::BadRequest,
            Self::Conflict(_) => ErrorCode::Conflict,
            Self::ApprovalRequired { .. } => ErrorCode::ApprovalRequired,
            Self::TooManyRequests(_) => ErrorCode::RateLimited,
            Self::Internal(_) => ErrorCode::Internal,
            Self::Unavailable(_) => ErrorCode::Unavailable,
            Self::Unauthorized(_) => ErrorCode::Unauthorized,
            Self::Forbidden(_) => ErrorCode::Forbidden,
            Self::NotFound(_) => ErrorCode::NotFound,
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::TooManyRequests(_) | Self::Unavailable(_) | Self::Internal(_)
        )
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) | Self::ApprovalRequired { .. } => StatusCode::CONFLICT,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
        }
    }

    pub fn body(&self) -> ErrorBody {
        let message = match self {
            Self::Internal(detail) => {
                let safe_detail = crate::log_safety::redact_for_log(detail);
                tracing::error!(error = %safe_detail, "internal API error");
                "Internal server error".to_string()
            }
            _ => self.to_string(),
        };
        let mut body = ErrorBody::new(self.code(), message, self.retryable());
        if let Self::ApprovalRequired { tool, approval_id } = self {
            body.tool = Some(tool.clone());
            body.approval_id = Some(*approval_id);
        }
        body
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = self.body();
        (status, Json(body.to_json())).into_response()
    }
}

/// Shared unauthorized JSON for the auth middleware.
pub fn unauthorized_json() -> Value {
    ErrorBody::new(
        ErrorCode::Unauthorized,
        "Missing or invalid API token. Use Authorization: Bearer <EVOHIME_API_TOKEN>, or connect from loopback when token is unset.",
        false,
    )
    .to_json()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxonomy_marks_rate_limit_and_unavailable_retryable() {
        assert!(ApiError::TooManyRequests("slow down".into()).retryable());
        assert!(ApiError::Unavailable("worker down".into()).retryable());
        assert!(ApiError::Internal("db blip".into()).retryable());
        assert!(!ApiError::BadRequest("nope".into()).retryable());
        assert!(!ApiError::Conflict("lease taken".into()).retryable());
    }

    #[test]
    fn body_includes_code_and_legacy_error_string() {
        let body = ApiError::BadRequest("path required".into()).body();
        let value = body.to_json();
        assert_eq!(value["error"], "path required");
        assert_eq!(value["code"], "bad_request");
        assert_eq!(value["retryable"], false);
    }

    #[test]
    fn approval_required_exposes_ids() {
        let approval_id = Uuid::new_v4();
        let body = ApiError::ApprovalRequired {
            tool: "shell.execute".into(),
            approval_id,
        }
        .body();
        let value = body.to_json();
        assert_eq!(value["code"], "approval_required");
        assert_eq!(value["tool"], "shell.execute");
        assert_eq!(value["approval_id"], approval_id.to_string());
        assert_eq!(value["retryable"], false);
    }

    #[test]
    fn internal_error_does_not_expose_details() {
        let value = ApiError::Internal("database password=secret".into())
            .body()
            .to_json();
        assert_eq!(value["code"], "internal");
        assert_eq!(value["error"], "Internal server error");
        assert!(!value.to_string().contains("password=secret"));
    }
}
