//! The process that holds a terminal: its foreground process group's
//! leader, with its name, arguments and folder, read from `/proc` on Linux,
//! from `proc_pidinfo` and `KERN_PROCARGS2` on macOS, and on Windows from
//! the process's command line and, for its folder, its own memory.

// sysctl and proc_pidinfo can only be called with `unsafe`; each call says
// why it is sound.
#![allow(unsafe_code)]

use serde_json::{Value, json};

/// A foreground process group's leader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leader {
    pub pid: u32,
    pub name: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

impl Leader {
    pub fn to_json(&self) -> Value {
        json!({"pid": self.pid, "name": self.name, "args": self.args, "cwd": self.cwd})
    }
}

/// The name of the process `pid`, if it runs: one call, where its
/// arguments and folder take three more.
pub fn name(pid: u32) -> Option<String> {
    Some(pinrail_runtime::process::process(pid)?.name)
}

/// The leader `pid` named `name`, with its arguments and folder.
pub fn with_details(pid: u32, name: String) -> Leader {
    Leader {
        pid,
        name,
        args: args(pid).unwrap_or_default(),
        cwd: cwd(pid),
    }
}

#[cfg(target_os = "linux")]
fn args(pid: u32) -> Option<Vec<String>> {
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(
        bytes
            .split(|&b| b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect(),
    )
}

#[cfg(target_os = "linux")]
fn cwd(pid: u32) -> Option<String> {
    let path = std::fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
    Some(path.to_string_lossy().into_owned())
}

/// `KERN_PROCARGS2`: the argument count, the executable's path, padding,
/// then the arguments, each NUL-terminated, then the environment.
#[cfg(target_os = "macos")]
fn args(pid: u32) -> Option<Vec<String>> {
    let mut size: libc::size_t = 0;
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
    // SAFETY: a null buffer asks for the size only.
    let asked = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if asked != 0 || size < 4 {
        return None;
    }
    let mut buf = vec![0u8; size];
    // SAFETY: `buf` holds `size` bytes, which the call writes at most.
    let read = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buf.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if read != 0 {
        return None;
    }
    parse_procargs(&buf[..size.min(buf.len())])
}

#[cfg(any(target_os = "macos", test))]
fn parse_procargs(buf: &[u8]) -> Option<Vec<String>> {
    let argc = i32::from_ne_bytes(buf.get(..4)?.try_into().ok()?);
    let rest = &buf[4..];
    // the executable's path, then NUL padding up to the first argument
    let path_end = rest.iter().position(|&b| b == 0)?;
    let start = path_end + rest[path_end..].iter().position(|&b| b != 0)?;
    Some(
        rest[start..]
            .split(|&b| b == 0)
            .take(usize::try_from(argc).ok()?)
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect(),
    )
}

#[cfg(target_os = "macos")]
fn cwd(pid: u32) -> Option<String> {
    // SAFETY: proc_vnodepathinfo is plain data, written by proc_pidinfo.
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
    // SAFETY: `info` is a valid place of `size` bytes for the flavor's
    // struct.
    let written = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            (&raw mut info).cast(),
            size,
        )
    };
    if written != size {
        return None;
    }
    let path = &info.pvi_cdir.vip_path;
    // SAFETY: vip_path is a fixed array of c_char, read as bytes.
    let bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(path.as_ptr().cast::<u8>(), std::mem::size_of_val(path))
    };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    (end > 0).then(|| String::from_utf8_lossy(&bytes[..end]).into_owned())
}

#[cfg(windows)]
fn args(pid: u32) -> Option<Vec<String>> {
    pinrail_runtime::process::arguments(pid)
}

/// The folder Windows keeps in the process's parameters, which only the
/// process's memory has: the PEB points to them. A 32-bit process lays
/// them out otherwise, and is left out.
#[cfg(windows)]
fn cwd(pid: u32) -> Option<String> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle, RawHandle};
    use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Threading::{
        IsWow64Process, OpenProcess, PROCESS_BASIC_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_VM_READ,
    };
    // where the 64-bit layouts keep what is read: the parameters in the
    // PEB, and the folder, as a UNICODE_STRING, in the parameters
    const PARAMETERS_IN_PEB: usize = 0x20;
    const FOLDER_IN_PARAMETERS: usize = 0x38;
    // SAFETY: the handle is owned below; each read is of the size of the
    // buffer it fills, from the other process's memory.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle.is_null() {
            return None;
        }
        let process = OwnedHandle::from_raw_handle(handle as RawHandle);
        let h = crate::win::raw(&process);
        let mut wow64 = 0;
        if IsWow64Process(h, &mut wow64) == 0 || wow64 != 0 {
            return None;
        }
        let mut basic: PROCESS_BASIC_INFORMATION = std::mem::zeroed();
        if NtQueryInformationProcess(
            h,
            ProcessBasicInformation,
            (&raw mut basic).cast(),
            size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            std::ptr::null_mut(),
        ) != 0
        {
            return None;
        }
        let read = |at: usize, into: &mut [u8]| {
            ReadProcessMemory(
                h,
                at as *const _,
                into.as_mut_ptr().cast(),
                into.len(),
                std::ptr::null_mut(),
            ) != 0
        };
        let mut word = [0u8; 8];
        if !read(basic.PebBaseAddress as usize + PARAMETERS_IN_PEB, &mut word) {
            return None;
        }
        let parameters = u64::from_le_bytes(word) as usize;
        // Length u16, MaximumLength u16, padding, Buffer u64
        let mut string = [0u8; 16];
        if !read(parameters + FOLDER_IN_PARAMETERS, &mut string) {
            return None;
        }
        let len = usize::from(u16::from_le_bytes([string[0], string[1]]));
        let buffer = u64::from_le_bytes(string[8..16].try_into().ok()?) as usize;
        if len == 0 || len > 32_768 || buffer == 0 {
            return None;
        }
        let mut bytes = vec![0u8; len];
        if !read(buffer, &mut bytes) {
            return None;
        }
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        let folder = String::from_utf16_lossy(&units);
        // kept with a trailing separator, but for a drive's root
        let trimmed = folder.trim_end_matches('\\');
        Some(if trimmed.ends_with(':') {
            folder
        } else {
            trimmed.to_string()
        })
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn args(_: u32) -> Option<Vec<String>> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn cwd(_: u32) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procargs_give_the_arguments_after_the_path_and_its_padding() {
        let mut buf = 2i32.to_ne_bytes().to_vec();
        buf.extend(b"/usr/bin/vim\0\0\0\0vim\0notes.md\0HOME=/Users/p\0");
        assert_eq!(
            parse_procargs(&buf).unwrap(),
            ["vim".to_string(), "notes.md".to_string()]
        );
    }

    #[test]
    fn this_process_reads_as_itself() {
        let pid = std::process::id();
        let me = with_details(pid, name(pid).unwrap());
        assert!(!me.name.is_empty());
        assert!(!me.args.is_empty());
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
        assert_eq!(
            std::path::Path::new(&me.cwd.unwrap())
                .canonicalize()
                .unwrap(),
            cwd
        );
    }
}
