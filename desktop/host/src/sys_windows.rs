//! The system calls the host makes on Windows, under the names `sys.rs`
//! gives them on Unix.

// The Windows API can only be called with `unsafe`; each call says why it
// is sound.
#![allow(unsafe_code)]

use std::os::windows::io::{FromRawHandle, OwnedHandle, RawHandle};

use windows_sys::Win32::Foundation::{STILL_ACTIVE, WAIT_TIMEOUT};
use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    WaitForSingleObject,
};

use crate::win::{raw, wide};

/// The id of the current boot: Windows counts its boots in the registry's
/// `BootId`. Where that is missing, the time the system started, to the
/// minute, from how long it has run.
pub fn boot_id() -> Option<String> {
    let key = wide(
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management\PrefetchParameters"
            .as_ref(),
    );
    let name = wide("BootId".as_ref());
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: both names are NUL-terminated; `value` holds the DWORD asked
    // for, `size` its size.
    let found = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut value).cast(),
            &mut size,
        )
    };
    if found == 0 {
        return Some(format!("boot-{value}"));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    // SAFETY: no preconditions.
    let up = unsafe { GetTickCount64() };
    Some(format!("started-{}", (now.saturating_sub(up)) / 60_000))
}

/// Whether a process with this id runs. One of another user, which cannot
/// be opened, is taken to run.
pub fn alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    // SAFETY: the handle is owned below.
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return std::io::Error::last_os_error().kind() == std::io::ErrorKind::PermissionDenied;
    }
    // SAFETY: `handle` is open and owned here.
    let handle = unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) };
    // SAFETY: waits on an open process handle without blocking.
    if unsafe { WaitForSingleObject(raw(&handle), 0) } == WAIT_TIMEOUT {
        return true;
    }
    let mut code = 0u32;
    // SAFETY: writes the exit code into `code`.
    unsafe { GetExitCodeProcess(raw(&handle), &mut code) != 0 && code == STILL_ACTIVE as u32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boot_id_stays_the_same_within_a_boot() {
        let id = boot_id().unwrap();
        assert!(!id.is_empty());
        assert_eq!(boot_id().unwrap(), id);
    }

    #[test]
    fn this_process_is_alive_and_an_ended_one_is_not() {
        assert!(alive(std::process::id()));
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "exit 0"])
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        drop(child);
        assert!(!alive(pid));
        assert!(!alive(0));
    }
}
