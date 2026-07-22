use std::time::{Duration, Instant};

pub fn redact_for_log(input: &str) -> String {
    evohime_memory::redact_secrets(input).text
}

pub fn should_emit_health_failure(
    previous: Option<(Instant, String)>,
    message: &str,
    now: Instant,
    interval: Duration,
) -> bool {
    previous.is_none_or(|(at, previous_message)| {
        previous_message != message || now.duration_since(at) >= interval
    })
}

pub fn health_sample_interval() -> Duration {
    let seconds = std::env::var("EVOHIME_LOG_SAMPLE_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(30)
        .clamp(1, 86_400);
    Duration::from_secs(seconds)
}

#[test]
fn redacts_dynamic_log_values() {
    let safe = redact_for_log("Authorization: Bearer abcdefghijklmnop password=hunter2");
    assert!(!safe.contains("abcdefghijklmnop"));
    assert!(!safe.contains("hunter2"));
    assert!(safe.contains("[REDACTED]"));
}

#[test]
fn preserves_benign_log_values() {
    assert_eq!(redact_for_log("worker restarted"), "worker restarted");
}

#[test]
fn samples_only_identical_messages_inside_interval() {
    let now = Instant::now();
    assert!(should_emit_health_failure(None, "connection refused", now, Duration::from_secs(30)));
    assert!(!should_emit_health_failure(
        Some((now, "connection refused".into())),
        "connection refused",
        now + Duration::from_secs(1),
        Duration::from_secs(30),
    ));
    assert!(should_emit_health_failure(
        Some((now, "connection refused".into())),
        "timeout",
        now + Duration::from_secs(1),
        Duration::from_secs(30),
    ));
    assert!(should_emit_health_failure(
        Some((now, "connection refused".into())),
        "connection refused",
        now + Duration::from_secs(31),
        Duration::from_secs(30),
    ));
}
