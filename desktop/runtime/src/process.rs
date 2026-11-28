//! The processes an agent runs as: a process's parent, its name and when it
//! started. The start time tells a live process from a later one that got
//! the same pid, so it is read one way, here, by everything that compares
//! it: the CLI when it reports a hook, and the core when it checks a
//! process again.

/// What the system says of a running process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    pub parent: u32,
    /// Its executable's name, as the system keeps it (cut to 15 bytes on
    /// Linux).
    pub name: String,
    /// When it started, as the platform's raw value: seconds and
    /// microseconds since the epoch on macOS, clock ticks since boot on
    /// Linux, 100 ns intervals since 1601 on Windows. Compare it only with
    /// another value read here.
    pub started: String,
}

/// How many parents the walk looks at before it gives up.
const MAX_DEPTH: usize = 32;

/// The process `pid`, if it runs and the platform says.
pub fn process(pid: u32) -> Option<Process> {
    read(pid)
}

/// The pid of this process's parent, such as the agent that ran a hook.
pub fn parent_id() -> Option<u32> {
    #[cfg(unix)]
    {
        Some(std::os::unix::process::parent_id())
    }
    #[cfg(not(unix))]
    {
        read(std::process::id()).map(|me| me.parent)
    }
}

/// Asks the process `pid` to end, or kills it with `force`, if it is still
/// the one that began at `started`, not a later process that got its pid.
/// Returns whether the signal was sent.
///
/// Windows has no way to ask a console program to end from outside its
/// console, so there it is ended either way.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn stop(pid: u32, started: &str, _force: bool) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
    if !read(pid).is_some_and(|p| p.started == started) {
        return false;
    }
    // SAFETY: takes no pointers; the handle is closed below
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if handle.is_null() {
        return false;
    }
    // SAFETY: the handle is open, then closed once
    let ended = unsafe { TerminateProcess(handle, 1) } != 0;
    unsafe { CloseHandle(handle) };
    ended
}

/// Asks the process `pid` to end, or kills it with `force`, if it is still
/// the one that began at `started`, not a later process that got its pid.
/// Returns whether the signal was sent.
#[cfg(unix)]
#[allow(unsafe_code)]
pub fn stop(pid: u32, started: &str, force: bool) -> bool {
    let Ok(raw) = i32::try_from(pid) else {
        return false;
    };
    let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
    read(pid).is_some_and(|p| p.started == started)
        // SAFETY: kill(2) takes any pid and signal, and only returns an error
        && unsafe { libc::kill(raw, signal) } == 0
}

/// The nearest process above `from` (itself included) that `accepts`
/// takes: the agent a hook or a command runs under, as its adapter
/// recognises it.
pub fn ancestor(from: u32, accepts: &dyn Fn(&Process) -> bool) -> Option<Process> {
    let mut pid = from;
    for _ in 0..MAX_DEPTH {
        let process = read(pid)?;
        if accepts(&process) {
            return Some(process);
        }
        if process.parent == 0 || process.parent == pid {
            return None;
        }
        pid = process.parent;
    }
    None
}

/// The arguments the process `pid` was started with, its own name first as
/// it was given: what tells an agent that runs under an interpreter, such
/// as `node`, from another. `None` when the system does not say.
pub fn arguments(pid: u32) -> Option<Vec<String>> {
    read_arguments(pid)
}

// KERN_PROCARGS2 gives the argument count, the executable's path, padding,
// then the arguments, each ending in a NUL
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn read_arguments(pid: u32) -> Option<Vec<String>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as libc::c_int];
    let mut size: libc::size_t = 0;
    // SAFETY: a null buffer asks for the size only
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
    let mut buffer = vec![0u8; size];
    // SAFETY: the buffer holds `size` bytes, which the call does not exceed
    let read = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if read != 0 {
        return None;
    }
    buffer.truncate(size);
    let count = i32::from_ne_bytes(buffer[..4].try_into().ok()?);
    let rest = &buffer[4..];
    // past the executable's path and the NULs after it
    let path_end = rest.iter().position(|b| *b == 0)?;
    let start = path_end + rest[path_end..].iter().position(|b| *b != 0)?;
    let args = rest[start..]
        .split(|b| *b == 0)
        .take(usize::try_from(count).ok()?)
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    Some(args)
}

