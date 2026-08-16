use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize)]
struct TransactionState {
    install_dir: PathBuf,
    backup_dir: PathBuf,
    phase: TransactionPhase,
    #[serde(default)]
    scope: TransactionScope,
}

/// What the transaction backed up, and therefore what a rollback restores.
///
/// An installer run only replaces the known components, while a locally rebuilt
/// package replaces the whole installation directory.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
enum TransactionScope {
    #[default]
    Components,
    Tree,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum TransactionPhase {
    Installing,
    Committed,
    RollbackRequired,
    Restored,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RecoveryResult {
    pub recovered: bool,
}

pub struct UpdateTransaction {
    install_dir: PathBuf,
    backup_dir: PathBuf,
    state_path: PathBuf,
    scope: TransactionScope,
}

pub fn verify_installation(install_dir: &Path) -> io::Result<()> {
    validate_absolute(install_dir, "install directory")?;
    for component in UpdateTransaction::COMPONENTS {
        let path = install_dir.join(component);
        if !path.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "required installed component is missing: {}",
                    path.display()
                ),
            ));
        }
    }
    Ok(())
}

pub fn run_update(
    installer: &Path,
    install_dir: &Path,
    state_dir: &Path,
    relaunch: Option<&Path>,
) -> io::Result<()> {
    validate_absolute(installer, "installer path")?;
    let _ = UpdateTransaction::recover(state_dir)?;
    // Inno Setup can replace the Electron payload as well as the four native
    // components. Back up the whole tree so a failure after app.asar or a
    // resource write restores a runnable installation, not just its binaries.
    let transaction = UpdateTransaction::prepare_tree(install_dir, state_dir)?;
    if !installer.is_file() {
        return rollback_after_failure(
            transaction,
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("installer does not exist: {}", installer.display()),
            ),
        );
    }
    let status = Command::new(installer)
        .args([
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/CLOSEAPPLICATIONS",
        ])
        .arg(format!("/DIR={}", install_dir.display()))
        .status();

    match status {
        Ok(status) if status.success() => match verify_installation(install_dir) {
            Ok(()) => {
                transaction.commit()?;
                if let Some(executable) = relaunch {
                    Command::new(executable).current_dir(install_dir).spawn()?;
                }
                Ok(())
            }
            Err(error) => rollback_after_failure(transaction, error),
        },
        Ok(status) => rollback_after_failure(
            transaction,
            io::Error::other(format!("installer exited with status {status}")),
        ),
        Err(error) => rollback_after_failure(transaction, error),
    }
}

/// Options of a staged apply, produced by a local rebuild of the sources.
pub struct StagedApply<'a> {
    pub staging: &'a Path,
    pub install_dir: &'a Path,
    pub state_dir: &'a Path,
    /// Shell process that must exit before its files can be replaced.
    pub wait_pid: Option<u32>,
    /// Executable started once the new package is in place.
    pub relaunch: Option<&'a Path>,
}

/// Replaces the installation with a locally rebuilt package.
///
/// The staged tree is verified before anything is touched, the previous
/// installation is backed up in full, and any failure restores it. Only after a
/// successful commit is the shell started again.
pub fn apply_staged(options: StagedApply<'_>) -> io::Result<()> {
    validate_absolute(options.staging, "staging directory")?;
    validate_absolute(options.install_dir, "install directory")?;
    verify_installation(options.staging)?;

    if let Some(pid) = options.wait_pid {
        wait_for_process_exit(pid, WAIT_FOR_SHELL);
    }
    // Waiting for the shell process is not enough: Electron's GPU and renderer
    // children keep the executable and `app.asar` open for a moment after the
    // main process is gone. Nothing is backed up until the files are actually
    // writable, so a still-locked installation fails before it is touched.
    wait_until_writable(options.install_dir, WAIT_FOR_UNLOCK)?;

    let _ = UpdateTransaction::recover(options.state_dir)?;
    let transaction = UpdateTransaction::prepare_tree(options.install_dir, options.state_dir)?;

    let outcome = copy_tree(options.staging, options.install_dir)
        .and_then(|()| verify_installation(options.install_dir));
    match outcome {
        Ok(()) => {
            transaction.commit()?;
            if let Some(executable) = options.relaunch {
                Command::new(executable)
                    .current_dir(options.install_dir)
                    .spawn()?;
            }
            Ok(())
        }
        Err(error) => rollback_after_failure(transaction, error),
    }
}

