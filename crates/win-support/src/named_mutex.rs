//! Именованный Mutex с префиксом `Local\` (не `Global\`) для блокировки
//! одновременного запуска Installer/Launcher (раздел IX плана).
//!
//! `Local\` привязан к текущей сессии пользователя: не требует привилегии
//! `SeCreateGlobalPrivilege` (в отличие от `Global\`, который к тому же
//! помешал бы двум разным пользователям Windows на одной машине запускать
//! EvoHime параллельно), и ОС автоматически освобождает мьютекс при
//! аварийном завершении процесса — в отличие от файла-лока с PID, который
//! может пережить BSOD/`kill -9` и ошибочно указывать "уже запущено".

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MutexError {
    #[error("another instance is already running (mutex `{0}` already held)")]
    AlreadyRunning(String),
    #[error("failed to create named mutex `{name}`: OS error {code}")]
    CreateFailed { name: String, code: u32 },
}

/// RAII-хендл на именованный `Local\` mutex. Пока значение живёт — блокировка
/// удерживается; при `Drop` — освобождается.
#[cfg(windows)]
#[derive(Debug)]
pub struct SingleInstanceLock {
    handle: windows::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl SingleInstanceLock {
    /// Пытается захватить эксклюзивный, привязанный к сессии именованный
    /// mutex. `name` передаётся без префикса `Local\` — он добавляется
    /// здесь автоматически.
    pub fn acquire(name: &str) -> Result<Self, MutexError> {
        use windows::core::HSTRING;
        use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows::Win32::System::Threading::CreateMutexW;

        let full_name = format!("Local\\{name}");
        let wide = HSTRING::from(full_name.as_str());

        // SAFETY: стандартный вызов CreateMutexW с валидной wide-строкой и
        // атрибутами безопасности по умолчанию (None).
        let handle = unsafe { CreateMutexW(None, false, &wide) };

        let handle = match handle {
            Ok(h) => h,
            Err(err) => {
                return Err(MutexError::CreateFailed {
                    name: full_name,
                    code: err.code().0 as u32,
                });
            }
        };

        // GetLastError() нужно проверить сразу после CreateMutexW, даже при
        // успехе — так по конвенции WinAPI отличают "создан новый" от
        // "открыт уже существующий" (ERROR_ALREADY_EXISTS).
        let last_error = unsafe { GetLastError() };
        if last_error == ERROR_ALREADY_EXISTS {
            // SAFETY: закрываем хендл на уже существующий чужой мьютекс —
            // он нам не принадлежит, и мы не должны держать его открытым.
            unsafe {
                use windows::Win32::Foundation::CloseHandle;
                let _ = CloseHandle(handle);
            }
            return Err(MutexError::AlreadyRunning(full_name));
        }

        Ok(Self { handle })
    }
}

#[cfg(windows)]
impl Drop for SingleInstanceLock {
    fn drop(&mut self) {
        use windows::Win32::Foundation::CloseHandle;
        // SAFETY: `handle` создан в `acquire` через CreateMutexW и
        // закрывается ровно один раз — здесь.
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

// SAFETY: HANDLE — это просто хендл на ядерный объект-мьютекс; сигнализация
// и закрытие такого объекта безопасны из любого потока.
#[cfg(windows)]
unsafe impl Send for SingleInstanceLock {}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_of_same_name_fails() {
        let name = format!("EvoHimeTestMutex-{}", std::process::id());
        let _first = SingleInstanceLock::acquire(&name).expect("first acquire should succeed");

        let second = SingleInstanceLock::acquire(&name);
        assert!(
            matches!(second, Err(MutexError::AlreadyRunning(_))),
            "expected AlreadyRunning, got {second:?}"
        );
    }

    #[test]
    fn different_names_do_not_conflict() {
        let base = std::process::id();
        let _a = SingleInstanceLock::acquire(&format!("EvoHimeTestMutexA-{base}"))
            .expect("acquire A should succeed");
        let _b = SingleInstanceLock::acquire(&format!("EvoHimeTestMutexB-{base}"))
            .expect("acquire B should succeed");
    }

    #[test]
    fn releasing_lock_allows_reacquire() {
        let name = format!("EvoHimeTestMutexReacquire-{}", std::process::id());
        {
            let _guard = SingleInstanceLock::acquire(&name).expect("first acquire should succeed");
        } // guard dropped here, mutex released

        let reacquired = SingleInstanceLock::acquire(&name);
        assert!(
            reacquired.is_ok(),
            "expected reacquire to succeed after drop, got {reacquired:?}"
        );
    }
}
