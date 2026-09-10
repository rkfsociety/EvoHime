mod pulse;
// Keep the contract-driven runtime loop available to non-Windows unit tests;
// the production supervisor entry point is Windows-only, so these items are
// otherwise intentionally unused on Linux/macOS workspace checks.
#[cfg_attr(not(windows), allow(dead_code))]
mod runtime_loop;
mod schedule_contract;
mod scheduler_state;

mod local_provider;
#[cfg(windows)]
mod windows_supervisor;

#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    windows_supervisor::run().await
}

#[cfg(not(windows))]
fn main() {
    eprintln!("evohime-supervisor is supported on Windows only");
}
