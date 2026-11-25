//! Windows named-pipe access control (audit F2 / Phase 10 F2).
//!
//! The agent's pipe (`\\.\pipe\reponyx-agent`) used to be created
//! with the default security descriptor: any local user could
//! connect, and only the IPC shared-secret check stood between them
//! and the agent. Pipes created here carry a DACL that grants access
//! only to the current user and to SYSTEM — every other local user
//! gets Access Denied at connect time, before any protocol bytes
//! flow.
//!
//! The descriptor is built with the raw Win32 security APIs and
//! handed to tokio's
//! `ServerOptions::create_with_security_attributes_raw`, so the pipe
//! is restricted from the instant it exists: there is no
//! create-then-restrict window in which another user could connect.
//!
//! If the restricted descriptor cannot be built, pipe creation
//! fails loudly instead of silently falling back to a
//! world-accessible pipe — a fail-open here would reintroduce F2
//! without anyone noticing.
//!
//! This file calls the raw Win32 security APIs, which have no safe
//! Rust wrapper — every call is an `unsafe` block, each marked
//! `// nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage`
//! (the repo's Semgrep gate flags every `unsafe` block; the
//! suppressions are per-call, never file-wide, following the
//! convention in `apps/desktop/src-tauri/src/exe.rs`). The calls
//! are audited inline: every pointer is either stack-owned, a
//! buffer this module owns and keeps alive, or freed in the same
//! function; every return value is checked before use.

use std::ffi::c_void;
use std::io;
use std::path::Path;
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_ALL, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GRANT_ACCESS, SetEntriesInAclW, TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    AllocateAndInitializeSid, FreeSid, GetTokenInformation, InitializeSecurityDescriptor,
    NO_INHERITANCE, PSID, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR, SECURITY_NT_AUTHORITY,
    SetSecurityDescriptorDacl, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::core::PWSTR;

/// Win32 constants windows-sys does not export: the security
/// descriptor revision (always 1) and the SYSTEM account's
/// relative id (S-1-5-18).
const SECURITY_DESCRIPTOR_REVISION: u32 = 1;
const SECURITY_LOCAL_SYSTEM_RID: u32 = 18;

/// The current user's SID, from the process token. Returns the
/// owning buffer plus a SID pointer into it; the buffer must outlive
/// every use of the pointer (moving the `Vec` is fine — its heap
/// allocation does not move).
fn current_user_sid() -> io::Result<(Vec<u8>, PSID)> {
    let mut token_handle = std::ptr::null_mut();
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle) };
    if opened == 0 {
        return Err(io::Error::last_os_error());
    }
    let result = current_user_sid_from_token(token_handle);
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        CloseHandle(token_handle);
    }
    result
}

