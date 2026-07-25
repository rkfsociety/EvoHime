//! Логика самообновления Launcher'а (раздел VIII плана).
//!
//! `updater.exe` — единственный компонент, который никогда не должен
//! требовать обновления сам (иначе "кто обновит обновляющего"), поэтому
//! максимально прост: подождать завершения исходного Launcher'а, заменить
//! файл, запустить новую версию, удалить себя. Никакой сетевой логики,
//! никакого парсинга — вся эта работа уже сделана самим Launcher'ом до
//! того, как он передал управление сюда.

use evohime_win_support::{
    is_process_alive, process_start_time, resolve_process_exe_path, terminate_process,
};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, thiserror::Error)]
pub enum UpdaterError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[cfg(windows)]
    #[error(transparent)]
    Replace(#[from] evohime_win_support::ReplaceFileError),
}

/// Идентифицирует исходный Launcher-процесс, за завершением которого
/// нужно проследить. Сверяются PID, путь exe и время старта одновременно
/// (раздел VIII плана) — иначе, если ОС успеет переиспользовать PID для
/// совершенно другого процесса за время ожидания, updater решит, что
/// "исходный процесс всё ещё жив", хотя на самом деле это уже кто-то
/// другой.
pub struct TargetProcess {
    pub pid: u32,
    pub expected_exe_path: PathBuf,
    pub expected_start_time: SystemTime,
}

impl TargetProcess {
    /// `true`, если PID прямо сейчас принадлежит именно тому процессу, за
    /// которым мы следим.
    pub fn still_is_original_process(&self) -> bool {
        if !is_process_alive(self.pid) {
            return false;
        }
        let exe_matches = resolve_process_exe_path(self.pid)
            .map(|path| path == self.expected_exe_path)
            .unwrap_or(false);
        let start_matches = process_start_time(self.pid)
            .map(|time| times_close_enough(time, self.expected_start_time))
            .unwrap_or(false);
        exe_matches && start_matches
    }
}

/// `GetProcessTimes`/сохранённое значение могут отличаться на доли
/// секунды из-за округления при передаче через аргументы командной строки
/// — точное равенство было бы излишне хрупким.
fn times_close_enough(a: SystemTime, b: SystemTime) -> bool {
    let diff = a
        .duration_since(b)
        .or_else(|_| b.duration_since(a))
        .unwrap_or(Duration::MAX);
    diff <= Duration::from_secs(2)
}

/// Ждёт, пока исходный процесс либо завершится сам, либо PID перестанет
/// ему соответствовать (что тоже означает "исходного процесса больше
/// нет"). Возвращает `true`, если дождались за `timeout`; `false` — если
/// нужен принудительный force kill (раздел VIII плана: таймаут 30 сек).
pub fn wait_for_exit(target: &TargetProcess, timeout: Duration, poll_interval: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !target.still_is_original_process() {
            return true;
        }
        std::thread::sleep(poll_interval);
    }
    false
}

/// Принудительно завершает исходный процесс, если он всё ещё жив —
/// fallback после истечения таймаута `wait_for_exit`.
pub fn force_kill(target: &TargetProcess) -> bool {
    if !target.still_is_original_process() {
        return true;
    }
    terminate_process(target.pid)
}

/// Атомарно заменяет `old_exe` содержимым `new_exe` (раздел IX плана —
/// `ReplaceFileW`, устойчивее к блокировке антивирусом, чем `fs::rename`),
/// затем запускает результат.
pub fn replace_and_relaunch(
    old_exe: &Path,
    new_exe: &Path,
) -> Result<std::process::Child, UpdaterError> {
    #[cfg(windows)]
    evohime_win_support::atomic_replace_or_create(old_exe, new_exe)?;
    #[cfg(not(windows))]
    std::fs::rename(new_exe, old_exe)?;

    Ok(std::process::Command::new(old_exe).spawn()?)
}

