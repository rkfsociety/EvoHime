use evohime_update_agent::{select_outdated, InstalledManifest, ModuleRecord};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let args = env::args().collect::<Vec<_>>();
    let Some(path) = args
        .windows(2)
        .find(|pair| pair[0] == "--manifest")
        .map(|pair| &pair[1])
    else {
        eprintln!("usage: evohime-updater --manifest <installed-manifest.json> --available <manifest.json>");
        return ExitCode::from(2);
    };
    let Some(available_path) = args
        .windows(2)
        .find(|pair| pair[0] == "--available")
        .map(|pair| &pair[1])
    else {
        return ExitCode::from(2);
    };
    let installed = match read::<InstalledManifest>(path) {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    let available = match read::<Vec<ModuleRecord>>(available_path) {
        Ok(value) => value,
        Err(error) => return fail(error),
    };
    match select_outdated(&installed, &available) {
        Ok(plan) => {
            println!("{}", serde_json::to_string(&plan).expect("plan serializes"));
            ExitCode::SUCCESS
        }
        Err(error) => fail(error),
    }
}

fn read<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    fs::read_to_string(path)
        .map_err(|error| error.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))
}
fn fail(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("updater: {error}");
    ExitCode::from(1)
}