const WAIT_FOR_SHELL: Duration = Duration::from_secs(60);
const WAIT_FOR_UNLOCK: Duration = Duration::from_secs(120);
const RETRY_INTERVAL: Duration = Duration::from_millis(250);

/// Blocks until every installed component can be opened for writing.
///
/// A locked file is a timing problem, not a broken installation, so it is
/// retried rather than reported. After the deadline the caller still sees a
/// plain error and the installation is left exactly as it was.
fn wait_until_writable(install_dir: &Path, limit: Duration) -> io::Result<()> {
    let deadline = std::time::Instant::now() + limit;
    loop {
        let error = match UpdateTransaction::COMPONENTS
            .iter()
            .map(|component| install_dir.join(component))
            .filter(|path| path.exists())
            .try_for_each(|path| {
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .map(|_| ())
                    .map_err(|error| {
                        io::Error::new(
                            error.kind(),
                            format!("{} is still in use: {error}", path.display()),
                        )
                    })
            }) {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        if std::time::Instant::now() >= deadline {
            return Err(error);
        }
        std::thread::sleep(RETRY_INTERVAL);
    }
}

fn rollback_after_failure(transaction: UpdateTransaction, failure: io::Error) -> io::Result<()> {
    match transaction.rollback() {
        Ok(()) => Err(failure),
        Err(rollback_error) => Err(io::Error::other(format!(
            "update failed: {failure}; rollback failed: {rollback_error}"
        ))),
    }
}

impl UpdateTransaction {
    pub const COMPONENTS: [&'static str; 4] = [
        "EvoHime.exe",
        "evohime-core.exe",
        "evohime-supervisor.exe",
        "evohime.manifest.json",
    ];

    pub fn prepare(install_dir: &Path, state_dir: &Path) -> io::Result<Self> {
        Self::prepare_with(install_dir, state_dir, TransactionScope::Components)
    }

    /// Backs up the whole installation directory, for updates that replace more
    /// than the known components — a locally rebuilt package does.
    pub fn prepare_tree(install_dir: &Path, state_dir: &Path) -> io::Result<Self> {
        Self::prepare_with(install_dir, state_dir, TransactionScope::Tree)
    }

    fn prepare_with(
        install_dir: &Path,
        state_dir: &Path,
        scope: TransactionScope,
    ) -> io::Result<Self> {
        validate_absolute(install_dir, "install directory")?;
        validate_absolute(state_dir, "state directory")?;
        if !install_dir.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "install directory does not exist: {}",
                    install_dir.display()
                ),
            ));
        }
        fs::create_dir_all(state_dir)?;
        let state_path = state_dir.join("transaction.json");
        if state_path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "an update transaction is already in progress",
            ));
        }

        let backup_dir = state_dir.join(format!(
            "backup-{}-{}",
            timestamp_nanos(),
            std::process::id()
        ));
        fs::create_dir_all(&backup_dir)?;
        let transaction = Self {
            install_dir: install_dir.to_path_buf(),
            backup_dir,
            state_path,
            scope,
        };
        let backup = match scope {
            TransactionScope::Components => transaction.copy_current_components(),
            TransactionScope::Tree => copy_tree(&transaction.install_dir, &transaction.backup_dir),
        };
        if let Err(error) = backup {
            let _ = fs::remove_dir_all(&transaction.backup_dir);
            return Err(error);
        }
        transaction.write_state(TransactionPhase::Installing)?;
        Ok(transaction)
    }

    pub fn backup_dir(&self) -> &Path {
        &self.backup_dir
    }

    pub fn state_path(&self) -> &Path {
        &self.state_path
    }

    pub fn commit(&self) -> io::Result<()> {
        self.write_state(TransactionPhase::Committed)?;
        fs::remove_dir_all(&self.backup_dir)?;
        fs::remove_file(&self.state_path)
    }

    pub fn rollback(&self) -> io::Result<()> {
        self.write_state(TransactionPhase::RollbackRequired)?;
        match self.scope {
            TransactionScope::Components => {
                for component in Self::COMPONENTS {
                    let source = self.backup_dir.join(component);
                    let destination = self.install_dir.join(component);
                    restore_file(&source, &destination)?;
                }
            }
            // Files the failed package added are left behind: overwriting the
            // previous tree is what makes the installation work again, and
            // deleting unknown files is the riskier half of the operation.
            TransactionScope::Tree => copy_tree(&self.backup_dir, &self.install_dir)?,
        }
        self.write_state(TransactionPhase::Restored)?;
        fs::remove_dir_all(&self.backup_dir)?;
        fs::remove_file(&self.state_path)
    }

    pub fn recover(state_dir: &Path) -> io::Result<RecoveryResult> {
        validate_absolute(state_dir, "state directory")?;
        let state_path = state_dir.join("transaction.json");
        if !state_path.exists() {
            return Ok(RecoveryResult { recovered: false });
        }
        let state: TransactionState = serde_json::from_slice(&fs::read(&state_path)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let transaction = Self {
            install_dir: state.install_dir,
            backup_dir: state.backup_dir,
            state_path,
            scope: state.scope,
        };
        match state.phase {
            TransactionPhase::Committed => {
                if transaction.backup_dir.exists() {
                    fs::remove_dir_all(&transaction.backup_dir)?;
                }
                fs::remove_file(&transaction.state_path)?;
            }
            TransactionPhase::Installing | TransactionPhase::RollbackRequired => {
                transaction.rollback()?;
            }
            TransactionPhase::Restored => {
                if transaction.backup_dir.exists() {
                    fs::remove_dir_all(&transaction.backup_dir)?;
                }
                fs::remove_file(&transaction.state_path)?;
            }
        }
        Ok(RecoveryResult { recovered: true })
    }

    fn copy_current_components(&self) -> io::Result<()> {
        for component in Self::COMPONENTS {
            let source = self.install_dir.join(component);
            if !source.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "required installed component is missing: {}",
                        source.display()
                    ),
                ));
            }
            fs::copy(&source, self.backup_dir.join(component))?;
        }
        Ok(())
    }

    fn write_state(&self, phase: TransactionPhase) -> io::Result<()> {
        let state = serde_json::to_vec_pretty(&TransactionState {
            install_dir: self.install_dir.clone(),
            backup_dir: self.backup_dir.clone(),
            phase,
            scope: self.scope,
        })
        .map_err(io::Error::other)?;
        let temporary = self.state_path.with_extension("json.tmp");
        fs::write(&temporary, state)?;
        fs::rename(temporary, &self.state_path)
    }
}

