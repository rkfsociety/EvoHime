//! Supervisor-owned launch boundary for the Persistent Analysis Kernel.
//!
//! The model never supplies an executable, arguments, environment or working
//! directory. The supervisor resolves one fixed sibling binary and places it
//! in a separate kill-on-close Job Object with explicit limits.

use std::{io, path::PathBuf};

use tokio::process::{Child, Command};

use super::JobObject;

pub const KERNEL_WORKER_PROTOCOL_VERSION: u32 = 1;
pub const KERNEL_WORKER_MEMORY_LIMIT_BYTES: u64 = 512 * 1024 * 1024;
pub const KERNEL_WORKER_CPU_LIMIT_PERCENT: u8 = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelWorkerLaunchSpec {
    pub runtime_version: String,
    pub package_manifest_hash: String,
}

impl KernelWorkerLaunchSpec {
    pub fn validate(&self) -> io::Result<()> {
        if self.runtime_version != "trusted-local-1" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported kernel runtime identity",
            ));
        }
        if self.package_manifest_hash.len() != 64
            || !self
                .package_manifest_hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid kernel package manifest hash",
            ));
        }
        Ok(())
    }
}

pub(crate) struct KernelWorkerProcess {
    _job: JobObject,
    child: Child,
}

impl KernelWorkerProcess {
    pub(crate) fn spawn(supervisor_exe: PathBuf, spec: KernelWorkerLaunchSpec) -> io::Result<Self> {
        spec.validate()?;
        let worker = supervisor_exe
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "supervisor directory missing"))?
            .join("evohime-analysis-worker.exe");
        let job = JobObject::create_with_limits(
            Some(KERNEL_WORKER_MEMORY_LIMIT_BYTES),
            Some(KERNEL_WORKER_CPU_LIMIT_PERCENT),
        )?;
        let mut command = Command::new(worker);
        // Fixed arguments only. Runtime identity is selected by the supervisor
        // and is not interpolated into a command line supplied by the model.
        command
            .arg("--protocol-version=1")
            .arg("--runtime=trusted-local-1")
            .env_clear()
            .env("EVOHIME_KERNEL_MODE", "trusted_local_analysis")
            .env("EVOHIME_KERNEL_PACKAGE_HASH", spec.package_manifest_hash)
            .current_dir(supervisor_exe.parent().ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "supervisor directory missing")
            })?)
            .kill_on_drop(true);
        let child = command.spawn()?;
        job.assign(&child)?;
        Ok(Self { _job: job, child })
    }

    pub(crate) fn child_id(&self) -> Option<u32> {
        self.child.id()
    }

    pub(crate) async fn stop(mut self) -> io::Result<()> {
        self.child.kill().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_spec_is_allowlisted() {
        let valid = KernelWorkerLaunchSpec {
            runtime_version: "trusted-local-1".into(),
            package_manifest_hash: "a".repeat(64),
        };
        assert!(valid.validate().is_ok());
        let mut invalid = valid;
        invalid.runtime_version = "arbitrary".into();
        assert!(invalid.validate().is_err());
    }
}
