//! Bounded runtime policy for read-only child tasks.
//!
//! This module is intentionally standalone until the Core delegation runner is
//! wired. It validates the boundary before any child can receive context or
//! return a report: no nested children, elevation, writes, shell, or network
//! mutation are expressible as an accepted request.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt};

pub const MAX_ID_CHARS: usize = 128;
pub const MAX_ROLE_CHARS: usize = 64;
pub const MAX_CONTEXT_ITEMS: usize = 32;
pub const MAX_CONTEXT_ITEM_CHARS: usize = 2_048;
pub const MAX_CONTEXT_BYTES: usize = 16 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 32 * 1024;
pub const MAX_REPORT_CHARS: usize = 8_192;
pub const MAX_SOURCES: usize = 32;
pub const MAX_SOURCE_CHARS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildTaskKind {
    CodeSearch,
    ThreatModelReview,
    TestPlanReview,
    Documentation,
    Onboarding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildReportStatus {
    Complete,
    Partial,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildRuntimeError {
    EmptyField(&'static str),
    FieldTooLong { field: &'static str, max: usize },
    TooManyItems { field: &'static str, max: usize },
    ContextTooLarge { actual: usize, max: usize },
    OutputTooLarge { actual: usize, max: usize },
    ForbiddenCapability(String),
    NestedChildForbidden,
    TaskMismatch,
    DuplicateSource,
    SecretLikeContent,
}

impl fmt::Display for ChildRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::TooManyItems { field, max } => write!(f, "{field} exceeds {max} items"),
            Self::ContextTooLarge { actual, max } => {
                write!(f, "context is {actual} bytes, maximum is {max}")
            }
            Self::OutputTooLarge { actual, max } => {
                write!(f, "output is {actual} bytes, maximum is {max}")
            }
            Self::ForbiddenCapability(capability) => {
                write!(f, "forbidden child capability: {capability}")
            }
            Self::NestedChildForbidden => write!(f, "nested child delegation is forbidden"),
            Self::TaskMismatch => write!(f, "report child_task_id does not match request"),
            Self::DuplicateSource => write!(f, "report contains duplicate sources"),
            Self::SecretLikeContent => write!(f, "secret-like content is not allowed"),
        }
    }
}

impl std::error::Error for ChildRuntimeError {}

/// The accepted request has only read-oriented capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildTaskRequest {
    pub child_task_id: String,
    pub parent_task_id: String,
    pub role: String,
    pub kind: ChildTaskKind,
    pub reduced_context: Vec<String>,
    pub max_output_bytes: usize,
    pub requested_capabilities: Vec<String>,
    pub parent_is_child: bool,
}

impl ChildTaskRequest {
    pub fn validate(&self) -> Result<(), ChildRuntimeError> {
        validate_text("child_task_id", &self.child_task_id, MAX_ID_CHARS)?;
        validate_text("parent_task_id", &self.parent_task_id, MAX_ID_CHARS)?;
        validate_text("role", &self.role, MAX_ROLE_CHARS)?;
        if self.parent_is_child {
            return Err(ChildRuntimeError::NestedChildForbidden);
        }
        if self.reduced_context.len() > MAX_CONTEXT_ITEMS {
            return Err(ChildRuntimeError::TooManyItems {
                field: "reduced_context",
                max: MAX_CONTEXT_ITEMS,
            });
        }
        let context_bytes = self
            .reduced_context
            .iter()
            .try_fold(0usize, |total, item| {
                validate_text("context_item", item, MAX_CONTEXT_ITEM_CHARS)?;
                Ok::<_, ChildRuntimeError>(total.saturating_add(item.len()))
            })?;
        if context_bytes > MAX_CONTEXT_BYTES {
            return Err(ChildRuntimeError::ContextTooLarge {
                actual: context_bytes,
                max: MAX_CONTEXT_BYTES,
            });
        }
        if self.max_output_bytes == 0 || self.max_output_bytes > MAX_OUTPUT_BYTES {
            return Err(ChildRuntimeError::OutputTooLarge {
                actual: self.max_output_bytes,
                max: MAX_OUTPUT_BYTES,
            });
        }
        for capability in &self.requested_capabilities {
            validate_text("capability", capability, MAX_ROLE_CHARS)?;
            if !is_read_only_capability(capability) {
                return Err(ChildRuntimeError::ForbiddenCapability(capability.clone()));
            }
        }
        Ok(())
    }

    pub fn deterministic_json(&self) -> Result<String, ChildRuntimeError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|_| ChildRuntimeError::FieldTooLong {
            field: "serialized_request",
            max: MAX_CONTEXT_BYTES,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildReport {
    pub child_task_id: String,
    pub status: ChildReportStatus,
    pub summary: String,
    pub findings: Vec<String>,
    pub sources: Vec<String>,
    pub confidence_percent: u8,
}

impl ChildReport {
    pub fn validate(&self) -> Result<(), ChildRuntimeError> {
        validate_text("child_task_id", &self.child_task_id, MAX_ID_CHARS)?;
        validate_text("summary", &self.summary, MAX_REPORT_CHARS)?;
        if self.findings.len() > MAX_CONTEXT_ITEMS {
            return Err(ChildRuntimeError::TooManyItems {
                field: "findings",
                max: MAX_CONTEXT_ITEMS,
            });
        }
        for finding in &self.findings {
            validate_text("finding", finding, MAX_REPORT_CHARS)?;
            reject_secret_like(finding)?;
        }
        if self.sources.len() > MAX_SOURCES {
            return Err(ChildRuntimeError::TooManyItems {
                field: "sources",
                max: MAX_SOURCES,
            });
        }
        let mut sources = BTreeSet::new();
        for source in &self.sources {
            validate_text("source", source, MAX_SOURCE_CHARS)?;
            reject_secret_like(source)?;
            if !sources.insert(source) {
                return Err(ChildRuntimeError::DuplicateSource);
            }
        }
        reject_secret_like(&self.summary)?;
        let serialized =
            serde_json::to_vec(self).map_err(|_| ChildRuntimeError::OutputTooLarge {
                actual: usize::MAX,
                max: MAX_OUTPUT_BYTES,
            })?;
        if serialized.len() > MAX_OUTPUT_BYTES {
            return Err(ChildRuntimeError::OutputTooLarge {
                actual: serialized.len(),
                max: MAX_OUTPUT_BYTES,
            });
        }
        Ok(())
    }
}

pub fn accept_report(
    request: &ChildTaskRequest,
    report: &ChildReport,
) -> Result<ChildReport, ChildRuntimeError> {
    request.validate()?;
    report.validate()?;
    if request.child_task_id != report.child_task_id {
        return Err(ChildRuntimeError::TaskMismatch);
    }
    Ok(report.clone())
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), ChildRuntimeError> {
    if value.trim().is_empty() {
        return Err(ChildRuntimeError::EmptyField(field));
    }
    if value.chars().count() > max {
        return Err(ChildRuntimeError::FieldTooLong { field, max });
    }
    Ok(())
}

fn is_read_only_capability(capability: &str) -> bool {
    matches!(
        capability,
        "workspace.read" | "workspace.search" | "git.diff" | "git.status"
    )
}

fn reject_secret_like(value: &str) -> Result<(), ChildRuntimeError> {
    let lower = value.to_ascii_lowercase();
    if [
        "api_key",
        "apikey",
        "authorization",
        "bearer ",
        "password",
        "secret",
        "token",
        "sk-",
        "ghp_",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return Err(ChildRuntimeError::SecretLikeContent);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ChildTaskRequest {
        ChildTaskRequest {
            child_task_id: "child-1".into(),
            parent_task_id: "task-1".into(),
            role: "researcher".into(),
            kind: ChildTaskKind::CodeSearch,
            reduced_context: vec!["inspect src".into()],
            max_output_bytes: 4096,
            requested_capabilities: vec!["workspace.read".into(), "git.diff".into()],
            parent_is_child: false,
        }
    }

    fn report() -> ChildReport {
        ChildReport {
            child_task_id: "child-1".into(),
            status: ChildReportStatus::Complete,
            summary: "found one relevant module".into(),
            findings: vec!["module is bounded".into()],
            sources: vec!["src/lib.rs:10".into()],
            confidence_percent: 90,
        }
    }

    #[test]
    fn accepts_read_only_request_and_report() {
        let accepted = accept_report(&request(), &report()).unwrap();
        assert_eq!(accepted.child_task_id, "child-1");
    }

    #[test]
    fn rejects_nested_or_mutating_capabilities() {
        let mut nested = request();
        nested.parent_is_child = true;
        assert_eq!(
            nested.validate(),
            Err(ChildRuntimeError::NestedChildForbidden)
        );

        let mut mutating = request();
        mutating.requested_capabilities = vec!["workspace.write".into()];
        assert!(matches!(
            mutating.validate(),
            Err(ChildRuntimeError::ForbiddenCapability(_))
        ));
    }

    #[test]
    fn bounds_context_and_output() {
        let mut oversized = request();
        oversized.reduced_context = vec!["x".repeat(MAX_CONTEXT_ITEM_CHARS + 1)];
        assert!(oversized.validate().is_err());

        oversized = request();
        oversized.max_output_bytes = MAX_OUTPUT_BYTES + 1;
        assert!(matches!(
            oversized.validate(),
            Err(ChildRuntimeError::OutputTooLarge { .. })
        ));
    }

    #[test]
    fn rejects_secret_like_report_content_and_duplicate_sources() {
        let mut unsafe_report = report();
        unsafe_report.summary = "token must not leak".into();
        assert_eq!(
            unsafe_report.validate(),
            Err(ChildRuntimeError::SecretLikeContent)
        );

        unsafe_report = report();
        unsafe_report.sources.push("src/lib.rs:10".into());
        assert_eq!(
            unsafe_report.validate(),
            Err(ChildRuntimeError::DuplicateSource)
        );
    }

    #[test]
    fn rejects_mismatched_handoff_and_serializes_deterministically() {
        let mut mismatched = report();
        mismatched.child_task_id = "other".into();
        assert_eq!(
            accept_report(&request(), &mismatched),
            Err(ChildRuntimeError::TaskMismatch)
        );
        let first = request().deterministic_json().unwrap();
        assert_eq!(first, request().deterministic_json().unwrap());
    }
}