fn current_user_sid_from_token(
    token_handle: windows_sys::Win32::Foundation::HANDLE,
) -> io::Result<(Vec<u8>, PSID)> {
    let mut needed = 0u32;
    // First call: learn the buffer size (fails with
    // ERROR_INSUFFICIENT_BUFFER, which is expected).
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        GetTokenInformation(
            token_handle,
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    let mut token_info = vec![0u8; needed as usize];
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let ok = unsafe {
        GetTokenInformation(
            token_handle,
            TokenUser,
            token_info.as_mut_ptr() as *mut c_void,
            needed,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the buffer holds a TOKEN_USER whose `User.Sid` is the
    // process user's SID; the buffer is returned alongside the
    // pointer so it stays alive.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let token_user = unsafe { &*(token_info.as_ptr() as *const TOKEN_USER) };
    Ok((token_info, token_user.User.Sid))
}

/// The SYSTEM SID (S-1-5-18). The caller frees it with `FreeSid`.
fn system_sid() -> io::Result<PSID> {
    let mut sid = std::ptr::null_mut();
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let ok = unsafe {
        AllocateAndInitializeSid(
            &SECURITY_NT_AUTHORITY,
            1,
            SECURITY_LOCAL_SYSTEM_RID,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            &mut sid,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(sid)
}

fn explicit_access(sid: PSID) -> EXPLICIT_ACCESS_W {
    EXPLICIT_ACCESS_W {
        grfAccessPermissions: GENERIC_ALL,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: 0,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            // The SID is the trustee name when TrusteeForm is
            // TRUSTEE_IS_SID.
            ptstrName: sid as PWSTR,
        },
    }
}

/// A security descriptor + ACL that grants the pipe to the current
/// user and SYSTEM only. Owns every native allocation; freed on
/// drop. Only needs to live through the `CreateNamedPipe` call —
/// the kernel copies the descriptor into the pipe object.
struct RestrictedAttributes {
    descriptor: SECURITY_DESCRIPTOR,
    acl: *mut windows_sys::Win32::Security::ACL,
    /// Owns the `TOKEN_USER` buffer; the user SID points into it.
    /// Never read directly — its job is keeping the buffer alive
    /// until the pipe is created (hence the underscore name).
    _token_info: Vec<u8>,
}

impl RestrictedAttributes {
    fn build() -> io::Result<Self> {
        let (token_info, user_sid) = current_user_sid()?;
        let sys_sid = system_sid()?;
        // One GRANT entry per SID; NO_INHERITANCE keeps the ACEs off
        // child objects. The entries are copied into the new ACL by
        // SetEntriesInAclW, so the SID buffers may be freed after.
        let entries = [explicit_access(user_sid), explicit_access(sys_sid)];
        let mut acl = std::ptr::null_mut();
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let status = unsafe {
            SetEntriesInAclW(
                entries.len() as u32,
                entries.as_ptr(),
                std::ptr::null(),
                &mut acl,
            )
        };
        if status != 0 {
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            unsafe {
                FreeSid(sys_sid);
            }
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let result = (|| {
            // The descriptor itself: absolute form, DACL present.
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            let mut descriptor: SECURITY_DESCRIPTOR = unsafe { std::mem::zeroed() };
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            let ok = unsafe {
                InitializeSecurityDescriptor(
                    &mut descriptor as *mut _ as *mut c_void,
                    SECURITY_DESCRIPTOR_REVISION,
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            let ok = unsafe {
                SetSecurityDescriptorDacl(&mut descriptor as *mut _ as *mut c_void, 1, acl, 0)
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(descriptor)
        })();
        // The SIDs were copied into the ACL; free the originals
        // whether the descriptor build succeeded or not.
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe {
            FreeSid(sys_sid);
        }
        let descriptor = match result {
            Ok(descriptor) => descriptor,
            Err(err) => {
                // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
                unsafe {
                    LocalFree(acl as *mut c_void);
                }
                return Err(err);
            }
        };
        Ok(Self {
            descriptor,
            acl,
            _token_info: token_info,
        })
    }

    fn security_attributes(&mut self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: &mut self.descriptor as *mut _ as *mut c_void,
            bInheritHandle: 0,
        }
    }
}

impl Drop for RestrictedAttributes {
    fn drop(&mut self) {
        if !self.acl.is_null() {
            // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
            unsafe {
                LocalFree(self.acl as *mut c_void);
            }
        }
    }
}

/// Create a named-pipe server whose DACL grants access only to the
/// current user and SYSTEM (audit F2). Mirrors
/// `ServerOptions::create` (byte mode, overlapped, unlimited
/// instances) with the restricted descriptor attached; `first`
/// selects `FILE_FLAG_FIRST_PIPE_INSTANCE` for the agent's initial
/// instance.
pub fn create_restricted_pipe(path: &Path, first: bool) -> io::Result<NamedPipeServer> {
    let mut attrs = RestrictedAttributes::build()?;
    let mut security_attributes = attrs.security_attributes();
    let mut options = ServerOptions::new();
    let options = options.first_pipe_instance(first);
    // SAFETY: `security_attributes` points at a valid
    // SECURITY_ATTRIBUTES whose descriptor and ACL outlive this call
    // (they are owned by `attrs`, dropped only after `create`
    // returns); tokio copies the descriptor into the pipe object.
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        options.create_with_security_attributes_raw(
            path,
            &mut security_attributes as *mut _ as *mut c_void,
        )
    }
}

/// Whether a pipe's DACL grants access to exactly the current user
/// and SYSTEM — two ACCESS_ALLOWED ACEs, one per trustee. Used by
/// the cross-user test: a pipe whose DACL contains any other
/// trustee, a duplicate trustee, or a missing DACL fails the check.
#[cfg(test)]
pub fn pipe_dacl_is_user_and_system_only(server: &NamedPipeServer) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT};
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACL_SIZE_INFORMATION, AclSizeInformation, DACL_SECURITY_INFORMATION,
        EqualSid, GetAce, GetAclInformation,
    };

    let handle = server.as_raw_handle();
    let mut dacl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    let status = unsafe {
        GetSecurityInfo(
            handle,
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    // The descriptor returned by GetSecurityInfo is freed with
    // LocalFree; the DACL pointer points into it.
    let outcome = (|| {
        if dacl.is_null() {
            return Ok(false);
        }
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let mut info: ACL_SIZE_INFORMATION = unsafe { std::mem::zeroed() };
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        let ok = unsafe {
            GetAclInformation(
                dacl,
                &mut info as *mut _ as *mut c_void,
                std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        if info.AceCount != 2 {
            return Ok(false);
        }
        let (_user_buf, user_sid) = current_user_sid()?;
        let sys_sid = system_sid()?;
        let result = (|| {
            let mut saw_user = false;
            let mut saw_system = false;
            for index in 0..info.AceCount {
                let mut ace = std::ptr::null_mut();
                // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
                let ok = unsafe { GetAce(dacl, index, &mut ace) };
                if ok == 0 || ace.is_null() {
                    return Err(io::Error::last_os_error());
                }
                // SAFETY: an ACCESS_ALLOWED_ACE is an ACE_HEADER
                // followed by the access mask and the SID bytes.
                // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
                let ace = unsafe { &*(ace as *const ACCESS_ALLOWED_ACE) };
                if ace.Header.AceType != 0 {
                    // 0 == ACCESS_ALLOWED_ACE_TYPE.
                    return Ok(false);
                }
                let sid = &ace.SidStart as *const u32 as PSID;
                // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
                if unsafe { EqualSid(sid, user_sid) } != 0 {
                    saw_user = true;
                // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
                } else if unsafe { EqualSid(sid, sys_sid) } != 0 {
                    saw_system = true;
                } else {
                    return Ok(false);
                }
            }
            Ok(saw_user && saw_system)
        })();
        // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
        unsafe {
            FreeSid(sys_sid);
        }
        result
    })();
    // nosemgrep: rust.lang.security.unsafe-usage.unsafe-usage
    unsafe {
        LocalFree(descriptor);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricted_pipe_dacl_is_user_and_system_only() {
        // A unique pipe name per run, so parallel tests never share
        // one.
        let name = format!(r"\\.\pipe\reponyx-test-acl-{}", std::process::id());
        let path = Path::new(&name);
        let server = create_restricted_pipe(path, true).expect("restricted pipe creation succeeds");
        assert!(
            pipe_dacl_is_user_and_system_only(&server).expect("reading the pipe DACL succeeds"),
            "pipe DACL must grant access only to the current user and SYSTEM"
        );
    }
}
