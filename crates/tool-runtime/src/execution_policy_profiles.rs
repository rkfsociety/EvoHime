//! Core-resolved execution profiles for process based tools.
//!
//! The command text is never allowed to select the backend or widen the
//! environment.  `ToolRegistry` and the two process tools use this module as
//! their single resolver.  Durable profile catalogs belong to Core/storage;
//! this crate only owns the bounded runtime contract and ephemeral process
//! guard.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, io, time::Duration};
use tokio::process::{Child, Command};

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_PROFILE_ID: usize = 64;
pub const MAX_TIMEOUT_MS: u64 = 60_000;
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendRequirement {
    Portable,
    WindowsJobObject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    Deny,
    Inherit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentPolicy {
    ScrubbedAllowlist,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPolicyProfile {
    pub schema_version: u32,
    pub profile_id: String,
    pub version: u64,
    pub backend: BackendRequirement,
    pub sandbox_required: bool,
    pub network: NetworkPolicy,
    pub environment: EnvironmentPolicy,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
    pub kill_process_tree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedExecutionProfile {
    pub profile: ExecutionPolicyProfile,
    pub profile_hash: String,
    pub backend: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionPolicyError {
    InvalidProfile(&'static str),
    UnsupportedVersion(u32),
    BackendUnavailable,
    UnsupportedTool,
}

impl std::fmt::Display for ExecutionPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidProfile(reason) => write!(f, "invalid execution profile: {reason}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported execution profile version: {version}")
            }
            Self::BackendUnavailable => f.write_str("required execution backend unavailable"),
            Self::UnsupportedTool => f.write_str("tool is not a process entrypoint"),
        }
    }
}

impl std::error::Error for ExecutionPolicyError {}

impl ExecutionPolicyProfile {
    pub fn default_for(tool: &str) -> Result<Self, ExecutionPolicyError> {
        if !matches!(tool, "shell.execute" | "process.run") {
            return Err(ExecutionPolicyError::UnsupportedTool);
        }
        Ok(Self {
            schema_version: CONTRACT_VERSION,
            profile_id: "restricted-process-v1".into(),
            version: 1,
            backend: if cfg!(windows) {
                BackendRequirement::WindowsJobObject
            } else {
                BackendRequirement::Portable
            },
            sandbox_required: cfg!(windows),
            network: NetworkPolicy::Deny,
            environment: EnvironmentPolicy::ScrubbedAllowlist,
            timeout_ms: MAX_TIMEOUT_MS,
            max_output_bytes: MAX_OUTPUT_BYTES,
            kill_process_tree: true,
        })
    }

    pub fn validate(&self) -> Result<(), ExecutionPolicyError> {
        if self.schema_version != CONTRACT_VERSION {
            return Err(ExecutionPolicyError::UnsupportedVersion(
                self.schema_version,
            ));
        }
        if self.profile_id.is_empty() || self.profile_id.len() > MAX_PROFILE_ID {
            return Err(ExecutionPolicyError::InvalidProfile("profile_id bound"));
        }
        if self.version == 0 || self.timeout_ms == 0 || self.timeout_ms > MAX_TIMEOUT_MS {
            return Err(ExecutionPolicyError::InvalidProfile(
                "timeout/version bound",
            ));
        }
        if self.max_output_bytes == 0 || self.max_output_bytes > MAX_OUTPUT_BYTES {
            return Err(ExecutionPolicyError::InvalidProfile("output bound"));
        }
        if !self.kill_process_tree {
            return Err(ExecutionPolicyError::InvalidProfile(
                "tree cleanup is required",
            ));
        }
        if self.sandbox_required
            && !cfg!(windows)
            && self.backend == BackendRequirement::WindowsJobObject
        {
            return Err(ExecutionPolicyError::BackendUnavailable);
        }
        Ok(())
    }

    pub fn resolve(tool: &str) -> Result<ResolvedExecutionProfile, ExecutionPolicyError> {
        let profile = Self::default_for(tool)?;
        profile.validate()?;
        let canonical = serde_json::to_vec(&profile)
            .map_err(|_| ExecutionPolicyError::InvalidProfile("canonical serialization"))?;
        let mut hasher = Sha256::new();
        hasher.update(b"evohime-execution-policy-profile-v1\0");
        hasher.update(canonical);
        Ok(ResolvedExecutionProfile {
            backend: match profile.backend {
                BackendRequirement::Portable => "portable".into(),
                BackendRequirement::WindowsJobObject => "windows_job_object".into(),
            },
            profile,
            profile_hash: hex::encode(hasher.finalize()),
        })
    }
}

pub fn validate_program_name(program: &str) -> Result<(), ExecutionPolicyError> {
    if program.is_empty()
        || program.contains(['/', '\\'])
        || matches!(
            program.to_ascii_lowercase().as_str(),
            "cmd"
                | "cmd.exe"
                | "powershell"
                | "powershell.exe"
                | "pwsh"
                | "pwsh.exe"
                | "sh"
                | "bash"
                | "zsh"
                | "fish"
                | "wsl"
                | "wsl.exe"
                | "python"
                | "python3"
                | "python.exe"
                | "py"
                | "node"
                | "node.exe"
                | "npm"
                | "npm.cmd"
                | "npx"
                | "npx.cmd"
                | "uv"
                | "uvx"
                | "perl"
                | "ruby"
                | "php"
                | "wscript"
                | "wscript.exe"
                | "cscript"
                | "cscript.exe"
                | "mshta"
                | "mshta.exe"
                | "rundll32"
                | "rundll32.exe"
        )
    {
        return Err(ExecutionPolicyError::InvalidProfile(
            "program must be a direct executable name",
        ));
    }
    Ok(())
}

impl ResolvedExecutionProfile {
    pub fn timeout(&self, requested_ms: Option<u64>) -> Duration {
        Duration::from_millis(
            requested_ms
                .unwrap_or(self.profile.timeout_ms)
                .min(self.profile.timeout_ms),
        )
    }
}

/// Applies the profile's deny-by-default environment.  Values are deliberately
/// not accepted from a tool input; this map is only the bounded inherited
/// environment selected by `shell_env`.
pub fn apply_environment(command: &mut Command) {
    crate::shell_env::apply_scrubbed_env(command);
}

pub fn reject_user_environment(
    env: Option<&HashMap<String, String>>,
) -> Result<(), ExecutionPolicyError> {
    if env.is_some_and(|values| !values.is_empty()) {
        return Err(ExecutionPolicyError::InvalidProfile(
            "user environment is not allowed",
        ));
    }
    Ok(())
}

#[derive(Debug)]
pub struct ProcessGuard {
    #[cfg(windows)]
    #[allow(dead_code)]
    job: Option<WindowsJobObject>,
}

impl ProcessGuard {
    pub fn attach(child: &Child, profile: &ResolvedExecutionProfile) -> io::Result<Self> {
        #[cfg(windows)]
        {
            if profile.profile.backend == BackendRequirement::WindowsJobObject {
                return Ok(Self {
                    job: Some(WindowsJobObject::create_and_assign(child)?),
                });
            }
        }
        #[cfg(not(windows))]
        let _ = (child, profile);
        Ok(Self {
            #[cfg(windows)]
            job: None,
        })
    }
}

#[cfg(windows)]
#[derive(Debug)]
struct WindowsJobObject(windows_sys::Win32::Foundation::HANDLE);

// The handle is an owned kernel object; ownership moves with the guard and
// Drop closes it exactly once.
#[cfg(windows)]
unsafe impl Send for WindowsJobObject {}
#[cfg(windows)]
unsafe impl Sync for WindowsJobObject {}

#[cfg(windows)]
impl WindowsJobObject {
    fn create_and_assign(child: &Child) -> io::Result<Self> {
        use std::{mem::size_of, ptr::null_mut};
        use windows_sys::Win32::{
            Foundation::{CloseHandle, HANDLE},
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
        };
        let handle: HANDLE = unsafe { CreateJobObjectW(null_mut(), null_mut()) };
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
        let process = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("child process has no handle"))?;
        if unsafe { AssignProcessToJobObject(handle, process) } == 0 {
            unsafe { CloseHandle(handle) };
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }
}

#[cfg(windows)]
impl Drop for WindowsJobObject {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_is_bounded_and_hashed() {
        let resolved = ExecutionPolicyProfile::resolve("shell.execute").unwrap();
        assert_eq!(resolved.profile.schema_version, 1);
        assert_eq!(resolved.profile_hash.len(), 64);
        assert_eq!(resolved.profile.network, NetworkPolicy::Deny);
    }

    #[test]
    fn arbitrary_environment_is_rejected() {
        let mut env = HashMap::new();
        env.insert("EVOHIME_API_TOKEN".into(), "secret".into());
        assert!(reject_user_environment(Some(&env)).is_err());
    }
}
