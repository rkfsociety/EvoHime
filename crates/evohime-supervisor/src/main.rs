mod runtime_loop;
mod schedule_contract;
mod scheduler_state;

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
