//! Terminals on Windows: each program on a pseudo console of its own
//! (ConPTY), with everything it starts in a job.
//!
//! The holder makes the pseudo console and keeps it: its handle cannot be
//! passed to another process, so the holder resizes and closes it when the
//! host asks. What the host reads and writes, and the job it ends, it gets
//! as duplicates of the holder's handles: the console's input and output
//! pipes and the job. The output pipe is opened for overlapped I/O, so a
//! reader can be woken as on Unix.
//!
//! The pseudo console is Microsoft's own build from beside the binary,
//! `conpty\conpty.dll`, when it is there, since Windows' drops the escape
//! sequences programs answer with; Windows' own otherwise. Adapted from
//! herdr-core's crates/herdr-core/src/platform/windows.rs and
//! windows_foreground.rs, with the console kept by the holder.

// ConPTY, jobs, pipes and process creation can only be used with `unsafe`;
// each use says why it is sound.
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::fs::File;
use std::io::{self, Write};
use std::os::windows::io::{FromRawHandle, OwnedHandle, RawHandle};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};

use windows_sys::Win32::Foundation::{
    DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, FILETIME,
    GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
    PIPE_ACCESS_INBOUND, ReadFile,
};
use windows_sys::Win32::System::Console::{COORD, HPCON, SetConsoleCtrlHandler};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_BREAKAWAY_OK,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, QueryInformationJobObject,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, CreatePipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetCurrentProcess, GetExitCodeProcess, GetProcessTimes, INFINITE,
    InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST, OpenProcess,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, ResumeThread,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, SetEvent, UpdateProcThreadAttribute,
    WaitForMultipleObjects, WaitForSingleObject,
};

pub use crate::pty_common::{Control, Exit, SpawnError, Spawned};
use crate::win::{check, event, raw, wide};

/// The protocol's signals, by their usual numbers. Windows has none of
/// them: [`signal`] gives each its meaning on a pseudo console.
pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGKILL: i32 = 9;
pub const SIGTERM: i32 = 15;

/// The exit code of a program whose job was ended.
const KILLED: u32 = 1;

// --- ConPTY ------------------------------------------------------------------

type CreatePc = unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> i32;
type ResizePc = unsafe extern "system" fn(HPCON, COORD) -> i32;
type ClosePc = unsafe extern "system" fn(HPCON);

struct ConPty {
    create: CreatePc,
    resize: ResizePc,
    close: ClosePc,
}

/// The ConPTY beside the binary, else Windows' own. A broken one fails each
/// start, never the process.
fn conpty() -> io::Result<&'static ConPty> {
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleHandleW, GetProcAddress, LoadLibraryW,
    };
    static API: std::sync::OnceLock<Result<ConPty, String>> = std::sync::OnceLock::new();
    let api = API.get_or_init(|| {
        let bundled = std::env::current_exe()
            .ok()
            .and_then(|exe| Some(exe.parent()?.join(r"conpty\conpty.dll")))
            .filter(|dll| dll.is_file());
        let from = bundled.as_ref().map_or_else(
            || "kernel32.dll".to_string(),
            |dll| dll.display().to_string(),
        );
        // SAFETY: the names are NUL-terminated and outlive the calls; a
        // module loaded stays loaded for the life of the process.
        let module = unsafe {
            match &bundled {
                Some(dll) => LoadLibraryW(wide(dll.as_os_str()).as_ptr()),
                None => GetModuleHandleW(wide("kernel32.dll".as_ref()).as_ptr()),
            }
        };
        if module.is_null() {
            return Err(format!(
                "cannot load ConPTY from {from}: {}",
                io::Error::last_os_error()
            ));
        }
        type Proc = unsafe extern "system" fn() -> isize;
        let get = |name: &std::ffi::CStr| {
            // SAFETY: `module` is loaded; the name is a C string.
            unsafe { GetProcAddress(module, name.as_ptr().cast()) }
                .ok_or_else(|| format!("{from} has no {name:?}"))
        };
        // SAFETY: these exports have exactly these signatures (consoleapi.h).
        unsafe {
            Ok(ConPty {
                create: std::mem::transmute::<Proc, CreatePc>(get(c"CreatePseudoConsole")?),
                resize: std::mem::transmute::<Proc, ResizePc>(get(c"ResizePseudoConsole")?),
                close: std::mem::transmute::<Proc, ClosePc>(get(c"ClosePseudoConsole")?),
            })
        }
    });
    api.as_ref().map_err(|e| io::Error::other(e.clone()))
}