/// Recursive copy that overwrites the destination and keeps extra files there.
fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            copy_file_resilient(&entry.path(), &target)?;
        }
        // Symlinks and reparse points are skipped: an update payload has no
        // reason to carry one, and following it would write outside the tree.
    }
    Ok(())
}

/// Windows codes for a file another process is holding open.
const SHARING_VIOLATION: i32 = 32;
const LOCK_VIOLATION: i32 = 33;
const ACCESS_DENIED: i32 = 5;
const COPY_RETRY_LIMIT: Duration = Duration::from_secs(30);

/// Copies one file, retrying while something still holds it open.
///
/// On Windows a virus scanner or a lingering child process can hold a file for
/// a moment right after it appears. Retrying turns that flake into a slightly
/// slower update instead of a rollback.
fn copy_file_resilient(source: &Path, destination: &Path) -> io::Result<()> {
    let deadline = std::time::Instant::now() + COPY_RETRY_LIMIT;
    loop {
        let attempt = if destination.exists() {
            fs::remove_file(destination).and_then(|()| fs::copy(source, destination).map(|_| ()))
        } else {
            fs::copy(source, destination).map(|_| ())
        };
        match attempt {
            Ok(()) => return Ok(()),
            Err(error) if is_locked(&error) && std::time::Instant::now() < deadline => {
                std::thread::sleep(RETRY_INTERVAL);
            }
            Err(error) => {
                return Err(io::Error::new(
                    error.kind(),
                    format!("cannot write {}: {error}", destination.display()),
                ))
            }
        }
    }
}

