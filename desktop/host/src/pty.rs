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

pub use crate::pty_common::{Control, Exit, SpawnError, Spawned};

/// The host's side of a terminal: the PTY's master, non-blocking.
pub type Master = OwnedFd;

pub const SIGHUP: i32 = libc::SIGHUP;
pub const SIGINT: i32 = libc::SIGINT;
pub const SIGKILL: i32 = libc::SIGKILL;
pub const SIGTERM: i32 = libc::SIGTERM;

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

/// The PTY's size, as columns and rows.
pub fn size(master: &OwnedFd) -> io::Result<(u16, u16)> {
    let mut size = winsize(0, 0);
    // SAFETY: TIOCGWINSZ writes a winsize into the struct it is given.
    if unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCGWINSZ as _, &mut size) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok((size.ws_col.max(1), size.ws_row.max(1)))
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

/// Sends a signal to the program's process group and, if it differs, to
/// the PTY's foreground process group.
pub fn signal(master: &Master, pid: u32, signal: i32) -> io::Result<()> {
    signal_group(pid, signal)?;
    if let Some(foreground) = foreground_group(master)
        && foreground != pid
    {
        let _ = signal_group(foreground, signal);
    }
    Ok(())
}

/// The host's side of a terminal from the descriptors a holder sent: the
/// master.
pub fn master_from(
    _pid: u32,
    fds: &mut dyn Iterator<Item = OwnedFd>,
    _size: (u16, u16),
) -> Option<Master> {
    fds.next()
}

/// The host acts on a PTY itself, so it never needs the holder for it.
pub fn set_control(_: &Master, _: Control) {}

/// A program the holder started and keeps, with its PTY's master.
pub struct Holding {
    pub pid: u32,
    pub started: Option<String>,
    master: OwnedFd,
}

impl Holding {
    pub fn spawn(
        argv: &[String],
        cwd: &Path,
        env: &BTreeMap<String, String>,
        cols: u16,
        rows: u16,
    ) -> Result<Holding, SpawnError> {
        let spawned = spawn(argv, cwd, env, cols, rows)?;
        Ok(Holding {
            pid: spawned.pid,
            started: spawned.started,
            master: spawned.master,
        })
    }

    /// What the host gets: a duplicate of the master.
    pub fn for_host(&self) -> io::Result<Vec<OwnedFd>> {
        Ok(vec![self.master.try_clone()?])
    }

    pub fn size(&self) -> (u16, u16) {
        size(&self.master).unwrap_or((80, 24))
    }

    /// Something that waits until the program exits, for a thread of its
    /// own, leaving it unreaped.
    pub fn exit_waiter(&self) -> io::Result<impl FnOnce() -> io::Result<Exit> + Send + 'static> {
        let pid = self.pid;
        Ok(move || wait_exit(pid))
    }

    /// Nothing to do after an exit: the PTY ends once its programs have.
    pub fn exited(&self) {}

    /// Lets the terminal go: closes the master, and reaps the program if it
    /// exited, which frees its pid.
    pub fn forget(self, exited: bool) {
        if exited {
            reap(self.pid);
        }
    }
}

/// Wakes a reader that waits for output, so that it stops.
pub struct Waker(io::PipeWriter);

/// What a reader watches for its waker.
pub struct Wake(io::PipeReader);

pub fn wake_pair() -> io::Result<(Waker, Wake)> {
    let (read, write) = io::pipe()?;
    Ok((Waker(write), Wake(read)))
}

impl Waker {
    pub fn wake(&mut self) {
        use std::io::Write;
        let _ = self.0.write_all(b"x");
    }
}

/// Reads the PTY until it ends or the reader is woken, giving each piece to
/// `out` until it says to stop. Reads first, and waits in `poll` only when
/// a read would block.
pub fn read_loop(master: Master, wake: Wake, mut out: impl FnMut(Vec<u8>) -> bool) {
    use std::io::Read;
    let mut master = std::fs::File::from(master);
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match master.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if !out(buf[..n].to_vec()) {
                    return;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                match wait_readable(&master, &wake.0) {
                    Ok(Ready::Master) => {}
                    Ok(Ready::Wake) | Err(_) => break,
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            // EIO once every slave descriptor is closed
            Err(_) => break,
        }
    }
}

/// Writes every input to the PTY in order, waiting when it is full, until
/// the terminal ends.
pub fn write_loop(master: Master, input: std::sync::mpsc::Receiver<Vec<u8>>) {
    use std::io::Write;
    let mut master = std::fs::File::from(master);
    for bytes in input {
        let mut rest = &bytes[..];
        while !rest.is_empty() {
            match master.write(rest) {
                Ok(n) => rest = &rest[n..],
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if wait_writable(&master).is_err() {
                        return;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return,
            }
        }
    }
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
