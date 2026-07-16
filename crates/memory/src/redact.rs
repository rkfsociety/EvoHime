use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactionOutcome {
    pub text: String,
    pub redacted: bool,
}

fn patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"(?i)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
            r"(?i)\b(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9_]{20,}\b",
            r"(?i)\b(sk|lr)[-_][A-Za-z0-9_\-]{16,}\b",
            r"(?i)\b(xox[baprs]-)[A-Za-z0-9\-]{10,}\b",
            r"(?i)\b(AKIA)[0-9A-Z]{16}\b",
            r"(?i)\b(Bearer\s+)[A-Za-z0-9\-._~+/]+=*\b",
            r"(?i)\b(api[_-]?key|token|secret|password|passwd|cookie)\s*[:=]\s*\S{6,}",
        ]
        .into_iter()
        .map(|pattern| Regex::new(pattern).expect("valid redaction regex"))
        .collect()
    })
}

/// Replace secret-looking spans with `[REDACTED]`.
pub fn redact_secrets(input: &str) -> RedactionOutcome {
    let mut text = input.to_string();
    let mut redacted = false;
    for pattern in patterns() {
        if pattern.is_match(&text) {
            redacted = true;
            text = pattern.replace_all(&text, "[REDACTED]").into_owned();
        }
    }
    RedactionOutcome { text, redacted }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn leaves_benign_text_alone() {
        let outcome = redact_secrets("prefer map over foreach");
        assert!(!outcome.redacted);
        assert_eq!(outcome.text, "prefer map over foreach");
    }
}
