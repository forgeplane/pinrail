//! The process that holds a terminal: its foreground process group's
//! leader, with its name, arguments and folder, read from `/proc` on Linux
//! and from `proc_pidinfo` and `KERN_PROCARGS2` on macOS.

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

/// The leader `pid`, if it runs.
pub fn read(pid: u32) -> Option<Leader> {
    let name = pinrail_runtime::process::process(pid)?.name;
    Some(Leader {
        pid,
        name,
        args: args(pid).unwrap_or_default(),
        cwd: cwd(pid),
    })
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

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn args(_: u32) -> Option<Vec<String>> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
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
        let me = read(std::process::id()).unwrap();
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
