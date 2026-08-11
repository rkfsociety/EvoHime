use serde_json::json;
use std::{
    ffi::OsStr,
    fs,
    io,
    mem::size_of,
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    ptr,
    sync::Mutex,
    time::{Duration as StdDuration, SystemTime, UNIX_EPOCH},
};

use std::path::Path;

use tokio::{
    process::Command,
    time::{sleep, Duration},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    },
    System::Threading::CreateMutexW,
};

struct SingleInstance(HANDLE);

struct SupervisorLogger(Mutex<std::io::BufWriter<std::fs::File>>);

impl SupervisorLogger {
    fn open() -> io::Result<Self> {
        let dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".evohime"))
            .join("EvoHime/logs");
        std::fs::create_dir_all(&dir)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("supervisor.jsonl"))?;
        Ok(Self(Mutex::new(std::io::BufWriter::new(file))))
    }

    fn write(&self, event: &str, fields: serde_json::Value) -> io::Result<()> {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut file = self
            .0
            .lock()
            .map_err(|_| io::Error::other("supervisor logger lock poisoned"))?;
        serde_json::to_writer(
            &mut *file,
            &json!({
                "timestamp_ms": timestamp_ms,
                "level": "info",
                "event": event,
                "fields": fields,
            }),
        )?;
        use std::io::Write;
        file.write_all(b"\n")?;
        file.flush()
    }
}

impl SingleInstance {
    fn acquire(name: &str) -> io::Result<Self> {
        let wide: Vec<u16> = OsStr::new(name).encode_wide().chain(Some(0)).collect();
        let handle = unsafe { CreateMutexW(ptr::null(), 1, wide.as_ptr()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(handle) };
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "EvoHime is already running",
            ));
        }
        Ok(Self(handle))
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

struct JobObject(HANDLE);

pub fn recover_pending_update(state_dir: &Path) -> io::Result<bool> {
    Ok(evohime_tx::UpdateTransaction::recover(state_dir)?.recovered)
}

fn update_state_dir() -> PathBuf {
    std::env::var_os("EVOHIME_UPDATE_STATE_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|path| path.join("EvoHime/update-state"))
        })
        .unwrap_or_else(|| PathBuf::from(".evohime/update-state"))
}

impl JobObject {
    fn create() -> io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            unsafe { CloseHandle(handle) };
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }

    fn assign(&self, child: &tokio::process::Child) -> io::Result<()> {
        let process = child
            .raw_handle()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "core process has no handle"))?;
        if unsafe { AssignProcessToJobObject(self.0, process) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let _instance = SingleInstance::acquire("Local\\EvoHime.Supervisor")?;
    let logger = SupervisorLogger::open()?;
    let state_dir = update_state_dir();
    match recover_pending_update(&state_dir) {
        Ok(true) => {
            let _ = logger.write(
                "update.recovered",
                json!({"state_dir": state_dir.display().to_string()}),
            );
        }
        Ok(false) => {}
        Err(error) => {
            let _ = logger.write(
                "update.recovery_failed",
                json!({"state_dir": state_dir.display().to_string(), "error": error.to_string()}),
            );
            return Err(error.into());
        }
    }
    let core_exe = std::env::var_os("EVOHIME_CORE_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("evohime-core.exe"));
    let max_restarts = std::env::var("EVOHIME_CORE_MAX_RESTARTS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(3);
    let mut restarts = 0;
    let heartbeat_path = core_data_dir().join("core-heartbeat");

    loop {
        let _ = logger.write("core.spawn", json!({"restart": restarts}));
        let job = JobObject::create()?;
        let mut child = Command::new(&core_exe).kill_on_drop(true).spawn()?;
        job.assign(&child)?;
        let status = wait_for_core(&mut child, &heartbeat_path, &logger).await?;
        let _ = logger.write(
            "core.exit",
            json!({"success": status.success(), "code": status.code()}),
        );
        if status.success() || restarts >= max_restarts {
            return Ok(());
        }
        restarts += 1;
        let _ = logger.write("core.restart", json!({"restart": restarts}));
        sleep(Duration::from_millis(250 * u64::from(restarts))).await;
    }
}

fn core_data_dir() -> PathBuf {
    std::env::var_os("EVOHIME_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|path| path.join("EvoHime"))
        })
        .unwrap_or_else(|| PathBuf::from(".evohime"))
}

fn heartbeat_is_stale(path: &Path, max_age: StdDuration) -> bool {
    let modified = fs::metadata(path).and_then(|metadata| metadata.modified());
    modified
        .map(|time| SystemTime::now().duration_since(time).unwrap_or_default() > max_age)
        .unwrap_or(true)
}

async fn wait_for_core(
    child: &mut tokio::process::Child,
    heartbeat_path: &Path,
    logger: &SupervisorLogger,
) -> io::Result<std::process::ExitStatus> {
    let started = tokio::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if started.elapsed() > Duration::from_secs(10)
            && heartbeat_is_stale(heartbeat_path, StdDuration::from_secs(5))
        {
            let _ = logger.write(
                "core.health_timeout",
                json!({"heartbeat": heartbeat_path.display().to_string()}),
            );
            child.kill().await?;
            return child.wait().await;
        }
        sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{heartbeat_is_stale, recover_pending_update};
    use evohime_tx::UpdateTransaction;
    use std::fs;
    use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};

    #[test]
    fn recovers_pending_update_before_core_start() {
        let root = std::env::temp_dir().join(format!(
            "evohime-supervisor-recovery-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let install = root.join("install");
        let state = root.join("state");
        fs::create_dir_all(&install).unwrap();
        for component in UpdateTransaction::COMPONENTS {
            fs::write(install.join(component), format!("old:{component}")).unwrap();
        }
        let transaction = UpdateTransaction::prepare(&install, &state).unwrap();
        fs::write(install.join("EvoHime.exe"), "interrupted").unwrap();

        assert!(recover_pending_update(&state).unwrap());
        assert_eq!(
            fs::read_to_string(install.join("EvoHime.exe")).unwrap(),
            "old:EvoHime.exe"
        );
        assert!(!transaction.state_path().exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_core_heartbeat_is_stale() {
        let path = std::env::temp_dir().join(format!("evohime-missing-heartbeat-{}", std::process::id()));
        assert!(heartbeat_is_stale(&path, StdDuration::from_secs(5)));
    }
}