fn coord(cols: u16, rows: u16) -> COORD {
    COORD {
        X: cols.clamp(1, i16::MAX as u16) as i16,
        Y: rows.clamp(1, i16::MAX as u16) as i16,
    }
}

/// A pseudo console, closed once: when its program ended, so that its last
/// output arrives and the reader ends, or when it is forgotten. Closing can
/// wait until the output pipe is read, so it happens on a thread of its own.
struct PseudoConsole(Mutex<Option<usize>>);

impl PseudoConsole {
    fn close(self: &Arc<Self>) {
        let taken = self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let (Some(hpc), Ok(api)) = (taken, conpty()) {
            std::thread::spawn(move || {
                // SAFETY: `hpc` was made by this ConPTY and is closed once.
                unsafe { (api.close)(hpc as HPCON) }
            });
        }
    }

    fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        let hpc = *self.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some(hpc) = hpc else {
            return Err(io::Error::other("the terminal is closed"));
        };
        // SAFETY: `hpc` is open while the lock says so.
        let result = unsafe { (conpty()?.resize)(hpc as HPCON, coord(cols, rows)) };
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result));
        }
        Ok(())
    }
}

// --- Programs ----------------------------------------------------------------

/// The program as Windows starts it: found on the `PATH` with `PATHEXT`,
/// and a batch file through cmd, as npm installs agents as `.cmd` shims.
fn command_line(argv: &[String], env: &BTreeMap<String, String>) -> Result<Vec<u16>, SpawnError> {
    let name = argv.first().map(String::as_str).unwrap_or_default();
    let var = |key: &str| {
        env.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.clone())
    };
    let found = crate::environment::find_program(name, var("PATH").as_deref())
        .ok_or_else(|| SpawnError::NotFound(name.to_string()))?;
    let ext = found
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let line = if ext == "cmd" || ext == "bat" {
        batch_line(&found, &argv[1..])?
    } else {
        std::iter::once(found.display().to_string())
            .chain(argv[1..].iter().cloned())
            .map(|a| quote(&a))
            .collect::<Vec<_>>()
            .join(" ")
    };
    Ok(wide(line.as_ref()))
}

/// A batch file's command line through cmd, which reads `& | < > ^ %`
/// itself: each argument quoted, `"` doubled and `%` defused, so that it
/// arrives as one literal argument, as Rust's std quotes them. cmd cannot
/// carry a line break in an argument.
fn batch_line(script: &Path, args: &[String]) -> io::Result<String> {
    let mut line = format!("cmd.exe /e:ON /v:OFF /d /c \"\"{}\"", script.display());
    for arg in args {
        if arg.contains(['\r', '\n', '\0']) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "{} is a batch file, which cannot take an argument with a line break",
                    script.display()
                ),
            ));
        }
        let plain = |c: char| {
            c.is_ascii_alphanumeric()
                || r"#$*+-./:?@\_".contains(c)
                || !c.is_ascii() && !c.is_control()
        };
        let quoted = arg.is_empty() || arg.ends_with('\\') || !arg.chars().all(plain);
        line.push(' ');
        if quoted {
            line.push('"');
        }
        let mut slashes = 0;
        for c in arg.chars() {
            match c {
                '\\' => slashes += 1,
                '"' => {
                    line.push_str(&"\\".repeat(slashes));
                    line.push('"');
                    slashes = 0;
                }
                // `%cd:~,%` expands to nothing, so `%VAR%` cannot pair up
                '%' => {
                    line.push_str("%%cd:~,");
                    slashes = 0;
                }
                _ => slashes = 0,
            }
            line.push(c);
        }
        if quoted {
            line.push_str(&"\\".repeat(slashes));
            line.push('"');
        }
    }
    line.push('"');
    Ok(line)
}

