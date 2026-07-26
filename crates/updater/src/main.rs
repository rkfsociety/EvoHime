//! `updater.exe` (раздел VIII плана Installer/Launcher/Update) —
//! вспомогательный процесс для самообновления Launcher'а.
//!
//! Вызывается самим Launcher'ом перед тем, как тот завершается:
//! `updater.exe --old-exe <path> --new-exe <path> --pid <launcher_pid>
//!  --started-at <unix_millis>`
//!
//! Делает ровно четыре вещи: ждёт завершения исходного Launcher-процесса
//! (сверяя PID, путь exe и время старта — раздел VIII плана), убивает
//! принудительно по таймауту, атомарно заменяет файл и перезапускает,
//! затем планирует своё собственное удаление. Никакой сетевой логики,
//! никакого парсинга релизов — это уже сделал Launcher до вызова
//! `updater.exe`, поэтому здесь нечему ломаться при следующем изменении
//! формата релизов.

#[cfg(windows)]
use evohime_updater::{
    force_kill, replace_and_relaunch, schedule_self_delete, wait_for_exit, TargetProcess,
};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[cfg(windows)]
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(windows)]
const POLL_INTERVAL: Duration = Duration::from_millis(200);

#[cfg(windows)]
fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("evohime-updater: {err}");
            std::process::exit(2);
        }
    };

    let target = TargetProcess {
        pid: args.pid,
        expected_exe_path: args.old_exe.clone(),
        expected_start_time: args.started_at,
    };

    if !wait_for_exit(&target, WAIT_TIMEOUT, POLL_INTERVAL) {
        eprintln!("evohime-updater: Launcher did not exit within {WAIT_TIMEOUT:?}, forcing");
        force_kill(&target);
    }

    match replace_and_relaunch(&args.old_exe, &args.new_exe) {
        Ok(_child) => {
            let self_exe = std::env::current_exe().ok();
            if let Some(self_exe) = self_exe {
                let _ = schedule_self_delete(&self_exe);
            }
        }
        Err(err) => {
            eprintln!("evohime-updater: failed to replace and relaunch: {err}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(windows))]
fn main() {}

struct Args {
    old_exe: PathBuf,
    new_exe: PathBuf,
    pid: u32,
    started_at: SystemTime,
}

fn parse_args(mut iter: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut old_exe = None;
    let mut new_exe = None;
    let mut pid = None;
    let mut started_at = None;

    while let Some(flag) = iter.next() {
        let value = iter
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--old-exe" => old_exe = Some(PathBuf::from(value)),
            "--new-exe" => new_exe = Some(PathBuf::from(value)),
            "--pid" => {
                pid = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| format!("invalid --pid value: {value}"))?,
                )
            }
            "--started-at" => {
                let millis = value
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --started-at value: {value}"))?;
                started_at = Some(SystemTime::UNIX_EPOCH + Duration::from_millis(millis));
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(Args {
        old_exe: old_exe.ok_or("missing --old-exe")?,
        new_exe: new_exe.ok_or("missing --new-exe")?,
        pid: pid.ok_or("missing --pid")?,
        started_at: started_at.ok_or("missing --started-at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pairs: &[(&str, &str)]) -> Vec<String> {
        pairs
            .iter()
            .flat_map(|(k, v)| [k.to_string(), v.to_string()])
            .collect()
    }

    #[test]
    fn parses_all_required_arguments() {
        let parsed = parse_args(
            args(&[
                ("--old-exe", r"C:\EvoHime\launcher.exe"),
                ("--new-exe", r"C:\EvoHime\launcher_new.exe"),
                ("--pid", "4242"),
                ("--started-at", "1700000000000"),
            ])
            .into_iter(),
        )
        .expect("should parse successfully");

        assert_eq!(parsed.old_exe, PathBuf::from(r"C:\EvoHime\launcher.exe"));
        assert_eq!(
            parsed.new_exe,
            PathBuf::from(r"C:\EvoHime\launcher_new.exe")
        );
        assert_eq!(parsed.pid, 4242);
        assert_eq!(
            parsed.started_at,
            SystemTime::UNIX_EPOCH + Duration::from_millis(1700000000000)
        );
    }

    #[test]
    fn errors_on_missing_required_argument() {
        let result =
            parse_args(args(&[("--old-exe", "a.exe"), ("--new-exe", "b.exe")]).into_iter());
        assert!(result.is_err());
    }

    #[test]
    fn errors_on_invalid_pid() {
        let result = parse_args(
            args(&[
                ("--old-exe", "a.exe"),
                ("--new-exe", "b.exe"),
                ("--pid", "not-a-number"),
                ("--started-at", "0"),
            ])
            .into_iter(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn errors_on_unknown_argument() {
        let result = parse_args(args(&[("--bogus", "value")]).into_iter());
        assert!(result.is_err());
    }
}
