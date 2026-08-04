#[cfg(windows)]
#[tokio::main]
async fn main() {
    let data_dir = std::env::var_os("EVOHIME_DATA_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(|path| std::path::PathBuf::from(path).join("EvoHime"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from(".evohime"));
    let pipe_name =
        std::env::var("EVOHIME_CORE_PIPE").unwrap_or_else(|_| r"\\.\pipe\evohime-core-v1".into());
    if let Err(error) = evohime_core::run_windows_pipe(&pipe_name, data_dir.join("events.db")).await
    {
        eprintln!("evohime-core failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    println!("evohime-core {}", evohime_core::CoreVersion::current());
}
