//! Platform clocks and boot identity used by receipt approval recovery.

use crate::runtime_contract::RuntimeError;
use serde_json::{json, Value};
use std::sync::OnceLock;
use uuid::Uuid;

static BOOT_ID: OnceLock<String> = OnceLock::new();

/// Returns the OS monotonic uptime, not a process-local elapsed clock. This
/// is persisted in approval intents and therefore must remain comparable
/// across Core restarts within the same OS boot.
pub(crate) fn monotonic_ms() -> Result<i64, RuntimeError> {
    #[cfg(windows)]
    {
        Ok(unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() as i64 })
    }
    #[cfg(target_os = "linux")]
    {
        let uptime = std::fs::read_to_string("/proc/uptime")
            .map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
        let seconds = uptime
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<f64>().ok())
            .ok_or(RuntimeError::Code("storage_key_unavailable"))?;
        Ok((seconds * 1000.0).floor() as i64)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Err(RuntimeError::Code("storage_key_unavailable"))
    }
}

/// Stable identifier for the current OS boot session. Windows uses
/// `GetTickCount64`, which resets to zero on every boot and is therefore a
/// reliable platform clock API. Elsewhere the code first tries a real
/// platform signal (`/proc/uptime` on Linux) and only falls back to an
/// owner-only, checksummed boot marker file when no such signal exists. If
/// neither is available the caller receives a fail-closed runtime error
/// instead of a silently process-local, unreliable identifier.
pub(crate) fn boot_id() -> Result<&'static str, RuntimeError> {
    if let Some(id) = BOOT_ID.get() {
        return Ok(id);
    }
    let computed = compute_boot_id()?;
    Ok(BOOT_ID.get_or_init(|| computed))
}

fn compute_boot_id() -> Result<String, RuntimeError> {
    #[cfg(windows)]
    {
        Ok(format!("windows-boot-{}", unsafe {
            windows_sys::Win32::System::SystemInformation::GetTickCount64()
        }))
    }
    #[cfg(not(windows))]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/uptime") {
            if let Some(uptime_secs) = contents
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<f64>().ok())
            {
                if let Ok(now_secs) =
                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                {
                    let boot_epoch = (now_secs.as_secs_f64() - uptime_secs).round() as i64;
                    return Ok(format!("linux-boot-{boot_epoch}"));
                }
            }
        }
        boot_marker_id()
    }
}

#[cfg(not(windows))]
const BOOT_MARKER_SCHEMA_VERSION: u8 = 1;

/// Platform data directory for the owner-only boot marker, matching the
/// paths named in the plan for non-Windows targets.
#[cfg(not(windows))]
fn runtime_data_dir() -> Option<std::path::PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return Some(
                std::path::PathBuf::from(xdg)
                    .join("EvoHime")
                    .join("runtime"),
            );
        }
    }
    let home = std::env::var("HOME").ok()?;
    if home.is_empty() {
        return None;
    }
    if cfg!(target_os = "macos") {
        Some(
            std::path::PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("EvoHime")
                .join("runtime"),
        )
    } else {
        Some(
            std::path::PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("EvoHime")
                .join("runtime"),
        )
    }
}

#[cfg(not(windows))]
fn boot_marker_checksum(id: &str) -> String {
    crate::sha256_hex(format!("evohime-boot-marker-v1\0{id}").as_bytes())
}

#[cfg(not(windows))]
fn parse_boot_marker(bytes: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    if value.get("schema_version")?.as_u64()? != BOOT_MARKER_SCHEMA_VERSION as u64 {
        return None;
    }
    let id = value.get("boot_id")?.as_str()?.to_string();
    let checksum = value.get("checksum")?.as_str()?;
    if checksum != boot_marker_checksum(&id) {
        return None;
    }
    Some(id)
}

/// Reads or atomically (re)creates the boot marker. Loss, corruption or a
/// checksum mismatch is treated as a new boot, matching the plan's
/// invalidation rule; the file is never patched in place.
#[cfg(not(windows))]
fn boot_marker_id() -> Result<String, RuntimeError> {
    let dir = runtime_data_dir().ok_or(RuntimeError::Code("storage_key_unavailable"))?;
    std::fs::create_dir_all(&dir).map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    let marker_path = dir.join("boot-marker.json");
    if let Ok(bytes) = std::fs::read(&marker_path) {
        if let Some(id) = parse_boot_marker(&bytes) {
            return Ok(id);
        }
    }
    let id = format!("marker-boot-{}", Uuid::now_v7());
    let value = json!({"schema_version": BOOT_MARKER_SCHEMA_VERSION, "boot_id": id, "checksum": boot_marker_checksum(&id)});
    let bytes =
        serde_json::to_vec(&value).map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
    let tmp_path = dir.join(format!("boot-marker.{}.tmp", Uuid::now_v7()));
    std::fs::write(&tmp_path, &bytes).map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp_path, &marker_path)
        .map_err(|_| RuntimeError::Code("storage_key_unavailable"))?;
    Ok(id)
}