fn is_locked(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(SHARING_VIOLATION) | Some(LOCK_VIOLATION) | Some(ACCESS_DENIED)
    ) || error.kind() == io::ErrorKind::PermissionDenied
}

/// Waits for the shell to release its files, giving up after `limit`.
#[cfg(windows)]
fn wait_for_process_exit(pid: u32, limit: Duration) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};

    /// `SYNCHRONIZE` — the only right needed to wait on a process handle.
    const SYNCHRONIZE: u32 = 0x0010_0000;

    // SAFETY: the handle is closed on every path, and a failed open simply
    // means the process is already gone.
    unsafe {
        let handle = OpenProcess(SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return;
        }
        WaitForSingleObject(handle, limit.as_millis().min(u128::from(u32::MAX)) as u32);
        CloseHandle(handle);
    }
}

#[cfg(not(windows))]
fn wait_for_process_exit(_pid: u32, _limit: Duration) {}

fn restore_file(source: &Path, destination: &Path) -> io::Result<()> {
    if !source.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("backup component is missing: {}", source.display()),
        ));
    }
    let temporary = destination.with_extension("rollback.tmp");
    fs::copy(source, &temporary)?;
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    fs::rename(temporary, destination)
}

fn validate_absolute(path: &Path, label: &str) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{label} must be absolute: {}", path.display()),
        ));
    }
    Ok(())
}

