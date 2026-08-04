use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize)]
struct TransactionState {
    install_dir: PathBuf,
    backup_dir: PathBuf,
    phase: TransactionPhase,
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
}

impl UpdateTransaction {
    pub const COMPONENTS: [&'static str; 4] = [
        "EvoHime.exe",
        "evohime-core.exe",
        "evohime-supervisor.exe",
        "evohime.manifest.json",
    ];

    pub fn prepare(install_dir: &Path, state_dir: &Path) -> io::Result<Self> {
        validate_absolute(install_dir, "install directory")?;
        validate_absolute(state_dir, "state directory")?;
        if !install_dir.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("install directory does not exist: {}", install_dir.display()),
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
        };
        if let Err(error) = transaction.copy_current_components() {
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
        for component in Self::COMPONENTS {
            let source = self.backup_dir.join(component);
            let destination = self.install_dir.join(component);
            restore_file(&source, &destination)?;
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
                    format!("required installed component is missing: {}", source.display()),
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
        })
        .map_err(io::Error::other)?;
        let temporary = self.state_path.with_extension("json.tmp");
        fs::write(&temporary, state)?;
        fs::rename(temporary, &self.state_path)
    }
}

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
    use super::UpdateTransaction;
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
            assert_eq!(fs::read_to_string(install.join(component)).unwrap(), format!("old:{component}"));
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
        assert_eq!(fs::read_to_string(install.join("EvoHime.exe")).unwrap(), "old:EvoHime.exe");
        assert!(!transaction.state_path().exists());
        fs::remove_dir_all(root).unwrap();
    }
}