/// One argument as Windows' command-line parsing reads it back.
fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.into();
    }
    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(slashes * 2 + 1));
                out.push('"');
                slashes = 0;
            }
            c => {
                out.push_str(&"\\".repeat(slashes));
                out.push(c);
                slashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}

/// The environment block: `name=value` strings, each ending in a NUL,
/// sorted by name without regard to case, as Windows expects.
fn env_block(env: &BTreeMap<String, String>) -> Vec<u16> {
    let mut pairs: Vec<_> = env.iter().collect();
    pairs.sort_by_key(|(k, _)| k.to_uppercase());
    let mut block = Vec::new();
    for (k, v) in pairs {
        block.extend(format!("{k}={v}").encode_utf16());
        block.push(0);
    }
    block.push(0);
    block
}

/// `path` without the `\\?\` prefix a canonical path has, which cmd
/// cannot use as its folder: it falls back to Windows' own.
fn plain(path: &Path) -> PathBuf {
    match path.to_str().and_then(|p| p.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with(r"UNC\") => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// A pipe for the console's output whose read end is opened for overlapped
/// I/O, so a reader can be woken; the write end is the console's.
fn output_pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let name = wide(format!(r"\\.\pipe\pinrail-pty-{}", crate::random_hex(8)).as_ref());
    // SAFETY: the name is NUL-terminated; one instance, readable by this
    // user only; the handle is owned below.
    let read = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_INBOUND | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            0,
            64 * 1024,
            0,
            crate::win::pipe_security()?,
        )
    };
    if read == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: as above.
    let read = unsafe { OwnedHandle::from_raw_handle(read as RawHandle) };
    // SAFETY: the name is NUL-terminated; the handle is owned below.
    let write = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_WRITE,
            0,
            null(),
            OPEN_EXISTING,
            0,
            null_mut(),
        )
    };
    if write == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: as above.
    let write = unsafe { OwnedHandle::from_raw_handle(write as RawHandle) };
    // the client is connected already: the wait completes at once
    // SAFETY: OVERLAPPED is plain data; the pipe is open.
    let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
    let done = event()?;
    ov.hEvent = raw(&done);
    // SAFETY: `ov` lives until the call's result is read.
    if unsafe { ConnectNamedPipe(raw(&read), &mut ov) } == 0 {
        // SAFETY: reads this thread's last error.
        match unsafe { GetLastError() } {
            ERROR_PIPE_CONNECTED => {}
            ERROR_IO_PENDING => {
                let mut n = 0u32;
                // SAFETY: waits for the operation `ov` started.
                check(unsafe { GetOverlappedResult(raw(&read), &ov, &mut n, 1) })?;
            }
            error => return Err(io::Error::from_raw_os_error(error as i32)),
        }
    }
    Ok((read, write))
}

fn duplicate(handle: &OwnedHandle) -> io::Result<OwnedHandle> {
    let mut copy: HANDLE = null_mut();
    // SAFETY: duplicates a handle of this process within it.
    check(unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            raw(handle),
            GetCurrentProcess(),
            &mut copy,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    })?;
    // SAFETY: the copy is new and owned here.
    Ok(unsafe { OwnedHandle::from_raw_handle(copy as RawHandle) })
}

// --- The holder's side -------------------------------------------------------

/// A program the holder started and keeps, with its pseudo console.
pub struct Holding {
    pub pid: u32,
    pub started: Option<String>,
    console: Arc<PseudoConsole>,
    process: OwnedHandle,
    job: OwnedHandle,
    input: OwnedHandle,
    output: OwnedHandle,
    size: Mutex<(u16, u16)>,
}

