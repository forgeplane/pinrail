//! What the host's Windows code shares: wide strings, errors, events, and
//! the user a process runs as.

// The Windows API can only be called with `unsafe`; each call says why it
// is sound.
#![allow(unsafe_code)]

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{
    GetLengthSid, GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_ELEVATION, TOKEN_QUERY,
    TOKEN_USER, TokenElevation, TokenUser,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::core::BOOL;

/// `s` as a NUL-terminated UTF-16 string.
pub fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

/// The last error when `ok` is false, as the API reports failure.
pub fn check(ok: BOOL) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// A new event, manual-reset, not signalled.
pub fn event() -> io::Result<OwnedHandle> {
    // SAFETY: no security attributes or name; the handle is new and owned
    // by the result.
    let handle = unsafe { CreateEventW(null(), 1, 0, null()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: as above.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) })
}

pub fn raw(handle: &impl AsRawHandle) -> HANDLE {
    handle.as_raw_handle() as HANDLE
}

/// The user a token belongs to, as the bytes of its SID.
fn token_user(token: HANDLE) -> io::Result<Vec<u8>> {
    let mut needed = 0u32;
    // SAFETY: a null buffer asks only for the size.
    unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed) };
    let mut buffer = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
    // SAFETY: the buffer holds `needed` bytes, aligned for TOKEN_USER.
    check(unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    })?;
    // SAFETY: GetTokenInformation wrote a TOKEN_USER whose SID points into
    // the buffer, which lives until the end of this function.
    unsafe {
        let sid = (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        let len = GetLengthSid(sid) as usize;
        Ok(std::slice::from_raw_parts(sid.cast::<u8>(), len).to_vec())
    }
}

/// The user this process runs as, as the bytes of its SID.
pub fn current_user() -> io::Result<Vec<u8>> {
    static USER: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    if let Some(user) = USER.get() {
        return Ok(user.clone());
    }
    let mut token = null_mut();
    // SAFETY: the pseudo handle of this process needs no closing; the
    // token is closed below.
    check(unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) })?;
    let user = token_user(token);
    // SAFETY: the token was opened above.
    unsafe { CloseHandle(token) };
    Ok(USER.get_or_init(|| user.unwrap_or_default()).clone())
}

/// The user the process `pid` runs as, as the bytes of its SID. A process
/// of another user cannot be opened, which is an error too.
pub fn user_of(pid: u32) -> io::Result<Vec<u8>> {
    // SAFETY: the handle is closed below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `process` is open; it is owned here.
    let process = unsafe { OwnedHandle::from_raw_handle(process as RawHandle) };
    let mut token = null_mut();
    // SAFETY: the token is closed below.
    check(unsafe { OpenProcessToken(raw(&process), TOKEN_QUERY, &mut token) })?;
    let user = token_user(token);
    // SAFETY: the token was opened above.
    unsafe { CloseHandle(token) };
    user
}

/// A SID's text, `S-1-5-21-…`.
pub fn sid_text(sid: &[u8]) -> String {
    let mut text = null_mut();
    // SAFETY: `sid` holds a whole SID; the string is freed below.
    if unsafe { ConvertSidToStringSidW(sid.as_ptr() as *mut _, &mut text) } == 0 {
        return String::new();
    }
    // SAFETY: ConvertSidToStringSidW wrote a NUL-terminated string.
    unsafe {
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        let value = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        value
    }
}

/// Whether this process runs elevated, as an administrator.
pub fn elevated() -> bool {
    let mut token = null_mut();
    // SAFETY: the token is closed below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return false;
    }
    // SAFETY: TOKEN_ELEVATION is plain data that the call fills.
    let mut elevation: TOKEN_ELEVATION = unsafe { std::mem::zeroed() };
    let mut len = 0u32;
    // SAFETY: as above, within its size.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&raw mut elevation).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    };
    // SAFETY: the token was opened above.
    unsafe { CloseHandle(token) };
    ok != 0 && elevation.TokenIsElevated != 0
}

/// Who may open the host's pipes: this user and the system. The
/// integrity label keeps processes of a lower level out, so that an
/// unelevated program cannot drive an elevated host. Adapted from
/// herdr-core's `pipe_security` (crates/herdr-core/src/platform/windows.rs).
pub fn pipe_security() -> io::Result<*mut SECURITY_ATTRIBUTES> {
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    static MADE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    if let Some(sa) = MADE.get() {
        return Ok(*sa as *mut SECURITY_ATTRIBUTES);
    }
    let user = sid_text(&current_user()?);
    if user.is_empty() {
        return Err(io::Error::other("this user's SID cannot be read"));
    }
    let level = if elevated() { "HI" } else { "ME" };
    let sddl = wide(format!("D:P(A;;GA;;;SY)(A;;GA;;;{user})S:(ML;;NRNW;;;{level})").as_ref());
    let mut descriptor = null_mut();
    // SAFETY: the SDDL is NUL-terminated; the descriptor is kept for
    // the life of the process.
    check(unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    })?;
    let attributes = Box::leak(Box::new(SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    }));
    Ok(*MADE.get_or_init(|| attributes as *mut _ as usize) as *mut SECURITY_ATTRIBUTES)
}
