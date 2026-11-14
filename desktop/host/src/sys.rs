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

/// Leaves the caller's process tree: forks, the parent exits at once, and
/// the child starts a new session with its standard streams on
/// `/dev/null`. Returns in the child. Call it only while the process has
/// one thread, before any runtime starts.
pub fn daemonize() -> std::io::Result<()> {
    // SAFETY: with one thread, the child of fork may run any code.
    match unsafe { libc::fork() } {
        -1 => return Err(std::io::Error::last_os_error()),
        0 => {}
        // SAFETY: _exit ends the parent without running destructors or
        // flushing buffers that the child also holds.
        _ => unsafe { libc::_exit(0) },
    }
    // SAFETY: setsid has no preconditions; the child of a fork is never a
    // process group leader, so it succeeds.
    if unsafe { libc::setsid() } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    let null = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")?;
    use std::os::fd::AsRawFd;
    for fd in 0..=2 {
        // SAFETY: both descriptors are open; dup2 replaces the standard
        // stream with /dev/null.
        if unsafe { libc::dup2(null.as_raw_fd(), fd) } == -1 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Replaces this process's environment with `vars`. Call it only while
/// the process has one thread.
pub fn replace_env(vars: &[(String, String)]) {
    let names: Vec<_> = std::env::vars_os().map(|(name, _)| name).collect();
    for name in names {
        // SAFETY: the process has one thread, so nothing reads the
        // environment meanwhile.
        unsafe { std::env::remove_var(name) };
    }
    for (name, value) in vars {
        if name.is_empty() || name.contains('=') || name.contains('\0') || value.contains('\0') {
            continue;
        }
        // SAFETY: as above.
        unsafe { std::env::set_var(name, value) };
    }
}

/// Raises the soft limit of open files to the hard limit, at most
/// `most`. launchd starts processes with 256 and systemd with 1,024, and
/// the host holds a descriptor or more for each terminal.
#[cfg(feature = "server")]
pub fn raise_open_files(most: u64) {
    // SAFETY: rlimit is plain data, read and written by the two calls.
    unsafe {
        let mut limit: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 {
            return;
        }
        let wanted = (limit.rlim_max as u64).min(most) as libc::rlim_t;
        if wanted > limit.rlim_cur {
            limit.rlim_cur = wanted;
            libc::setrlimit(libc::RLIMIT_NOFILE, &limit);
        }
    }
}
