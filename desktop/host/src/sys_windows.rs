//! The system calls the host makes on Windows, under the names `sys.rs`
//! gives them on Unix.

// The Windows API can only be called with `unsafe`; each call says why it
// is sound.
#![allow(unsafe_code)]

use std::os::windows::io::{FromRawHandle, OwnedHandle, RawHandle};

use windows_sys::Win32::Foundation::{STILL_ACTIVE, WAIT_OBJECT_0, WAIT_TIMEOUT};
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
    // signalled once it has ended, whatever its exit code, 259 included,
    // which is the code a running process reports
    // SAFETY: waits on an open process handle without blocking.
    match unsafe { WaitForSingleObject(raw(&handle), 0) } {
        WAIT_TIMEOUT => true,
        WAIT_OBJECT_0 => false,
        _ => {
            let mut code = 0u32;
            // SAFETY: writes the exit code into `code`.
            unsafe {
                GetExitCodeProcess(raw(&handle), &mut code) != 0 && code == STILL_ACTIVE as u32
            }
        }
    }
}

/// Keeps this process's standard streams from the processes it starts:
/// Windows lets a child inherit every inheritable handle, and a host or a
/// holder that kept the pipes of whatever started it open would keep that
/// waiting for its output to end.
pub fn keep_own_output_from_children() {
    use windows_sys::Win32::Foundation::{
        HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation,
    };
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: changes only whether a handle of this process is inherited.
        unsafe {
            let handle = GetStdHandle(which);
            if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}

/// Starts `command` detached from this process: without a console, in a
/// process group of its own, and outside this process's job when the job
/// lets it out, so that it outlives an app that ends its job when it quits.
/// Its standard input and errors go nowhere, and it gets none of this
/// process's standard streams: what it inherited it would keep open for as
/// long as it runs, and whatever reads this process's output would wait
/// for it.
pub fn spawn_detached(mut command: std::process::Command) -> std::io::Result<std::process::Child> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::{
        CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS,
    };
    keep_own_output_from_children();
    command
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
    match command.spawn() {
        // a job that does not let its processes out
        Err(error) if error.raw_os_error() == Some(5) => {
            command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
            command.spawn()
        }
        spawned => spawned,
    }
}

/// The executable of the process `pid`, or `None` when it cannot be read.
/// The core needs it as much as the host, to keep the host's copies that
/// still run.
pub fn executable_of(pid: u32) -> Option<std::path::PathBuf> {
    use windows_sys::Win32::System::Threading::{PROCESS_NAME_WIN32, QueryFullProcessImageNameW};
    // SAFETY: the handle is owned below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    // SAFETY: `handle` is open and owned here.
    let handle = unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) };
    let mut path = vec![0u16; 32_768];
    let mut len = path.len() as u32;
    // SAFETY: the buffer holds `len` characters.
    if unsafe {
        QueryFullProcessImageNameW(
            raw(&handle),
            PROCESS_NAME_WIN32,
            path.as_mut_ptr(),
            &mut len,
        )
    } == 0
    {
        return None;
    }
    use std::os::windows::ffi::OsStringExt;
    Some(std::ffi::OsString::from_wide(&path[..len as usize]).into())
}

/// The executables of the running processes of this user. An error means
/// the list may be incomplete. A process that exits while the list is made
/// is left out.
pub fn running_executables() -> std::io::Result<Vec<std::path::PathBuf>> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    let user = crate::win::current_user()?;
    // SAFETY: takes no pointers; the handle is owned below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: as above.
    let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot as RawHandle) };
    // SAFETY: PROCESSENTRY32W is plain data, its size set below.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut pids = Vec::new();
    // SAFETY: `entry` is a PROCESSENTRY32W with its size set.
    let mut more = unsafe { Process32FirstW(raw(&snapshot), &mut entry) } != 0;
    while more {
        pids.push(entry.th32ProcessID);
        // SAFETY: as above.
        more = unsafe { Process32NextW(raw(&snapshot), &mut entry) } != 0;
    }
    Ok(pids
        .into_iter()
        .filter(|&pid| crate::win::user_of(pid).is_ok_and(|theirs| theirs == user))
        .filter_map(executable_of)
        // as the folders they are compared with are
        .map(|exe| exe.canonicalize().unwrap_or(exe))
        .collect())
}

/// Windows has no limit of open handles to raise: the host keeps as many
/// terminals as the limit for a thousand descriptors allows elsewhere.
#[cfg(feature = "server")]
pub fn raise_open_files(_most: u64) -> u64 {
    8_192
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

    /// As the core builds the crate, without the host's server: it is the
    /// core that decides which of the host's copies are still in use.
    #[test]
    fn the_running_programs_include_this_one() {
        let this = std::env::current_exe().unwrap().canonicalize().unwrap();
        let running = running_executables().unwrap();
        assert!(
            running.contains(&this),
            "{} not in {running:?}",
            this.display()
        );
    }

    /// A program that ended with 259, which is what Windows says of one
    /// still running, while a handle keeps it known.
    #[test]
    fn a_program_that_ended_with_still_active_s_code_is_not_alive() {
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "exit 259"])
            .spawn()
            .unwrap();
        child.wait().unwrap();
        assert!(!alive(child.id()));
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