fn timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::{verify_installation, UpdateTransaction};
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("evohime-updater-{name}-{nonce}"))
    }

    fn write_components(dir: &Path, prefix: &str) {
        fs::create_dir_all(dir).unwrap();
        for component in UpdateTransaction::COMPONENTS {
            fs::write(dir.join(component), format!("{prefix}:{component}")).unwrap();
        }
    }

    #[test]
    fn prepare_commit_removes_backup_and_state() {
        let root = temp_dir("commit");
        let install = root.join("install");
        let state = root.join("state");
        write_components(&install, "old");

        let transaction = UpdateTransaction::prepare(&install, &state).unwrap();
        assert!(transaction.backup_dir().exists());
        assert!(transaction.state_path().exists());

        transaction.commit().unwrap();

        assert!(!transaction.backup_dir().exists());
        assert!(!transaction.state_path().exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_restores_components_after_partial_install() {
        let root = temp_dir("rollback");
        let install = root.join("install");
        let state = root.join("state");
        write_components(&install, "old");
        let transaction = UpdateTransaction::prepare(&install, &state).unwrap();

        fs::write(install.join("EvoHime.exe"), "new").unwrap();
        fs::remove_file(install.join("evohime-core.exe")).unwrap();
        transaction.rollback().unwrap();

        for component in UpdateTransaction::COMPONENTS {
            assert_eq!(
                fs::read_to_string(install.join(component)).unwrap(),
                format!("old:{component}")
            );
        }
        assert!(!transaction.state_path().exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recover_rolls_back_leftover_installing_transaction() {
        let root = temp_dir("recover");
        let install = root.join("install");
        let state = root.join("state");
        write_components(&install, "old");
        let transaction = UpdateTransaction::prepare(&install, &state).unwrap();
        fs::write(install.join("EvoHime.exe"), "interrupted").unwrap();

        let result = UpdateTransaction::recover(&state).unwrap();

        assert!(result.recovered);
        assert_eq!(
            fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
            "old:EvoHime.exe"
        );
        assert!(!transaction.state_path().exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn apply_staged_replaces_installation_and_keeps_extra_files() {
        let root = temp_dir("staged");
        let install = root.join("install");
        let staging = root.join("staging");
        let state = root.join("state");
        write_components(&install, "old");
        fs::write(install.join("user-note.txt"), "keep me").unwrap();
        write_components(&staging, "new");
        fs::create_dir_all(staging.join("resources")).unwrap();
        fs::write(staging.join("resources").join("icon.ico"), "icon").unwrap();

        super::apply_staged(super::StagedApply {
            staging: &staging,
            install_dir: &install,
            state_dir: &state,
            wait_pid: None,
            relaunch: None,
        })
        .unwrap();

        assert_eq!(
            fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
            "new:EvoHime.exe"
        );
        assert_eq!(
            fs::read_to_string(install.join("resources").join("icon.ico")).unwrap(),
            "icon"
        );
        assert_eq!(
            fs::read_to_string(install.join("user-note.txt")).unwrap(),
            "keep me"
        );
        assert!(!state.join("transaction.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn apply_staged_refuses_an_incomplete_package() {
        let root = temp_dir("staged-incomplete");
        let install = root.join("install");
        let staging = root.join("staging");
        let state = root.join("state");
        write_components(&install, "old");
        write_components(&staging, "new");
        fs::remove_file(staging.join("evohime-core.exe")).unwrap();

        let error = super::apply_staged(super::StagedApply {
            staging: &staging,
            install_dir: &install,
            state_dir: &state,
            wait_pid: None,
            relaunch: None,
        })
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        // The installation was never touched, so no transaction is left behind.
        assert_eq!(
            fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
            "old:EvoHime.exe"
        );
        assert!(!state.join("transaction.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tree_rollback_restores_a_partially_replaced_installation() {
        let root = temp_dir("tree-rollback");
        let install = root.join("install");
        let state = root.join("state");
        write_components(&install, "old");
        fs::create_dir_all(install.join("resources")).unwrap();
        fs::write(install.join("resources").join("app.asar"), "old-asar").unwrap();

        let transaction = UpdateTransaction::prepare_tree(&install, &state).unwrap();
        fs::write(install.join("resources").join("app.asar"), "broken").unwrap();
        fs::remove_file(install.join("evohime-core.exe")).unwrap();

        transaction.rollback().unwrap();

        assert_eq!(
            fs::read_to_string(install.join("resources").join("app.asar")).unwrap(),
            "old-asar"
        );
        assert_eq!(
            fs::read_to_string(install.join("evohime-core.exe")).unwrap(),
            "old:evohime-core.exe"
        );
        assert!(!transaction.state_path().exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recover_rolls_back_a_leftover_tree_transaction() {
        let root = temp_dir("tree-recover");
        let install = root.join("install");
        let state = root.join("state");
        write_components(&install, "old");
        let transaction = UpdateTransaction::prepare_tree(&install, &state).unwrap();
        fs::write(install.join("EvoHime.exe"), "interrupted").unwrap();

        assert!(UpdateTransaction::recover(&state).unwrap().recovered);

        assert_eq!(
            fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
            "old:EvoHime.exe"
        );
        assert!(!transaction.state_path().exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn a_locked_installation_is_waited_for_and_never_half_written() {
        use std::os::windows::fs::OpenOptionsExt;
        use std::time::{Duration, Instant};

        let root = temp_dir("locked");
        let install = root.join("install");
        write_components(&install, "old");

        // FILE_SHARE_READ is how Windows holds a running image: readable by
        // anyone, writable by no one. Electron's children keep the executable
        // open like this for a moment after the main process is gone.
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(install.join("EvoHime.exe"))
            .unwrap();

        let started = Instant::now();
        let error = super::wait_until_writable(&install, Duration::from_millis(600)).unwrap_err();

        assert!(started.elapsed() >= Duration::from_millis(500));
        assert!(error.to_string().contains("still in use"), "{error}");
        assert_eq!(
            fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
            "old:EvoHime.exe"
        );

        // Once the handle is gone the same check passes immediately.
        drop(held);
        super::wait_until_writable(&install, Duration::from_millis(600)).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn waiting_ignores_components_a_fresh_installation_has_not_written_yet() {
        let root = temp_dir("writable");
        write_components(&root, "old");
        fs::remove_file(root.join("evohime.manifest.json")).unwrap();

        super::wait_until_writable(&root, std::time::Duration::from_millis(100)).unwrap();

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verify_installation_rejects_missing_component() {
        let root = temp_dir("verify");
        write_components(&root, "installed");
        fs::remove_file(root.join("evohime-supervisor.exe")).unwrap();

        let error = verify_installation(&root).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        fs::remove_dir_all(root).unwrap();
    }
}
