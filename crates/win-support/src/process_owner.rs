//! Резолв полного пути exe-файла процесса по PID — нужен, чтобы отличить
//! "наш" процесс, слушающий порт, от "чужого" (раздел IX плана): без этого
//! Launcher рискует либо убить чужой процесс, либо ошибочно решить, что
//! порт занят "врагом", когда на самом деле это его же процесс от
//! предыдущего запуска.

use std::path::PathBuf;

#[cfg(windows)]
pub fn resolve_process_exe_path(pid: u32) -> Option<PathBuf> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: PROCESS_QUERY_LIMITED_INFORMATION запрашивает только
    // read-only метаданные (не требует повышенных прав даже для чужих
    // процессов, кроме защищённых системных); pid приходит из
    // GetExtendedTcpTable — доверенного источника ОС.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;

    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;

    // SAFETY: buffer — валидный wide-char буфер нужного размера; size
    // передаётся по указателю и обновляется вызовом.
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    };

    // SAFETY: handle открыт этим же вызовом выше и закрывается ровно один раз.
    unsafe {
        let _ = CloseHandle(handle);
    }

    result.ok()?;

    let path_str = String::from_utf16_lossy(&buffer[..size as usize]);
    Some(PathBuf::from(path_str))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn resolves_own_process_exe_path() {
        let own_pid = std::process::id();
        let resolved = resolve_process_exe_path(own_pid).expect("should resolve own exe path");

        let expected = std::env::current_exe().expect("current_exe should succeed");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn returns_none_for_implausible_pid() {
        // PID 0 is reserved (System Idle Process) and OpenProcess with
        // QUERY_LIMITED_INFORMATION on it should fail rather than panic.
        assert!(resolve_process_exe_path(0).is_none());
    }
}
