//! Windows-специфичные примитивы, переиспользуемые Installer'ом, Launcher'ом
//! и `updater.exe` (разделы VIII, IX плана Installer/Launcher/Update):
//! именованный Mutex с префиксом `Local\`, атомарная замена файлов через
//! `ReplaceFileW`, и резолв "свой vs чужой процесс" на занятом порту.

pub mod disk_space;
pub mod named_mutex;
pub mod port_owner;
pub mod process_liveness;
pub mod process_owner;
pub mod process_time;
pub mod replace_file;

#[cfg(windows)]
pub use disk_space::free_bytes_available;
pub use disk_space::DiskSpaceError;
pub use named_mutex::MutexError;
#[cfg(windows)]
pub use named_mutex::SingleInstanceLock;
#[cfg(windows)]
pub use port_owner::find_pid_listening_on_port;
#[cfg(windows)]
pub use process_liveness::{is_process_alive, terminate_process};
#[cfg(windows)]
pub use process_owner::resolve_process_exe_path;
#[cfg(windows)]
pub use process_time::process_start_time;
#[cfg(windows)]
pub use replace_file::atomic_replace_or_create;
pub use replace_file::ReplaceFileError;
