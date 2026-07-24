//! Windows-специфичные примитивы, переиспользуемые Installer'ом, Launcher'ом
//! и `updater.exe` (разделы VIII, IX плана Installer/Launcher/Update):
//! именованный Mutex с префиксом `Local\`, атомарная замена файлов через
//! `ReplaceFileW`, и резолв "свой vs чужой процесс" на занятом порту.

pub mod disk_space;
pub mod named_mutex;
pub mod port_owner;
pub mod process_owner;
pub mod replace_file;

pub use disk_space::{free_bytes_available, DiskSpaceError};
pub use named_mutex::{MutexError, SingleInstanceLock};
pub use port_owner::find_pid_listening_on_port;
pub use process_owner::resolve_process_exe_path;
pub use replace_file::{atomic_replace_or_create, ReplaceFileError};
