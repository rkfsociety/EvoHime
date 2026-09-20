use crate::event_matches_run;

#[test]
fn filters_events_to_the_requested_run() {
    assert!(event_matches_run("run-1", "run-1"));
    assert!(!event_matches_run("run-2", "run-1"));
    assert!(!event_matches_run("", "run-1"));
    assert!(!event_matches_run("run-1", ""));
}
