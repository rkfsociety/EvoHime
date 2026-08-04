use std::{ffi::OsStr, io, mem::size_of, os::windows::ffi::OsStrExt, path::PathBuf, ptr};

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
    let core_exe = std::env::var_os("EVOHIME_CORE_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("evohime-core.exe"));
    let max_restarts = std::env::var("EVOHIME_CORE_MAX_RESTARTS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(3);
    let mut restarts = 0;

    loop {
        let job = JobObject::create()?;
        let mut child = Command::new(&core_exe).kill_on_drop(true).spawn()?;
        job.assign(&child)?;
        let status = child.wait().await?;
        if status.success() || restarts >= max_restarts {
            return Ok(());
        }
        restarts += 1;
        sleep(Duration::from_millis(250 * u64::from(restarts))).await;
    }
}
