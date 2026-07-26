//! Время старта процесса по PID — нужно `updater.exe` (раздел VIII плана),
//! чтобы ждать завершения именно исходного Launcher'а, а не случайно
//! переиспользованного ОС того же PID для другого процесса: сверяются и
//! PID, и путь exe (`process_owner::resolve_process_exe_path`), и время
//! создания процесса вместе — совпадение всех трёх делает подмену
//! практически невозможной.

#[cfg(windows)]
use std::time::SystemTime;

/// Возвращает момент создания процесса `pid`, если он существует и
/// доступен для чтения метаданных.
#[cfg(windows)]
pub fn process_start_time(pid: u32) -> Option<SystemTime> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: PROCESS_QUERY_LIMITED_INFORMATION запрашивает только
    // read-only метаданные; pid — обычный process id.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;

    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();

    // SAFETY: все четыре указателя — валидные локальные FILETIME.
    let result =
        unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) };

    // SAFETY: handle открыт этим же вызовом выше и закрывается один раз.
    unsafe {
        let _ = CloseHandle(handle);
    }

    result.ok()?;
    Some(filetime_to_system_time(creation))
}

#[cfg(windows)]
fn filetime_to_system_time(ft: windows::Win32::Foundation::FILETIME) -> SystemTime {
    // FILETIME — количество 100-наносекундных интервалов с 1601-01-01.
    // UNIX_EPOCH (1970-01-01) отстоит от этой даты на 11644473600 секунд.
    const UNIX_EPOCH_DIFF_100NS: u64 = 11_644_473_600 * 10_000_000;
    let ticks = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    let unix_100ns = ticks.saturating_sub(UNIX_EPOCH_DIFF_100NS);
    SystemTime::UNIX_EPOCH + std::time::Duration::from_nanos(unix_100ns * 100)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn returns_plausible_start_time_for_own_process() {
        let own_pid = std::process::id();
        let start = process_start_time(own_pid).expect("should resolve own start time");

        let now = SystemTime::now();
        assert!(start <= now, "process cannot have started in the future");

        // Sanity bound: the test process didn't start more than a day ago
        // (catches gross unit conversion errors without being flaky about
        // exact timing).
        let one_day_ago = now - std::time::Duration::from_secs(24 * 60 * 60);
        assert!(
            start >= one_day_ago,
            "start time implausibly far in the past"
        );
    }

    #[test]
    fn returns_none_for_implausible_pid() {
        assert!(process_start_time(0).is_none());
    }
}