impl Holding {
    /// Starts `argv` in `cwd` with exactly `env`, on a pseudo console of
    /// `cols` × `rows`, in a job of its own. The program is looked up on
    /// `env`'s `PATH`, not the holder's. Never falls back to another
    /// folder.
    pub fn spawn(
        argv: &[String],
        cwd: &Path,
        env: &BTreeMap<String, String>,
        cols: u16,
        rows: u16,
    ) -> Result<Holding, SpawnError> {
        if !cwd.is_dir() {
            return Err(SpawnError::FolderMissing);
        }
        let mut line = command_line(argv, env)?;
        let mut block = env_block(env);
        let folder = wide(plain(cwd).as_os_str());
        // SAFETY: each call below is given valid, live arguments, as noted;
        // every handle made is owned by a value that closes it.
        unsafe {
            // A process started in a group of its own ignores Ctrl+C, and
            // its children inherit that: take Ctrl+C again for the program.
            SetConsoleCtrlHandler(None, 0);
            let (mut in_read, mut in_write) = (null_mut(), null_mut());
            check(CreatePipe(&mut in_read, &mut in_write, null(), 0))?;
            let in_read = OwnedHandle::from_raw_handle(in_read as RawHandle);
            let input = OwnedHandle::from_raw_handle(in_write as RawHandle);
            let (output, out_write) = output_pipe()?;
            let mut hpc: HPCON = 0;
            let result = (conpty()?.create)(
                coord(cols, rows),
                raw(&in_read),
                raw(&out_write),
                0,
                &mut hpc,
            );
            if result != 0 {
                return Err(SpawnError::Io(io::Error::other(format!(
                    "the pseudo console did not start: {result:#x}"
                ))));
            }
            let console = Arc::new(PseudoConsole(Mutex::new(Some(hpc as usize))));
            // the console holds its own ends now
            drop((in_read, out_write));
            let fail = |error: io::Error| {
                console.close();
                SpawnError::Io(error)
            };

            // Everything the program starts goes with it, and ends when the
            // last handle to the job closes: the holder's and the host's.
            // Breakaway lets a holder or a host started from inside outlive
            // it.
            let job = CreateJobObjectW(null(), null());
            if job.is_null() {
                return Err(fail(io::Error::last_os_error()));
            }
            let job = OwnedHandle::from_raw_handle(job as RawHandle);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags =
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
            check(SetInformationJobObject(
                raw(&job),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ))
            .map_err(fail)?;

            let mut needed = 0usize;
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut needed);
            let mut attributes = vec![0u8; needed];
            let list = attributes.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
            check(InitializeProcThreadAttributeList(list, 1, 0, &mut needed)).map_err(fail)?;
            let updated = check(UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                hpc as *const c_void,
                size_of::<HPCON>(),
                null_mut(),
                null(),
            ));
            if let Err(error) = updated {
                DeleteProcThreadAttributeList(list);
                return Err(fail(error));
            }
            let mut startup: STARTUPINFOEXW = std::mem::zeroed();
            startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
            startup.lpAttributeList = list;
            // otherwise the program writes to the holder's own standard
            // streams, not to the pseudo console
            startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
            startup.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
            startup.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
            let mut info: PROCESS_INFORMATION = std::mem::zeroed();
            let created = CreateProcessW(
                null(),
                line.as_mut_ptr(),
                null(),
                null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED,
                block.as_mut_ptr().cast(),
                folder.as_ptr(),
                &startup.StartupInfo,
                &mut info,
            );
            DeleteProcThreadAttributeList(list);
            check(created).map_err(fail)?;
            let process = OwnedHandle::from_raw_handle(info.hProcess as RawHandle);
            let thread = OwnedHandle::from_raw_handle(info.hThread as RawHandle);
            // in the job before it runs, so nothing it starts escapes
            if AssignProcessToJobObject(raw(&job), raw(&process)) == 0 {
                let error = io::Error::last_os_error();
                TerminateJobObject(raw(&job), KILLED);
                windows_sys::Win32::System::Threading::TerminateProcess(raw(&process), KILLED);
                return Err(fail(error));
            }
            ResumeThread(raw(&thread));
            let pid = info.dwProcessId;
            Ok(Holding {
                pid,
                started: pinrail_runtime::process::process(pid).map(|p| p.started),
                console,
                process,
                job,
                input,
                output,
                size: Mutex::new((cols, rows)),
            })
        }
    }

    /// What the host gets: the console's input and output, and the job.
    pub fn for_host(&self) -> io::Result<Vec<OwnedHandle>> {
        Ok(vec![
            duplicate(&self.input)?,
            duplicate(&self.output)?,
            duplicate(&self.job)?,
        ])
    }

    pub fn size(&self) -> (u16, u16) {
        *self.size.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        self.console.resize(cols, rows)?;
        *self.size.lock().unwrap_or_else(|e| e.into_inner()) = (cols, rows);
        Ok(())
    }

    /// Closes the pseudo console, which tells every program on it that its
    /// console is gone, as a hangup does; once they end, the host reads the
    /// end of the output.
    pub fn close(&self) {
        self.console.close();
    }

    /// Something that waits until the program exits, for a thread of its
    /// own.
    pub fn exit_waiter(&self) -> io::Result<impl FnOnce() -> io::Result<Exit> + Send + 'static> {
        let process = duplicate(&self.process)?;
        Ok(move || {
            // SAFETY: waits on an open process handle, then reads its code.
            unsafe {
                if WaitForSingleObject(raw(&process), INFINITE) != WAIT_OBJECT_0 {
                    return Err(io::Error::last_os_error());
                }
                let mut code = 0u32;
                check(GetExitCodeProcess(raw(&process), &mut code))?;
                Ok(Exit::Code(code as i32))
            }
        })
    }

    /// After the program exits: its console closes, so its last output
    /// arrives and the host's reader ends, as a PTY whose programs have all
    /// gone ends on Unix.
    pub fn exited(&self) {
        self.console.close();
    }

    /// Lets the terminal go. The process handle kept its id from being
    /// given to another process until now.
    pub fn forget(self, _exited: bool) {
        self.console.close();
    }
}