#[cfg(target_os = "linux")]
fn read_arguments(pid: u32) -> Option<Vec<String>> {
    let line = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let line = line.strip_suffix(&[0]).unwrap_or(&line);
    Some(
        line.split(|b| *b == 0)
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect(),
    )
}

// the command line, which Windows keeps as one string, as the process's
// own parsing splits it
#[cfg(windows)]
#[allow(unsafe_code)]
fn read_arguments(pid: u32) -> Option<Vec<String>> {
    use windows_sys::Wdk::System::Threading::NtQueryInformationProcess;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, UNICODE_STRING};
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

    // PROCESSINFOCLASS's ProcessCommandLineInformation, since Windows 8.1
    const COMMAND_LINE: i32 = 60;
    // SAFETY: takes no pointers; the handle is closed below
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buffer = vec![0u64; 4096];
    let mut needed = 0u32;
    let mut status = -1;
    for _ in 0..2 {
        // SAFETY: the buffer holds `buffer.len() * 8` bytes, aligned for the
        // UNICODE_STRING the call writes first
        status = unsafe {
            NtQueryInformationProcess(
                handle,
                COMMAND_LINE,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut needed,
            )
        };
        if status >= 0 || (needed as usize) <= buffer.len() * 8 {
            break;
        }
        buffer = vec![0u64; (needed as usize).div_ceil(8)];
    }
    // SAFETY: the process handle is open and closed once
    unsafe { CloseHandle(handle) };
    if status < 0 {
        return None;
    }
    // SAFETY: the call wrote a UNICODE_STRING whose buffer lies inside ours
    let line: Vec<u16> = unsafe {
        let text = &*buffer.as_ptr().cast::<UNICODE_STRING>();
        if text.Buffer.is_null() || text.Length == 0 {
            return Some(Vec::new());
        }
        let mut line =
            std::slice::from_raw_parts(text.Buffer, usize::from(text.Length) / 2).to_vec();
        line.push(0);
        line
    };
    let mut count = 0;
    // SAFETY: the line is NUL-terminated; the array is freed below
    let argv = unsafe { CommandLineToArgvW(line.as_ptr(), &mut count) };
    if argv.is_null() {
        return None;
    }
    let args = (0..count as usize)
        .map(|i| {
            // SAFETY: each of the `count` entries is a NUL-terminated string
            unsafe {
                let arg = *argv.add(i);
                let len = (0..).take_while(|&j| *arg.add(j) != 0).count();
                String::from_utf16_lossy(std::slice::from_raw_parts(arg, len))
            }
        })
        .collect();
    // SAFETY: CommandLineToArgvW allocated the array with LocalAlloc
    unsafe { LocalFree(argv.cast()) };
    Some(args)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn read_arguments(_pid: u32) -> Option<Vec<String>> {
    None
}

// proc_pidinfo is the system's way to read another process's facts on macOS
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn read(pid: u32) -> Option<Process> {
    // SAFETY: proc_bsdinfo is plain data, for which all zeroes is valid
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: the buffer is a zeroed proc_bsdinfo of the size given
    let read = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        )
    };
    if read != size {
        return None;
    }
    let text = |chars: &[libc::c_char]| {
        let bytes: Vec<u8> = chars
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| *c as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    };
    // the long name when there is one: `comm` is cut to 16 bytes
    let name = match text(&info.pbi_name) {
        name if name.is_empty() => text(&info.pbi_comm),
        name => name,
    };
    Some(Process {
        pid,
        parent: info.pbi_ppid,
        name,
        started: format!("{}.{:06}", info.pbi_start_tvsec, info.pbi_start_tvusec),
    })
}

#[cfg(target_os = "linux")]
fn read(pid: u32) -> Option<Process> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_stat(pid, &stat)
}

