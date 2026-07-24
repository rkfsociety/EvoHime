//! Local golden-task report: prints per-task status, exits non-zero on failure.

use evohime_evals::{golden_dir, load_golden_dir, run_golden_task};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let tasks = match load_golden_dir(&golden_dir()) {
        Ok(tasks) => tasks,
        Err(error) => {
            eprintln!("eval harness: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let mut failed = 0usize;
    for task in &tasks {
        let report = run_golden_task(task).await;
        if report.passed() {
            println!("PASS  {}", report.name);
        } else {
            failed += 1;
            println!("FAIL  {}", report.name);
            for failure in &report.failures {
                println!("      - {failure}");
            }
        }
    }
    println!("{} passed, {} failed, {} total", tasks.len() - failed, failed, tasks.len());
    if failed > 0 {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}
