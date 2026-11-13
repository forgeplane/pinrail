//! Starting a program as the direct child of a new PTY, and observing its
//! exit without reaping it, so that its process group id stays its own
//! until the terminal is forgotten.

// openpty, ioctl, waitid and the step between fork and exec can only be
// called with `unsafe`; each call says why it is sound.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::environment::find_program;

/// A started program.
#[derive(Debug)]
pub struct Spawned {
    pub pid: u32,
    /// Its start time, as `pinrail-runtime` reads it for every comparison.
    pub started: Option<String>,
    /// The PTY's master side, non-blocking.
    pub master: OwnedFd,
}

#[derive(Debug)]
pub enum SpawnError {
    /// The folder does not exist or is not a folder.
    FolderMissing,
    /// The program is not on the environment's `PATH`.
    NotFound(String),
    Io(io::Error),
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnError::FolderMissing => write!(f, "the folder does not exist"),
            SpawnError::NotFound(name) => write!(f, "{name} was not found"),
            SpawnError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SpawnError {}

impl From<io::Error> for SpawnError {
    fn from(error: io::Error) -> Self {
        SpawnError::Io(error)
    }
}

/// How a program ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    Signal(i32),
}

/// Starts `argv` in `cwd` with exactly `env`, as the session leader of a
/// new PTY of `cols` × `rows` that is its controlling terminal. The
/// program is looked up on `env`'s `PATH`, not the host's, and runs
/// without a shell. Never falls back to another folder.
pub fn spawn(
    argv: &[String],
    cwd: &Path,
    env: &BTreeMap<String, String>,
    cols: u16,
    rows: u16,
) -> Result<Spawned, SpawnError> {
    if !cwd.is_dir() {
        return Err(SpawnError::FolderMissing);
    }
    let name = argv.first().map(String::as_str).unwrap_or_default();
    let program = find_program(name, env.get("PATH").map(String::as_str))
        .filter(|p| !name.contains('/') || p.is_file())
        .ok_or_else(|| SpawnError::NotFound(name.to_string()))?;
    let (master, slave) = open_pty(cols, rows)?;
    let mut command = Command::new(&program);
    command
        .arg0(name)
        .args(&argv[1..])
        .current_dir(cwd)
        .env_clear()
        .envs(env)
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave));
    // SAFETY: between fork and exec the step calls only setsid and ioctl,
    // which are async-signal-safe, and allocates nothing.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            // stdin is the PTY's slave side: make it the controlling
            // terminal of the new session
            if libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command.spawn()?;
    let pid = child.id();
    // the child is reaped by `reap`, never by `Child`
    drop(child);
    Ok(Spawned {
        pid,
        started: pinrail_runtime::process::process(pid).map(|p| p.started),
        master,
    })
}

/// Opens a PTY of the given size: the master non-blocking, both sides
/// closed on exec.
fn open_pty(cols: u16, rows: u16) -> io::Result<(OwnedFd, OwnedFd)> {
    let (mut master, mut slave): (RawFd, RawFd) = (-1, -1);
    let size = winsize(cols, rows);
    // SAFETY: both out-pointers are valid; the name buffer and terminal
    // settings may be null; the size is read during the call.
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &size as *const libc::winsize as *mut libc::winsize,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openpty succeeded and returned two new descriptors that
    // nothing else owns.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    for fd in [master.as_raw_fd(), slave.as_raw_fd()] {
        set_flag(fd, libc::F_GETFD, libc::F_SETFD, libc::FD_CLOEXEC)?;
    }
    set_flag(
        master.as_raw_fd(),
        libc::F_GETFL,
        libc::F_SETFL,
        libc::O_NONBLOCK,
    )?;
    Ok((master, slave))
}