/// Nothing to reap: a process's id stays its own while a handle to it is
/// open, and the holder closes its handle when it forgets the terminal.
pub fn reap(_pid: u32) {}

// --- The host's side ---------------------------------------------------------

struct Inner {
    pid: u32,
    input: File,
    output: OwnedHandle,
    job: OwnedHandle,
    size: Mutex<(u16, u16)>,
    control: Mutex<Option<Control>>,
    /// Whether the program at the root is a shell that runs a command, read
    /// once: its arguments do not change.
    launcher: std::sync::OnceLock<bool>,
}

/// The host's side of a terminal: the console's input and output, the job,
/// and how to reach the holder that keeps the console.
#[derive(Clone)]
pub struct Master(Arc<Inner>);

impl Master {
    pub fn try_clone(&self) -> io::Result<Master> {
        Ok(self.clone())
    }
}

/// The host's side of a terminal from the handles a holder sent, in the
/// order [`Holding::for_host`] gives them.
pub fn master_from(
    pid: u32,
    handles: &mut dyn Iterator<Item = OwnedHandle>,
    size: (u16, u16),
) -> Option<Master> {
    let (input, output, job) = (handles.next()?, handles.next()?, handles.next()?);
    Some(Master(Arc::new(Inner {
        pid,
        input: File::from(input),
        output,
        job,
        size: Mutex::new(size),
        control: Mutex::new(None),
        launcher: std::sync::OnceLock::new(),
    })))
}

/// Says how the host reaches the holder that keeps the terminal.
pub fn set_control(master: &Master, control: Control) {
    *master.0.control.lock().unwrap_or_else(|e| e.into_inner()) = Some(control);
}

