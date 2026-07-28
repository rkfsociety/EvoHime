//! Поиск и завершение процессов, исполняемые файлы которых принадлежат
//! конкретному каталогу установки.

use crate::{is_process_alive, resolve_process_exe_path, terminate_process};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedProcess {
    pub pid: u32,
    pub exe_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessCleanupError {
    #[error("не удалось завершить процесс {pid} ({path})")]
    Terminate { pid: u32, path: PathBuf },
    #[error("процессы EvoHime не завершились вовремя: {processes:?}")]
    Timeout { processes: Vec<OwnedProcess> },
}

pub fn processes_in_directory(
    root: &Path,
    excluded_pid: u32,
) -> std::io::Result<Vec<OwnedProcess>> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let canonical_root = root.canonicalize()?;
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|error| std::io::Error::other(error.to_string()))?;

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let first = unsafe { Process32FirstW(snapshot, &mut entry) };
    if let Err(error) = first {
        unsafe {
            let _ = CloseHandle(snapshot);
        }
        return Err(std::io::Error::other(error.to_string()));
    }

    let mut found = Vec::new();
    loop {
        let pid = entry.th32ProcessID;
        if pid != 0 && pid != excluded_pid {
            if let Some(exe_path) =
                resolve_process_exe_path(pid).and_then(|path| path.canonicalize().ok())
            {
                if exe_path != canonical_root && exe_path.starts_with(&canonical_root) {
                    found.push(OwnedProcess { pid, exe_path });
                }
            }
        }

        if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
            break;
        }
    }

    unsafe {
        let _ = CloseHandle(snapshot);
    }
    found.sort_by_key(|process| process.pid);
    found.dedup_by_key(|process| process.pid);
    Ok(found)
}

pub fn terminate_and_wait(
    processes: &[OwnedProcess],
    timeout: Duration,
) -> Result<(), ProcessCleanupError> {
    for process in processes {
        if !is_process_alive(process.pid) {
            continue;
        }

        let still_owned = resolve_process_exe_path(process.pid)
            .and_then(|path| path.canonicalize().ok())
            .is_some_and(|path| path == process.exe_path);
        if !still_owned || !terminate_process(process.pid) {
            return Err(ProcessCleanupError::Terminate {
                pid: process.pid,
                path: process.exe_path.clone(),
            });
        }
    }

    let deadline = Instant::now() + timeout;
    loop {
        let remaining: Vec<_> = processes
            .iter()
            .filter(|process| is_process_alive(process.pid))
            .cloned()
            .collect();
        if remaining.is_empty() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(ProcessCleanupError::Timeout {
                processes: remaining,
            });
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
