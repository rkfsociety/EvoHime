//! Golden-task report: mock mode by default (deterministic), `--live` runs
//! against the configured real provider, `--judge` adds LLM verdicts for
//! tasks that carry a rubric. Exits non-zero on any failure.

use evohime_evals::{golden_dir, load_golden_dir, run_golden_task, run_golden_task_with_gateway};
use evohime_model_gateway::ModelGateway;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let live = args.iter().any(|arg| arg == "--live");
    let judge = args.iter().any(|arg| arg == "--judge");
    if let Some(unknown) = args
        .iter()
        .find(|arg| *arg != "--live" && *arg != "--judge")
    {
        eprintln!("unknown argument: {unknown} (supported: --live, --judge)");
        return std::process::ExitCode::FAILURE;
    }
    if judge && !live {
        eprintln!("--judge requires --live: mock runs are scripted, there is nothing to grade");
        return std::process::ExitCode::FAILURE;
    }

    let tasks = match load_golden_dir(&golden_dir()) {
        Ok(tasks) => tasks,
        Err(error) => {
            eprintln!("eval harness: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let gateway = if live {
        match ModelGateway::try_from_env() {
            Ok(gateway) => {
                println!("mode: live provider{}", if judge { " + judge" } else { "" });
                Some(gateway)
            }
            Err(error) => {
                eprintln!("live mode requires a configured provider: {error}");
                return std::process::ExitCode::FAILURE;
            }
        }
    } else {
        println!("mode: deterministic mock");
        None
    };

    let mut failed = 0usize;
    for task in &tasks {
        let report = match &gateway {
            Some(gateway) => run_golden_task_with_gateway(task, gateway, judge).await,
            None => run_golden_task(task).await,
        };
        let status = if report.passed() { "PASS" } else { "FAIL" };
        let verdict = report
            .judge
            .as_ref()
            .map(|verdict| {
                format!(
                    "  [judge: {} score={}]",
                    if verdict.pass { "pass" } else { "fail" },
                    verdict
                        .score
                        .map(|score| score.to_string())
                        .unwrap_or_else(|| "-".into())
                )
            })
            .unwrap_or_default();
        println!("{status}  {}{verdict}", report.name);
        if !report.passed() {
            failed += 1;
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
