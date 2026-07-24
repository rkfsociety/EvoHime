//! Regression runner: every golden task must pass on every CI push.

use evohime_evals::{golden_dir, load_golden_dir, run_golden_task};

#[tokio::test]
async fn all_golden_tasks_pass() {
    let tasks = load_golden_dir(&golden_dir()).expect("golden tasks load");
    assert!(tasks.len() >= 3, "golden suite unexpectedly small");

    let mut failed = Vec::new();
    for task in &tasks {
        let report = run_golden_task(task).await;
        if !report.passed() {
            failed.push(format!("{}:\n  - {}", report.name, report.failures.join("\n  - ")));
        }
    }
    assert!(
        failed.is_empty(),
        "golden tasks failed:\n{}",
        failed.join("\n")
    );
}
