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
    /// Linux. Compare it only with another value read here.
    pub started: String,
}

/// How many parents the walk looks at before it gives up.
const MAX_DEPTH: usize = 32;

/// The process `pid`, if it runs and the platform says.
pub fn process(pid: u32) -> Option<Process> {
    read(pid)
}

/// Whether a process named `name` runs `binary`: by its own name, or by
/// the name of a versioned file it links to, `<binary>-<version>…`, as
/// Grok's installer links `grok` to `grok-1.0.89-macos-aarch64`. The
/// system names a process after the file it runs.
pub fn named(name: &str, binary: &str) -> bool {
    name == binary
        || name
            .strip_prefix(binary)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(|version| version.starts_with(|c: char| c.is_ascii_digit()))
}

/// The nearest process above `from` (itself included) that runs one of
/// `names` (see [`named`]): the agent a hook or a command runs under.
pub fn ancestor(from: u32, names: &[&str]) -> Option<Process> {
    let mut pid = from;
    for _ in 0..MAX_DEPTH {
        let process = read(pid)?;
        if names.iter().any(|n| named(&process.name, n)) {
            return Some(process);
        }
        if process.parent == 0 || process.parent == pid {
            return None;
        }
        pid = process.parent;
    }
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

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn read(_pid: u32) -> Option<Process> {
    None
}

#[cfg(test)]
mod tests {
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

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn this_process_is_found_with_a_start_that_stays_the_same() {
        let me = std::process::id();
        let first = process(me).unwrap();
        assert_eq!(first.pid, me);
        assert!(!first.started.is_empty());
        assert_eq!(process(me).unwrap().started, first.started);
        // walking up from here reaches itself by its own name
        let name = first.name.clone();
        assert_eq!(ancestor(me, &[name.as_str()]).unwrap().pid, me);
        assert_eq!(ancestor(me, &["no-such-agent"]), None);
    }

    #[test]
    fn a_process_named_after_a_versioned_binary_is_that_binary() {
        // Grok's installer links grok to grok-<version>-<platform>, and the
        // system names the process after the file it runs
        assert!(named("grok-1.0.89-macos-aarch64", "grok"));
        // cut to 15 bytes on Linux
        assert!(named("grok-1.0.89-lin", "grok"));
        assert!(named("grok", "grok"));
        assert!(!named("grok-helper", "grok"));
        assert!(!named("groker", "grok"));
        assert!(!named("claude", "grok"));
    }
}
