//! Windows-specific transport security for the Core named pipe.
//!
//! Two mechanisms are combined:
//!
//! * the pipe is created with an explicit, protected DACL that grants access
//!   only to the user that owns the session, so another user or another logon
//!   session cannot open it at all;
//! * the identity of a connected client is read from the operating system
//!   (impersonation of the pipe client), never from what the client claims.
//!
//! Neither mechanism protects against code already running as the same user;
//! that limit is documented in `docs/security/`.

use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::RawHandle;

use windows_sys::Win32::Foundation::{
    CloseHandle, LocalFree, ERROR_ALREADY_EXISTS, HANDLE, HLOCAL, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, RevertToSelf, TokenStatistics, TokenUser, PSECURITY_DESCRIPTOR,
    SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_STATISTICS, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;
use windows_sys::Win32::System::Pipes::ImpersonateNamedPipeClient;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
};

use crate::session::PeerIdentity;

/// Owns a security descriptor built from SDDL together with the
/// `SECURITY_ATTRIBUTES` that point at it. Both must outlive the pipe creation
/// call, which is why they are kept in one value.
pub struct PipeSecurity {
    descriptor: PSECURITY_DESCRIPTOR,
    attributes: SECURITY_ATTRIBUTES,
}

impl PipeSecurity {
    /// Grants full access to `user_sid` only, with inheritance disabled.
    pub fn owner_only(user_sid: &str) -> io::Result<Self> {
        if user_sid.is_empty() || !user_sid.starts_with("S-") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "pipe DACL requires a valid user SID",
            ));
        }
        let sddl = format!("D:P(A;;GA;;;{user_sid})");
        let wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();

        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let created = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if created == 0 {
            return Err(io::Error::last_os_error());
        }

        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        Ok(Self {
            descriptor,
            attributes,
        })
    }

    /// Raw pointer for `ServerOptions::create_with_security_attributes_raw`.
    pub fn as_raw(&mut self) -> *mut c_void {
        std::ptr::addr_of_mut!(self.attributes).cast()
    }
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        if !self.descriptor.is_null() {
            unsafe { LocalFree(self.descriptor as HLOCAL) };
        }
    }
}

// The descriptor is an owned allocation that is only read while creating a
// pipe and freed on drop; moving it between threads is sound.
unsafe impl Send for PipeSecurity {}

/// Creates (or keeps) a directory whose DACL grants the owning user only.
/// Files written inside inherit that DACL, which is how the launch context
/// stays readable by this user's processes and by nobody else.
pub fn create_protected_directory(path: &std::path::Path, user_sid: &str) -> io::Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut security = PipeSecurity::owner_only(user_sid)?;
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let created = unsafe { CreateDirectoryW(wide.as_ptr(), security.as_raw().cast()) };
    if created == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32) {
            return Err(error);
        }
    }
    Ok(())
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// SID of the user this process runs as, in `S-1-5-…` string form.
pub fn current_user_sid() -> io::Result<String> {
    let token = open_process_token()?;
    token_user_sid(&token)
}

/// Windows logon session (token authentication LUID) as `high:low`.
pub fn current_logon_session() -> io::Result<String> {
    let token = open_process_token()?;
    token_logon_session(&token)
}

/// Identity of the process on the other end of a connected pipe, as reported
/// by the operating system.
pub fn peer_identity(pipe: RawHandle) -> io::Result<PeerIdentity> {
    let handle = pipe as HANDLE;
    let impersonated = unsafe { ImpersonateNamedPipeClient(handle) };
    if impersonated == 0 {
        return Err(io::Error::last_os_error());
    }

    let identity = (|| {
        let mut token: HANDLE = std::ptr::null_mut();
        let opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) };
        if opened == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = OwnedHandle(token);
        Ok(PeerIdentity {
            user_sid: token_user_sid(&token)?,
            logon_session: token_logon_session(&token)?,
        })
    })();

    // Impersonation must be dropped before anything else runs on this thread.
    if unsafe { RevertToSelf() } == 0 {
        return Err(io::Error::last_os_error());
    }
    identity
}

fn open_process_token() -> io::Result<OwnedHandle> {
    let mut token: HANDLE = std::ptr::null_mut();
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
    if opened == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(OwnedHandle(token))
}

fn token_information(token: &OwnedHandle, class: i32) -> io::Result<Vec<u8>> {
    let mut needed = 0_u32;
    unsafe { GetTokenInformation(token.0, class, std::ptr::null_mut(), 0, &mut needed) };
    if needed == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0_u8; needed as usize];
    let read = unsafe {
        GetTokenInformation(
            token.0,
            class,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    if read == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(buffer)
}

fn token_user_sid(token: &OwnedHandle) -> io::Result<String> {
    let buffer = token_information(token, TokenUser)?;
    if buffer.len() < std::mem::size_of::<TOKEN_USER>() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "token user information is truncated",
        ));
    }
    let user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };

    let mut raw: *mut u16 = std::ptr::null_mut();
    let converted = unsafe { ConvertSidToStringSidW(user.User.Sid, &mut raw) };
    if converted == 0 {
        return Err(io::Error::last_os_error());
    }
    let sid = unsafe { wide_to_string(raw) };
    unsafe { LocalFree(raw as HLOCAL) };
    Ok(sid)
}

fn token_logon_session(token: &OwnedHandle) -> io::Result<String> {
    let buffer = token_information(token, TokenStatistics)?;
    if buffer.len() < std::mem::size_of::<TOKEN_STATISTICS>() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "token statistics are truncated",
        ));
    }
    let statistics = unsafe { &*(buffer.as_ptr() as *const TOKEN_STATISTICS) };
    Ok(format!(
        "{}:{}",
        statistics.AuthenticationId.HighPart, statistics.AuthenticationId.LowPart
    ))
}

unsafe fn wide_to_string(pointer: *const u16) -> String {
    let mut length = 0_usize;
    while unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(pointer, length) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_current_user_and_logon_session() {
        let sid = current_user_sid().expect("current user SID");
        assert!(sid.starts_with("S-1-"), "unexpected SID shape: {sid}");
        let session = current_logon_session().expect("logon session");
        assert!(session.contains(':'), "unexpected logon session: {session}");
    }

    #[test]
    fn builds_an_owner_only_descriptor_and_rejects_a_bad_sid() {
        let sid = current_user_sid().expect("current user SID");
        let mut security = PipeSecurity::owner_only(&sid).expect("descriptor");
        assert!(!security.as_raw().is_null());
        assert!(PipeSecurity::owner_only("not-a-sid").is_err());
    }
}