fn set_flag(fd: RawFd, get: libc::c_int, set: libc::c_int, flag: libc::c_int) -> io::Result<()> {
    // SAFETY: fcntl on an open descriptor with integer arguments.
    unsafe {
        let flags = libc::fcntl(fd, get);
        if flags == -1 || libc::fcntl(fd, set, flags | flag) == -1 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn winsize(cols: u16, rows: u16) -> libc::winsize {
    libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

/// Sets the PTY's size, which sends the program `SIGWINCH`.
pub fn resize(master: &OwnedFd, cols: u16, rows: u16) -> io::Result<()> {
    let size = winsize(cols, rows);
    // SAFETY: TIOCSWINSZ reads a winsize during the call.
    if unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ as _, &size) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Blocks until the program exits, and leaves it unreaped, so that its
/// pid and process group id cannot be given to another process until
/// [`reap`].
pub fn wait_exit(pid: u32) -> io::Result<Exit> {
    loop {
        // SAFETY: siginfo_t is plain data, written by waitid.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: `info` is a valid place for the result.
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOWAIT,
            )
        };
        if result == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        // SAFETY: waitid filled in the fields of an exited child.
        let (code, status) = unsafe { (info.si_code, info.si_status()) };
        return Ok(if code == libc::CLD_EXITED {
            Exit::Code(status)
        } else {
            Exit::Signal(status)
        });
    }
}

/// Reaps an exited program, which frees its pid. Call it once the
/// terminal no longer signals it.
pub fn reap(pid: u32) {
    let mut status = 0;
    // SAFETY: waits for this child only, without blocking.
    unsafe { libc::waitpid(pid as libc::pid_t, &mut status, libc::WNOHANG) };
}

/// Sends a signal to a process group.
pub fn signal_group(pgid: u32, signal: i32) -> io::Result<()> {
    // SAFETY: killpg has no memory preconditions.
    if unsafe { libc::killpg(pgid as libc::pid_t, signal) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The PTY's foreground process group, if it has one.
pub fn foreground_group(master: &OwnedFd) -> Option<u32> {
    // SAFETY: tcgetpgrp on an open descriptor.
    let pgid = unsafe { libc::tcgetpgrp(master.as_raw_fd()) };
    u32::try_from(pgid).ok().filter(|&p| p > 0)
}

/// What [`wait_readable`] saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ready {
    /// The master has output, or has hung up, so the next read tells.
    Master,
    /// The wake descriptor was written: the reader should stop.
    Wake,
}

/// Waits until the master can be read or the wake descriptor is written.
/// Called only when a read would block, so a busy terminal reads without
/// a poll per read.
pub fn wait_readable(master: &impl AsRawFd, wake: &impl AsRawFd) -> io::Result<Ready> {
    let mut fds = [
        libc::pollfd {
            fd: wake.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: master.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    loop {
        // SAFETY: `fds` holds two valid pollfd entries for the call.
        let result = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
        if result == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if fds[0].revents != 0 {
            return Ok(Ready::Wake);
        }
        return Ok(Ready::Master);
    }
}

/// Waits until the master accepts more input.
pub fn wait_writable(master: &impl AsRawFd) -> io::Result<()> {
    let mut fd = libc::pollfd {
        fd: master.as_raw_fd(),
        events: libc::POLLOUT,
        revents: 0,
    };
    loop {
        // SAFETY: `fd` is a valid pollfd for the call.
        let result = unsafe { libc::poll(&mut fd, 1, -1) };
        if result == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        return Ok(());
    }
}

/// Waits up to `within` until `fd` can be read; `false` when the time ran
/// out first.
pub fn readable_within(fd: &impl AsRawFd, within: std::time::Duration) -> io::Result<bool> {
    let mut entry = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let millis = within.as_millis().min(i32::MAX as u128) as i32;
    // SAFETY: `entry` is a valid pollfd for the call.
    let result = unsafe { libc::poll(&mut entry, 1, millis) };
    match result {
        -1 => {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                Ok(false)
            } else {
                Err(error)
            }
        }
        0 => Ok(false),
        _ => Ok(true),
    }
}