fn ask(master: &Master, request: serde_json::Value) -> bool {
    let control = master.0.control.lock().unwrap_or_else(|e| e.into_inner());
    match control.as_ref() {
        Some(control) => {
            control(request);
            true
        }
        None => false,
    }
}

/// The terminal's size, as columns and rows.
pub fn size(master: &Master) -> io::Result<(u16, u16)> {
    Ok(*master.0.size.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Resizes the pseudo console, through the holder that keeps it.
pub fn resize(master: &Master, cols: u16, rows: u16) -> io::Result<()> {
    *master.0.size.lock().unwrap_or_else(|e| e.into_inner()) = (cols, rows);
    if !ask(
        master,
        serde_json::json!({"op": "resize", "cols": cols, "rows": rows}),
    ) {
        return Err(io::Error::other("no holder keeps the terminal"));
    }
    Ok(())
}

/// What a signal means on a pseudo console: an interrupt is Ctrl+C typed
/// into it; a hangup or a terminate closes the console, which tells its
/// programs so, as a hangup does; a kill ends the job and everything in it.
pub fn signal(master: &Master, _pid: u32, signal: i32) -> io::Result<()> {
    match signal {
        SIGINT => (&master.0.input).write_all(b"\x03"),
        SIGHUP | SIGTERM if ask(master, serde_json::json!({"op": "close"})) => Ok(()),
        _ => {
            // SAFETY: the job handle is open.
            check(unsafe { TerminateJobObject(raw(&master.0.job), KILLED) })
        }
    }
}

/// The program in front: a pseudo console has no foreground process group.
/// What stands in for one: the newest live program a known shell at the
/// root started itself; otherwise the program at the root. A shell that
/// runs a command, as cmd runs a batch file an agent is installed as, is
/// a launcher, not a shell a person types into: what it starts stays
/// behind it, as on Unix, where a shell without job control keeps its
/// programs in its own process group.
pub fn foreground_group(master: &Master) -> Option<u32> {
    // SAFETY: the job handle is open.
    let procs = unsafe { job_processes(raw(&master.0.job)) };
    let root = master.0.pid;
    if !procs.iter().any(|p| p.pid == root) {
        return None;
    }
    let launcher = *master.0.launcher.get_or_init(|| {
        pinrail_runtime::process::arguments(root).is_some_and(|args| runs_a_command(&args))
    });
    if launcher {
        return Some(root);
    }
    Some(newest_child(root, &procs).unwrap_or(root))
}

/// Whether a shell's arguments give it a command to run: `cmd /c`,
/// `powershell -Command` or `-File`, `sh -c`.
fn runs_a_command(args: &[String]) -> bool {
    args.iter().skip(1).any(|arg| {
        let arg = arg.to_ascii_lowercase();
        matches!(
            arg.as_str(),
            "/c" | "-c" | "-command" | "-file" | "-f" | "-encodedcommand" | "-e"
        )
    })
}

/// A process in a terminal's job.
struct Proc {
    pid: u32,
    parent: u32,
    started: u64,
    name: String,
}

/// A known shell root's newest child, console hosts aside: Windows starts
/// them for console programs, and they are never what a person uses.
fn newest_child(root: u32, procs: &[Proc]) -> Option<u32> {
    let shell = |name: &str| {
        let name = name.to_ascii_lowercase();
        matches!(
            name.strip_suffix(".exe").unwrap_or(&name),
            "zsh"
                | "bash"
                | "fish"
                | "sh"
                | "dash"
                | "nu"
                | "pwsh"
                | "powershell"
                | "cmd"
                | "elvish"
                | "xonsh"
                | "tcsh"
                | "csh"
                | "ksh"
                | "ion"
        )
    };
    if !procs.iter().any(|p| p.pid == root && shell(&p.name)) {
        return None;
    }
    let console_host = |name: &str| {
        ["conhost.exe", "OpenConsole.exe"]
            .iter()
            .any(|h| name.eq_ignore_ascii_case(h))
    };
    procs
        .iter()
        .filter(|p| p.parent == root && p.pid != root && !console_host(&p.name))
        .max_by_key(|p| p.started)
        .map(|p| p.pid)
}

/// The job's live processes, read one by one: a few per terminal, which a
/// snapshot of the whole system would cost more for.
unsafe fn job_processes(job: HANDLE) -> Vec<Proc> {
    #[repr(C)]
    struct Ids {
        assigned: u32,
        listed: u32,
        ids: [usize; 64],
    }
    // SAFETY: the list is plain data the call fills as far as it fits.
    unsafe {
        let mut list: Ids = std::mem::zeroed();
        QueryInformationJobObject(
            job,
            JobObjectBasicProcessIdList,
            (&raw mut list).cast(),
            size_of::<Ids>() as u32,
            null_mut(),
        );
        let n = (list.listed as usize).min(list.ids.len());
        list.ids[..n]
            .iter()
            .filter_map(|&pid| about(pid as u32))
            .collect()
    }
}

fn about(pid: u32) -> Option<Proc> {
    use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows_sys::Win32::System::Threading::PROCESS_BASIC_INFORMATION;
    // SAFETY: the handle is owned below; each call writes within the size
    // it is given.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let process = OwnedHandle::from_raw_handle(handle as RawHandle);
        let mut basic: PROCESS_BASIC_INFORMATION = std::mem::zeroed();
        let size = size_of::<PROCESS_BASIC_INFORMATION>() as u32;
        if NtQueryInformationProcess(
            raw(&process),
            ProcessBasicInformation,
            (&raw mut basic).cast(),
            size,
            null_mut(),
        ) != 0
        {
            return None;
        }
        let [mut started, mut exited, mut kernel, mut user]: [FILETIME; 4] = std::mem::zeroed();
        if GetProcessTimes(
            raw(&process),
            &mut started,
            &mut exited,
            &mut kernel,
            &mut user,
        ) == 0
        {
            return None;
        }
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        if QueryFullProcessImageNameW(
            raw(&process),
            PROCESS_NAME_WIN32,
            path.as_mut_ptr(),
            &mut len,
        ) == 0
        {
            return None;
        }
        let path = String::from_utf16_lossy(&path[..len as usize]);
        Some(Proc {
            pid,
            parent: basic.InheritedFromUniqueProcessId as u32,
            started: (u64::from(started.dwHighDateTime) << 32) | u64::from(started.dwLowDateTime),
            name: path.rsplit('\\').next().unwrap_or_default().to_string(),
        })
    }
}

// --- Reading and writing -----------------------------------------------------

/// Wakes a reader that waits for output, so that it stops.
pub struct Waker(Arc<OwnedHandle>);

/// What a reader watches for its waker.
pub struct Wake(Arc<OwnedHandle>);

pub fn wake_pair() -> io::Result<(Waker, Wake)> {
    let event = Arc::new(event()?);
    Ok((Waker(event.clone()), Wake(event)))
}

impl Waker {
    pub fn wake(&mut self) {
        // SAFETY: the event is open.
        unsafe { SetEvent(raw(&*self.0)) };
    }
}

/// Reads the console's output until it ends or the reader is woken, giving
/// each piece to `out` until it says to stop.
pub fn read_loop(master: Master, wake: Wake, mut out: impl FnMut(Vec<u8>) -> bool) {
    let output = raw(&master.0.output);
    let woken = raw(&*wake.0);
    let mut buf = vec![0u8; 64 * 1024];
    let Ok(done) = event() else {
        return;
    };
    loop {
        // SAFETY: OVERLAPPED is plain data; the buffer and `ov` live until
        // the operation completes or is cancelled and waited for.
        let n = unsafe {
            let mut ov: OVERLAPPED = std::mem::zeroed();
            ov.hEvent = raw(&done);
            if ReadFile(
                output,
                buf.as_mut_ptr(),
                buf.len() as u32,
                null_mut(),
                &mut ov,
            ) == 0
            {
                match GetLastError() {
                    ERROR_IO_PENDING => {}
                    _ => break,
                }
                let which = WaitForMultipleObjects(2, [woken, ov.hEvent].as_ptr(), 0, INFINITE);
                if which != WAIT_OBJECT_0 + 1 {
                    CancelIoEx(output, &ov);
                    let mut n = 0u32;
                    GetOverlappedResult(output, &ov, &mut n, 1);
                    break;
                }
            }
            let mut n = 0u32;
            // the console closed its end, or the read was cancelled
            if GetOverlappedResult(output, &ov, &mut n, 1) == 0 {
                break;
            }
            n as usize
        };
        if n == 0 {
            continue;
        }
        if !out(buf[..n].to_vec()) {
            return;
        }
    }
}

/// Writes every input to the console in order, until the terminal ends.
pub fn write_loop(master: Master, input: std::sync::mpsc::Receiver<Vec<u8>>) {
    for bytes in input {
        if (&master.0.input).write_all(&bytes).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_quoted_as_windows_reads_them_back() {
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote("two words"), "\"two words\"");
        assert_eq!(quote(""), "\"\"");
        assert_eq!(quote(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(quote(r"C:\dir with space\"), r#""C:\dir with space\\""#);
    }

    #[test]
    fn a_batch_file_takes_its_arguments_literally_and_refuses_a_line_break() {
        let line = batch_line(
            Path::new(r"C:\npm\claude.cmd"),
            &["--model".into(), "a & b %PATH%".into()],
        )
        .unwrap();
        assert_eq!(
            line,
            r#"cmd.exe /e:ON /v:OFF /d /c ""C:\npm\claude.cmd" --model "a & b %%cd:~,%PATH%%cd:~,%"""#
        );
        assert!(batch_line(Path::new("x.cmd"), &["two\nlines".into()]).is_err());
    }

    #[test]
    fn a_canonical_folder_loses_the_prefix_cmd_cannot_use() {
        assert_eq!(plain(Path::new(r"\\?\C:\dev\x")), Path::new(r"C:\dev\x"));
        assert_eq!(plain(Path::new(r"C:\dev\x")), Path::new(r"C:\dev\x"));
        // a share's long form has no short one to fall back to
        assert_eq!(
            plain(Path::new(r"\\?\UNC\server\share")),
            Path::new(r"\\?\UNC\server\share")
        );
    }

    #[test]
    fn a_shell_given_a_command_is_a_launcher() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(runs_a_command(&args(&[
            "cmd.exe", "/e:ON", "/v:OFF", "/d", "/c", "x.cmd"
        ])));
        assert!(runs_a_command(&args(&[
            "pwsh",
            "-NoProfile",
            "-Command",
            "x"
        ])));
        assert!(runs_a_command(&args(&["sh", "-c", "x"])));
        assert!(!runs_a_command(&args(&["cmd", "/d", "/q"])));
        assert!(!runs_a_command(&args(&["pwsh", "-NoLogo"])));
    }

    #[test]
    fn the_newest_program_a_shell_started_is_in_front() {
        let p = |pid, parent, started, name: &str| Proc {
            pid,
            parent,
            started,
            name: name.into(),
        };
        assert_eq!(newest_child(10, &[p(10, 1, 1, "powershell.exe")]), None);
        let procs = [
            p(10, 1, 1, "powershell.exe"),
            p(11, 10, 2, "node.exe"),
            p(12, 10, 3, "claude.exe"),
            p(13, 10, 4, "conhost.exe"),
            p(14, 12, 5, "node.exe"),
        ];
        assert_eq!(newest_child(10, &procs), Some(12));
        // a program started directly keeps its place when it starts a helper
        assert_eq!(
            newest_child(10, &[p(10, 1, 1, "node.exe"), p(11, 10, 2, "ping.exe")]),
            None
        );
    }
}