/// `/proc/<pid>/stat`: the name between parentheses, which may hold spaces
/// and parentheses itself, then the parent (field 4) and the start time
/// (field 22).
#[cfg(any(target_os = "linux", test))]
fn parse_stat(pid: u32, stat: &str) -> Option<Process> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let name = stat.get(open + 1..close)?.to_string();
    let fields: Vec<&str> = stat.get(close + 1..)?.split_whitespace().collect();
    // fields after the name start at field 3, the state
    Some(Process {
        pid,
        parent: fields.get(1)?.parse().ok()?,
        name,
        started: fields.get(19)?.to_string(),
    })
}

// The process itself is asked for its parent, its executable and its
// times, which takes microseconds. A snapshot of every process says the
// first two as well, but takes milliseconds, and more of them the more
// processes run: the host asks twice a second for each of its terminals,
// and with 200 of them did nothing else.
#[cfg(windows)]
#[allow(unsafe_code)]
fn read(pid: u32) -> Option<Process> {
    use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_BASIC_INFORMATION,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: takes no pointers; the handle is closed below
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    let mut code = 0u32;
    // SAFETY: PROCESS_BASIC_INFORMATION is plain data, for which all zeroes
    // is valid
    let mut basic: PROCESS_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: the handle is open; each out-pointer points to a value of the
    // type and size the call writes
    let (timed, coded, parented) = unsafe {
        (
            GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0,
            GetExitCodeProcess(handle, &mut code) != 0,
            NtQueryInformationProcess(
                handle,
                ProcessBasicInformation,
                (&raw mut basic).cast(),
                std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
                std::ptr::null_mut(),
            ) >= 0,
        )
    };
    let name = if timed && coded {
        image_name(handle)
    } else {
        None
    };
    // SAFETY: the process handle is open and closed once
    unsafe { CloseHandle(handle) };
    if !timed || !coded {
        return None;
    }
    // A process that ended stays for as long as a handle keeps it, with its
    // exit code and the time it ended; one that runs has neither, unless
    // it is to end with the code that says a process is still active.
    let ended =
        code != STILL_ACTIVE as u32 || exited.dwLowDateTime != 0 || exited.dwHighDateTime != 0;
    if ended {
        return None;
    }
    // what the process does not say of itself, the list of them all does:
    // the system's own few processes have no executable to name
    let (parent, file) = match (parented, name) {
        (true, Some(name)) => (basic.InheritedFromUniqueProcessId as u32, name),
        _ => listed(pid)?,
    };
    let started = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
    // named without .exe, as on the other systems, and in lower case, since
    // Windows takes `Claude.exe` and `claude.exe` for the same program
    let file = file.to_lowercase();
    let name = match file.strip_suffix(".exe") {
        Some(name) => name.to_string(),
        None => file,
    };
    Some(Process {
        pid,
        parent,
        name,
        started: started.to_string(),
    })
}

/// The name of the executable of the process behind `handle`, without its
/// folder.
#[cfg(windows)]
#[allow(unsafe_code)]
fn image_name(handle: windows_sys::Win32::Foundation::HANDLE) -> Option<String> {
    use windows_sys::Win32::System::Threading::{PROCESS_NAME_WIN32, QueryFullProcessImageNameW};
    // a path is seldom longer than the first, and never than the second
    for room in [512usize, 32_768] {
        let mut path = vec![0u16; room];
        let mut len = room as u32;
        // SAFETY: the handle is open, and `path` holds `len` characters
        let read = unsafe {
            QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut len)
        };
        if read != 0 {
            let path = String::from_utf16_lossy(&path[..len as usize]);
            let file = path.rsplit(['\\', '/']).next().unwrap_or_default();
            return (!file.is_empty()).then(|| file.to_string());
        }
    }
    None
}

