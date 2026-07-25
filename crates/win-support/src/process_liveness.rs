//! Проверка "жив ли процесс" и принудительное завершение по PID —
//! нужны `updater.exe` (раздел VIII плана), который проверяет произвольный
//! чужой PID (Launcher его не порождал как дочерний, поэтому
//! `tokio::process::Child::try_wait` недоступен — только сырой WinAPI).

/// `true`, если процесс с данным PID всё ещё выполняется.
#[cfg(windows)]
pub fn is_process_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: PROCESS_QUERY_LIMITED_INFORMATION запрашивает только
    // read-only метаданные.
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return false;
    };

    let mut exit_code: u32 = 0;
    // SAFETY: handle валиден (проверено выше), exit_code — валидный
    // локальный указатель.
    let result = unsafe { GetExitCodeProcess(handle, &mut exit_code) };

    // SAFETY: handle открыт этим же вызовом выше и закрывается один раз.
    unsafe {
        let _ = CloseHandle(handle);
    }

    result.is_ok() && exit_code == STILL_ACTIVE.0 as u32
}

/// Принудительно завершает процесс с данным PID. Возвращает `true`, если
/// вызов `TerminateProcess` прошёл успешно (сам процесс уже мог не
/// существовать — это не считается ошибкой вызывающего кода).
#[cfg(windows)]
pub fn terminate_process(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    // SAFETY: PROCESS_TERMINATE — минимально нужные права для
    // TerminateProcess.
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }) else {
        return false;
    };

    // SAFETY: handle валиден; exit code 1 — произвольный ненулевой код,
    // означающий "завершён принудительно".
    let result = unsafe { TerminateProcess(handle, 1) };

    // SAFETY: handle открыт этим же вызовом выше и закрывается один раз.
    unsafe {
        let _ = CloseHandle(handle);
    }

    result.is_ok()
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn own_process_is_alive() {
        assert!(is_process_alive(std::process::id()));
    }

    #[test]
    fn implausible_pid_is_not_alive() {
        assert!(!is_process_alive(0));
    }

    #[test]
    fn spawned_process_can_be_terminated() {
        let mut child = Command::new("cmd")
            .args(["/c", "timeout /t 30 /nobreak >nul"])
            .spawn()
            .expect("spawn should succeed");
        let pid = child.id();

        assert!(
            is_process_alive(pid),
            "freshly spawned process should be alive"
        );
        assert!(terminate_process(pid), "terminate_process should succeed");

        // Give the OS a moment to finalize the termination before checking.
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            !is_process_alive(pid),
            "process should be dead after terminate_process"
        );

        let _ = child.wait();
    }
}
