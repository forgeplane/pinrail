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
    // an error is not kept: the next call asks again
    let user = user?;
    if user.is_empty() {
        return Err(io::Error::other("this process's user has no SID"));
    }
    Ok(USER.get_or_init(|| user).clone())
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
    use windows_sys::Win32::Security::{GetLengthSid, IsValidSid};
    // the bytes must hold a whole SID before Windows reads one from them
    // SAFETY: IsValidSid and GetLengthSid read within the SID's header,
    // which a slice of 8 bytes or more holds.
    if sid.len() < 8
        || unsafe { IsValidSid(sid.as_ptr() as *mut _) } == 0
        || (unsafe { GetLengthSid(sid.as_ptr() as *mut _) } as usize) > sid.len()
    {
        return String::new();
    }
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

/// Makes `path`, a folder, readable and writable by this user and the
/// system only, and everything made in it after: a protected ACL that
/// files and folders inside inherit, as mode 0700 is on Unix.
///
/// A folder that is so already is left alone, as Unix leaves a closed
/// folder: setting its ACL again would rewrite the inherited entries of
/// everything inside it, on every start: some 3 seconds for 20,000 files.
pub fn make_private(path: &std::path::Path) -> io::Result<()> {
    let user = sid_text(&current_user()?);
    if user.is_empty() {
        return Err(io::Error::other("this user's SID cannot be read"));
    }
    if acl_of(path).is_ok_and(|acl| is_private(&acl, &user)) {
        return Ok(());
    }
    set_private(path, &user)
}

/// Whether `acl`, a folder's DACL in SDDL, is what [`make_private`] sets
/// for `user`: protected, and granting all to the system and `user` only,
/// for the folder and all inside it.
fn is_private(acl: &str, user: &str) -> bool {
    let Some(rest) = acl.strip_prefix("D:") else {
        return false;
    };
    let (flags, aces) = rest.split_at(rest.find('(').unwrap_or(rest.len()));
    if !flags.contains('P') {
        return false;
    }
    let mut granted: Vec<&str> = aces
        .split(')')
        .filter(|a| !a.is_empty())
        .map(|a| a.strip_prefix('('))
        .map(|a| a.and_then(|a| a.strip_prefix("A;OICI;FA;;;")).unwrap_or(""))
        .collect();
    // SDDL names the computer's own Administrator, RID 500, as LA
    let user = if user.ends_with("-500") && granted.contains(&"LA") {
        "LA"
    } else {
        user
    };
    granted.sort_unstable();
    let mut wanted = vec!["SY", user];
    wanted.sort_unstable();
    granted == wanted
}

/// Sets the ACL of [`make_private`] on `path`.
fn set_private(path: &std::path::Path, user: &str) -> io::Result<()> {
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
        SetNamedSecurityInfoW,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
    };
    let sddl = wide(format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;{user})").as_ref());
    let mut descriptor = null_mut();
    // SAFETY: the SDDL is NUL-terminated; the descriptor is freed below.
    check(unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    })?;
    let (mut present, mut dacl, mut defaulted) = (0, null_mut(), 0);
    // SAFETY: reads the DACL out of the descriptor made above.
    let found = check(unsafe {
        GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted)
    });
    let name = wide(path.as_os_str());
    let result = found.and_then(|()| {
        // SAFETY: the name is NUL-terminated; the DACL lives in the
        // descriptor until it is freed below.
        let error = unsafe {
            SetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                dacl,
                null_mut(),
            )
        };
        if error == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(error as i32))
        }
    });
    // SAFETY: the descriptor was allocated with LocalAlloc.
    unsafe { LocalFree(descriptor) };
    result
}

/// The access control list of `path` in SDDL.
pub fn acl_of(path: &std::path::Path) -> io::Result<String> {
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
    let name = wide(path.as_os_str());
    let mut descriptor = null_mut();
    // SAFETY: the name is NUL-terminated; the descriptor is freed below.
    let error = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    if error != 0 {
        return Err(io::Error::from_raw_os_error(error as i32));
    }
    let mut text = null_mut();
    // SAFETY: converts the descriptor read above; the text is freed below.
    let converted = check(unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            null_mut(),
        )
    });
    let result = converted.map(|()| {
        // SAFETY: the text is NUL-terminated.
        unsafe {
            let len = (0..).take_while(|&i| *text.add(i) != 0).count();
            String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
        }
    });
    // SAFETY: both were allocated with LocalAlloc.
    unsafe {
        LocalFree(text.cast());
        LocalFree(descriptor);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "S-1-5-21-1-2-3-1001";

    #[test]
    fn a_folder_is_private_with_the_two_grants_only_and_protected() {
        let private = format!("D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;{USER})");
        assert!(is_private(&private, USER));
        // in either order, as Windows writes them in its own
        assert!(is_private(
            &format!("D:P(A;OICI;FA;;;{USER})(A;OICI;FA;;;SY)"),
            USER
        ));
        // the computer's own Administrator, whom SDDL names LA
        assert!(is_private(
            "D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;LA)",
            "S-1-5-21-1-2-3-500"
        ));
        for open in [
            // inherited from the folder above, which can change
            format!("D:AI(A;OICIID;FA;;;SY)(A;OICIID;FA;;;{USER})"),
            // one more
            format!("D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;{USER})(A;OICI;0x1200a9;;;WD)"),
            // another user's
            "D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;S-1-5-21-1-2-3-1002)".to_string(),
            // not for what is made inside
            format!("D:PAI(A;;FA;;;SY)(A;;FA;;;{USER})"),
            // a denial
            format!("D:PAI(D;OICI;FA;;;SY)(A;OICI;FA;;;{USER})"),
            // the computer's Administrator, who is not this user
            "D:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;LA)".to_string(),
            // none at all: everyone may
            "D:NO_ACCESS_CONTROL".to_string(),
            String::new(),
        ] {
            assert!(!is_private(&open, USER), "{open}");
        }
    }

    #[test]
    fn an_already_private_folder_is_left_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        make_private(dir.path()).unwrap();
        let user = sid_text(&current_user().unwrap());
        let acl = acl_of(dir.path()).unwrap();
        assert!(is_private(&acl, &user), "{acl}");
        // a file inside that inherits nothing, written without passing
        // the folder's on: setting the folder's ACL again would
        let file = dir.path().join("file");
        std::fs::write(&file, "").unwrap();
        set_without_inheriting(&file, &format!("D:(A;;FA;;;{user})"));
        let before = acl_of(&file).unwrap();
        assert!(!before.contains("ID;"), "{before}");
        make_private(dir.path()).unwrap();
        assert_eq!(acl_of(&file).unwrap(), before);
    }

    /// Sets the DACL of `path` to `sddl` alone, as SetFileSecurityW does:
    /// it passes on nothing and takes nothing from the folder.
    fn set_without_inheriting(path: &std::path::Path, sddl: &str) {
        use windows_sys::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, SetFileSecurityW};
        let sddl = wide(sddl.as_ref());
        let mut descriptor = null_mut();
        // SAFETY: the SDDL is NUL-terminated; the descriptor is freed below.
        check(unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        })
        .unwrap();
        let name = wide(path.as_os_str());
        // SAFETY: the name is NUL-terminated and the descriptor valid.
        let set = check(unsafe {
            SetFileSecurityW(name.as_ptr(), DACL_SECURITY_INFORMATION, descriptor)
        });
        // SAFETY: allocated with LocalAlloc.
        unsafe { LocalFree(descriptor) };
        set.unwrap();
    }
}