/// Планирует удаление `exe_path` через отдельный отсоединённый процесс —
/// сам `updater.exe` не может удалить себя, пока выполняется (файл
/// заблокирован ОС до завершения процесса).
pub fn schedule_self_delete(exe_path: &Path) -> std::io::Result<std::process::Child> {
    let path_str = exe_path.display().to_string();
    std::process::Command::new("cmd")
        .args([
            "/C",
            &format!("ping 127.0.0.1 -n 2 > nul & del \"{path_str}\""),
        ])
        .spawn()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::process::Command;

    /// Использует `ping` (не `timeout`) как долгоживущий процесс-заглушку:
    /// в некоторых окружениях `PATH` перед системным
    /// `C:\Windows\System32\timeout.exe` оказывается версия `timeout` из
    /// Git for Windows (coreutils), у которой другой синтаксис аргументов
    /// — она немедленно завершается с ошибкой вместо ожидания, что
    /// незаметно ломает тесты, полагающиеся на "процесс ещё жив". `ping`
    /// не имеет такого неоднозначного тёзки.
    fn spawn_dummy(seconds: u32) -> std::process::Child {
        Command::new("cmd")
            .args(["/c", &format!("ping 127.0.0.1 -n {} > nul", seconds + 1)])
            .spawn()
            .expect("spawn should succeed")
    }

    fn target_for(child: &std::process::Child) -> TargetProcess {
        let pid = child.id();
        TargetProcess {
            pid,
            expected_exe_path: resolve_process_exe_path(pid).expect("should resolve exe path"),
            expected_start_time: process_start_time(pid).expect("should resolve start time"),
        }
    }

    #[test]
    fn times_close_enough_allows_small_tolerance() {
        let base = SystemTime::now();
        assert!(times_close_enough(base, base + Duration::from_millis(500)));
        assert!(!times_close_enough(base, base + Duration::from_secs(10)));
    }

    #[test]
    fn still_is_original_process_true_while_alive() {
        let mut child = spawn_dummy(10);
        let target = target_for(&child);
        assert!(target.still_is_original_process());

        terminate_process(target.pid);
        std::thread::sleep(Duration::from_millis(300));
        assert!(!target.still_is_original_process());
        let _ = child.wait();
    }

    #[test]
    fn wait_for_exit_returns_false_on_timeout_for_long_running_process() {
        let mut child = spawn_dummy(30);
        let target = target_for(&child);

        let finished = wait_for_exit(
            &target,
            Duration::from_millis(500),
            Duration::from_millis(100),
        );
        assert!(
            !finished,
            "long-running process should not finish within 500ms"
        );

        terminate_process(target.pid);
        let _ = child.wait();
    }

    #[test]
    fn force_kill_stops_still_running_process() {
        let mut child = spawn_dummy(30);
        let target = target_for(&child);

        assert!(target.still_is_original_process());
        assert!(force_kill(&target));

        std::thread::sleep(Duration::from_millis(300));
        assert!(!target.still_is_original_process());
        let _ = child.wait();
    }

    #[test]
    fn force_kill_is_a_no_op_when_already_gone() {
        let mut child = spawn_dummy(1);
        let pid = child.id();
        let target = TargetProcess {
            pid,
            expected_exe_path: resolve_process_exe_path(pid).expect("should resolve exe path"),
            expected_start_time: process_start_time(pid).expect("should resolve start time"),
        };
        let _ = child.wait(); // let it exit naturally

        assert!(
            force_kill(&target),
            "force_kill on an already-exited process is not an error"
        );
    }

    /// Копирует реальный, безопасный, немедленно завершающийся системный
    /// бинарник (`hostname.exe`) в два временных пути и прогоняет через
    /// него весь цикл replace+relaunch — проверяет не только механику
    /// замены файла (уже покрыта тестами win-support), но и то, что
    /// получившийся файл действительно можно запустить как процесс.
    #[test]
    fn replace_and_relaunch_produces_a_runnable_process() {
        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
        let hostname_exe = PathBuf::from(system_root)
            .join("System32")
            .join("hostname.exe");
        if !hostname_exe.exists() {
            eprintln!("skipping: hostname.exe not found on this machine");
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("old_app.exe");
        let new_path = dir.path().join("new_app.exe");
        std::fs::copy(&hostname_exe, &old_path).unwrap();
        std::fs::copy(&hostname_exe, &new_path).unwrap();

        let mut child =
            replace_and_relaunch(&old_path, &new_path).expect("relaunch should succeed");
        let status = child.wait().expect("child process should exit");
        assert!(
            status.success(),
            "relaunched hostname.exe should exit successfully"
        );
        assert!(
            !new_path.exists(),
            "source file should have been consumed by the replace"
        );
    }
}
