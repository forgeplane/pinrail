//! The system calls the host makes through libc.

// libc's functions can only be called with `unsafe`; each call says why it
// is sound.
#![allow(unsafe_code)]

/// The id of the current boot, which changes at every boot: the kernel's
/// `boot_id` on Linux, `kern.bootsessionuuid` on macOS. `None` when the
/// system does not say.
pub fn boot_id() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
        Some(id.trim().to_string()).filter(|id| !id.is_empty())
    }
    #[cfg(target_os = "macos")]
    {
        sysctl_string(c"kern.bootsessionuuid")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn sysctl_string(name: &std::ffi::CStr) -> Option<String> {
    let mut buf = [0u8; 128];
    let mut len = buf.len();
    // SAFETY: the name is NUL-terminated, `buf` holds `len` bytes, and no
    // new value is set.
    let result = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        return None;
    }
    let value = &buf[..len.min(buf.len())];
    let value = value.split(|&b| b == 0).next().unwrap_or_default();
    Some(String::from_utf8_lossy(value).into_owned()).filter(|id| !id.is_empty())
}

/// Whether a process with this id exists.
pub fn alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    // SAFETY: signal 0 sends nothing; it only checks that the process
    // exists. EPERM means it exists under another user.
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// The effective user id of this process.
pub fn uid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}