/// The parent and the executable's name of the process `pid`, as the
/// snapshot of every process lists them.
#[cfg(windows)]
#[allow(unsafe_code)]
fn listed(pid: u32) -> Option<(u32, String)> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    // SAFETY: takes no pointers; the handle is closed below
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    // SAFETY: PROCESSENTRY32W is plain data, for which all zeroes is valid
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = None;
    // SAFETY: `entry` is a valid PROCESSENTRY32W with its size set
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while more {
        if entry.th32ProcessID == pid {
            let file = &entry.szExeFile;
            let end = file.iter().position(|c| *c == 0).unwrap_or(file.len());
            found = Some((
                entry.th32ParentProcessID,
                String::from_utf16_lossy(&file[..end]),
            ));
            break;
        }
        // SAFETY: as for Process32FirstW
        more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    // SAFETY: the snapshot is open and closed once
    unsafe { CloseHandle(snapshot) };
    found
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn read(_pid: u32) -> Option<Process> {
    None
}

#[cfg(test)]
mod tests {

    /// A process that ended while a handle to it is still open, as the
    /// one that started it keeps one.
    #[cfg(windows)]
    #[test]
    fn an_ended_process_is_not_read_while_a_handle_keeps_it() {
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "exit 0"])
            .spawn()
            .unwrap();
        child.wait().unwrap();
        assert!(read(child.id()).is_none());
        drop(child);
    }
    use super::*;

    #[test]
    fn a_stat_line_gives_the_name_the_parent_and_the_start() {
        let stat = "4242 (claude (2.1)) S 4200 4242 4200 34817 4242 4194304 1 0 0 0 \
                    5 3 0 0 20 0 9 0 812345 1000 200 18446744073709551615";
        let process = parse_stat(4242, stat).unwrap();
        assert_eq!(process.name, "claude (2.1)");
        assert_eq!(process.parent, 4200);
        assert_eq!(process.started, "812345");
    }

    #[cfg(windows)]
    #[test]
    fn a_process_s_arguments_and_its_stop_on_windows() {
        let mut child = std::process::Command::new("cmd")
            .args(["/d", "/c", "ping -n 30 127.0.0.1 > nul", "with space"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let found = process(pid).unwrap();
        assert_eq!(found.name, "cmd");
        let args = arguments(pid).unwrap();
        assert!(
            args[0].to_lowercase().ends_with("cmd.exe") || args[0] == "cmd",
            "{args:?}"
        );
        assert_eq!(
            &args[1..],
            ["/d", "/c", "ping -n 30 127.0.0.1 > nul", "with space"]
        );
        // a start that is not the process's own stops nothing
        assert!(!stop(pid, "1", false));
        assert!(child.try_wait().unwrap().is_none());
        assert!(stop(pid, &found.started, false));
        assert!(child.wait().unwrap().code() == Some(1));
    }

    /// The host reads the process in front of each terminal twice a
    /// second: with 200 terminals, as many processes more for the list
    /// and a list for each read, its threads did nothing else.
    #[cfg(windows)]
    #[test]
    fn a_process_is_read_without_listing_them_all() {
        let me = std::process::id();
        let read_me = read(me).unwrap();
        let (parent, file) = listed(me).unwrap();
        assert_eq!(read_me.parent, parent);
        assert_eq!(format!("{}.exe", read_me.name), file.to_lowercase());
        let took = |act: &dyn Fn()| {
            let began = std::time::Instant::now();
            for _ in 0..200 {
                act();
            }
            began.elapsed()
        };
        let reads = took(&|| assert!(read(me).is_some()));
        let lists = took(&|| assert!(listed(me).is_some()));
        // some eighty times faster where this was written
        assert!(reads * 5 < lists, "{reads:?} to read, {lists:?} to list");
    }

    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    #[test]
    fn this_process_is_found_with_a_start_that_stays_the_same() {
        let me = std::process::id();
        let first = process(me).unwrap();
        assert_eq!(first.pid, me);
        assert!(!first.started.is_empty());
        assert_eq!(process(me).unwrap().started, first.started);
        // walking up from here reaches itself by its own name
        let name = first.name.clone();
        assert_eq!(ancestor(me, &|p| p.name == name).unwrap().pid, me);
        assert_eq!(ancestor(me, &|p| p.name == "no-such-agent"), None);
    }
}
