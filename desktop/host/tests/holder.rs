//! The holder keeps the PTYs of the host's terminals open: it spawns a
//! program when a host that knows the secret asks, hands over the master
//! descriptor, reports the exit without reaping, and gives the
//! descriptors to the next host that connects, so a program outlives the
//! host that started it. On Windows it keeps the pseudo consoles, and
//! what it hands over is each console's input and output and its job.

mod common;

use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use pinrail_host::fdpass::{self, Owned};
use pinrail_host::net::{self, Stream};
use pinrail_host::paths::Paths;
use pinrail_host::pty;
use pinrail_host::registration::Registration;
use serde_json::{Value, json};

use common::{Folders, command, folders};

const SECRET: &str = "holder-test-secret";

/// How many descriptors the host gets of a terminal: the PTY's master, or
/// on Windows the console's input, its output and its job.
const OF_A_TERMINAL: usize = if cfg!(windows) { 3 } else { 1 };

/// A holder of the test's folders, and its socket, with a registration
/// that holds the secret a host would.
struct Holder {
    child: Child,
    socket: std::path::PathBuf,
}

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn holder(folders: &Folders, idle_exit: u64) -> Holder {
    #[cfg(unix)]
    let paths = {
        // this user's id, as the owner of a folder the test made
        let uid =
            std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(&folders.runtime).unwrap());
        Paths::with_runtime_dir(&folders.data, Some(&folders.runtime), uid)
    };
    #[cfg(windows)]
    let paths = Paths::new(&folders.data);
    std::fs::create_dir_all(&paths.host_dir).unwrap();
    Registration {
        pid: std::process::id(),
        started_at: String::new(),
        boot_id: String::new(),
        version: String::new(),
        protocol: 1,
        features: vec![],
        socket: paths.socket(),
        secret: SECRET.to_string(),
        build: None,
    }
    .write(&paths.registration())
    .unwrap();
    let mut child = command(folders, &["hold", "--idle-exit", &idle_exit.to_string()])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let instance = line.trim().to_string();
    assert_eq!(instance.len(), 8, "{line:?}");
    Holder {
        child,
        socket: paths.holder_socket(&instance),
    }
}

/// A connection that has presented `secret`, and the holder's answer.
fn connect(holder: &Holder, secret: &str) -> (Stream, Value) {
    let stream = net::connect(&holder.socket).unwrap();
    fdpass::send(&stream, &json!({"secret": secret, "protocol": 1}), &[]).unwrap();
    let (answer, _) = fdpass::recv(&stream).unwrap().unwrap();
    (stream, answer)
}

fn request(stream: &Stream, value: Value, fds: &[fdpass::Raw]) -> (Value, Vec<Owned>) {
    fdpass::send(stream, &value, fds).unwrap();
    loop {
        let (answer, fds) = fdpass::recv(stream).unwrap().unwrap();
        if answer.get("event").is_none() {
            return (answer, fds);
        }
    }
}

/// The host's side of a terminal, from the first of the descriptors a
/// holder sent.
fn terminal(fds: &mut dyn Iterator<Item = Owned>) -> pty::Master {
    pty::master_from(0, fds, (80, 24)).unwrap()
}

fn spawn_request(folders: &Folders, terminal: &str, script: &str) -> Value {
    json!({"op": "spawn", "id": 1, "terminal": terminal, "argv": common::shell(script),
           "cwd": folders.runtime, "env": {"PATH": common::path()}, "cols": 80, "rows": 24})
}

/// Whether the process `pid` is still there for its parent to reap, as a
/// zombie once it has exited.
#[cfg(unix)]
fn exists(pid: u32) -> bool {
    common::alive(pid)
}

/// Whether a handle keeps the process `pid`, which keeps its id its own
/// after it has exited: it can be opened as long as one does.
#[cfg(windows)]
#[allow(unsafe_code)]
fn exists(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: takes no pointers; the handle is closed at once.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        !handle.is_null() && CloseHandle(handle) != 0
    }
}

#[test]
fn a_program_outlives_the_host_that_started_it() {
    let folders = folders();
    let holder = holder(&folders, 30);
    let (first, hello) = connect(&holder, SECRET);
    assert_eq!(hello["protocol"], 1);
    assert!(hello["instance"].is_string());

    let ring = Owned::from(tempfile::tempfile().unwrap());
    let (spawned, fds) = request(
        &first,
        spawn_request(
            &folders,
            "t_1",
            "echo ready; read line; echo got:$line; read again; exit 7",
        ),
        &[fdpass::raw(&ring)],
    );
    assert!(spawned["pid"].as_u64().unwrap() > 0, "{spawned}");
    assert_eq!(fds.len(), OF_A_TERMINAL);
    let master = terminal(&mut fds.into_iter());
    common::read_until(&master, "ready");

    // the host goes away; the program does not
    drop(master);
    drop(first);
    std::thread::sleep(Duration::from_millis(300));
    let (second, _) = connect(&holder, SECRET);
    let (listed, fds) = request(&second, json!({"op": "list", "id": 2}), &[]);
    assert_eq!(listed["terminals"][0]["terminal"], "t_1");
    assert_eq!(listed["terminals"][0]["ring"], true);
    assert!(listed["terminals"][0].get("exit").is_none());
    assert_eq!(fds.len(), OF_A_TERMINAL + 1, "the master and the ring");
    let mut fds = fds.into_iter();
    let master = terminal(&mut fds);
    common::type_into(&master, b"hello\n");
    common::read_until(&master, "got:hello");

    // the exit is reported, and the program stays unreaped until forgotten;
    // the PTY is read meanwhile, as the host does, since a program's exit
    // waits for its terminal's output to be read on macOS
    common::type_into(&master, b"bye\n");
    let drain = master.try_clone().unwrap();
    std::thread::spawn(move || {
        let (_waker, wake) = pty::wake_pair().unwrap();
        pty::read_loop(drain, wake, |_| true);
    });
    let deadline = Instant::now() + common::within(10);
    let event = loop {
        let (message, _) = fdpass::recv(&second).unwrap().unwrap();
        if message["event"] == "exited" {
            break message;
        }
        assert!(Instant::now() < deadline);
    };
    assert_eq!(event["terminal"], "t_1");
    assert_eq!(event["code"], 7);
    let pid = listed["terminals"][0]["pid"].as_u64().unwrap() as u32;
    // a pid exists until its process is reaped, as a zombie meanwhile
    assert!(exists(pid), "reaped before forget");
    let (forgot, _) = request(
        &second,
        json!({"op": "forget", "id": 3, "terminal": "t_1"}),
        &[],
    );
    assert_eq!(forgot["ok"], true);
    let (listed, _) = request(&second, json!({"op": "list", "id": 4}), &[]);
    assert_eq!(listed["terminals"], json!([]));
    assert!(!exists(pid), "not reaped on forget");
}

#[test]
fn an_exit_while_no_host_is_connected_is_reported_to_the_next() {
    let folders = folders();
    let holder = holder(&folders, 30);
    let (first, _) = connect(&holder, SECRET);
    let (_, fds) = request(
        &first,
        spawn_request(&folders, "t_1", "sleep 0.3; exit 3"),
        &[],
    );
    drop(fds);
    drop(first);
    // past the program's exit: Git's sh starts slowly on Windows
    std::thread::sleep(common::within(1));
    let (second, _) = connect(&holder, SECRET);
    let (event, _) = fdpass::recv(&second).unwrap().unwrap();
    assert_eq!(
        event,
        json!({"event": "exited", "terminal": "t_1", "code": 3})
    );
    let (listed, _) = request(&second, json!({"op": "list", "id": 2}), &[]);
    assert_eq!(listed["terminals"][0]["exit"], json!({"code": 3}));
}

/// What only the holder can do to a pseudo console on Windows: resize it,
/// and close it, which ends its program, whose exit it then reports.
#[cfg(windows)]
#[test]
fn a_holder_resizes_and_closes_a_console_when_asked() {
    let folders = folders();
    let holder = holder(&folders, 30);
    let (stream, _) = connect(&holder, SECRET);
    // an answer or an exit that does not come fails the test
    stream.set_read_timeout(Some(common::within(10))).unwrap();
    // a program that never ends unless its console is closed, whatever is
    // typed, and starts nothing while it waits: a program that starts as
    // the console closes is not told, and runs on
    let (spawned, fds) = request(
        &stream,
        spawn_request(
            &folders,
            "t_1",
            "echo ready; read line; echo size:$(stty size); while :; do read line; done",
        ),
        &[],
    );
    assert!(spawned["pid"].as_u64().is_some(), "{spawned}");
    assert_eq!(fds.len(), OF_A_TERMINAL);
    let master = terminal(&mut fds.into_iter());
    // the program is on its console by now: one that has yet to attach
    // is not told that the console closed
    common::read_until(&master, "ready");
    let resize = |id: u64, params: Value| {
        let mut request = json!({"op": "resize", "id": id});
        request
            .as_object_mut()
            .unwrap()
            .extend(params.as_object().unwrap().clone());
        request
    };
    let (answer, _) = request(
        &stream,
        resize(2, json!({"terminal": "t_1", "cols": 100, "rows": 30})),
        &[],
    );
    assert_eq!(answer["ok"], true, "{answer}");
    // the program finds its console that large
    common::type_into(&master, b"\n");
    common::read_until(&master, "size:30 100");
    let (answer, _) = request(&stream, resize(3, json!({"terminal": "t_1"})), &[]);
    assert_eq!(answer["error"], "invalid", "{answer}");
    // the answer, and the exit of the program, which goes with its
    // console, in whichever order they come
    let closed = Instant::now();
    fdpass::send(
        &stream,
        &json!({"op": "close", "id": 5, "terminal": "t_1"}),
        &[],
    )
    .unwrap();
    let (mut answered, mut exited) = (false, false);
    while !(answered && exited) {
        let (message, _) = fdpass::recv(&stream)
            .expect("the close was not answered or ended no program")
            .unwrap();
        if message["event"] == "exited" {
            assert_eq!(message["terminal"], "t_1");
            // ended, not done
            assert!(
                message["code"].as_i64().is_some_and(|code| code != 0),
                "{message}"
            );
            exited = true;
        } else if message.get("event").is_none() {
            assert_eq!(message["ok"], true, "{message}");
            answered = true;
        }
    }
    assert!(
        closed.elapsed() < common::within(10),
        "the program ended {:?} after the close",
        closed.elapsed()
    );
}

#[test]
fn a_process_without_the_secret_is_refused() {
    let folders = folders();
    let holder = holder(&folders, 30);
    let (stream, answer) = connect(&holder, "wrong");
    assert_eq!(answer["error"], "forbidden");
    assert!(fdpass::recv(&stream).unwrap().is_none());
}

#[test]
fn a_spawn_that_cannot_happen_says_why() {
    let folders = folders();
    let holder = holder(&folders, 30);
    let (stream, _) = connect(&holder, SECRET);
    let mut missing = spawn_request(&folders, "t_1", "true");
    missing["cwd"] = json!("/no/such/folder");
    assert_eq!(request(&stream, missing, &[]).0["error"], "folder_missing");
    let mut unknown = spawn_request(&folders, "t_2", "true");
    unknown["argv"] = json!(["no-such-program-anywhere"]);
    assert_eq!(request(&stream, unknown, &[]).0["error"], "not_found");
}

#[test]
fn a_holder_with_nothing_to_hold_exits() {
    let folders = folders();
    let mut holder = holder(&folders, 1);
    let deadline = Instant::now() + common::within(10);
    loop {
        if let Some(status) = holder.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "the holder kept running");
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!holder.socket.exists());
}

/// A host that hangs: it asks and reads no answer, with its end of the
/// connection open. The answers fill the connection, and the holder's
/// write of the next one waits for room that never comes.
#[test]
fn a_host_that_stops_reading_does_not_keep_the_next_host_out() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    let folders = folders();
    let holder = holder(&folders, 30);
    let (hung, _) = connect(&holder, SECRET);
    let asked = Arc::new(AtomicU64::new(0));
    {
        let (hung, asked) = (hung.try_clone().unwrap(), asked.clone());
        std::thread::spawn(move || {
            while fdpass::send(&hung, &json!({"op": "list", "id": 1}), &[]).is_ok() {
                asked.fetch_add(1, Ordering::Relaxed);
            }
        });
    }
    // until the holder takes no more requests: it waits to write
    let deadline = Instant::now() + common::within(30);
    let mut before = 0;
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let now = asked.load(Ordering::Relaxed);
        if now > 0 && now == before {
            break;
        }
        before = now;
        assert!(
            Instant::now() < deadline,
            "the holder never waited to write"
        );
    }

    // the next host is let in, and served
    let next = net::connect(&holder.socket).unwrap();
    next.set_read_timeout(Some(common::within(10))).unwrap();
    fdpass::send(&next, &json!({"secret": SECRET, "protocol": 1}), &[]).unwrap();
    let hello = fdpass::recv(&next).expect("the holder let no other host in");
    assert_eq!(hello.unwrap().0["protocol"], 1);
    let (listed, _) = request(&next, json!({"op": "list", "id": 2}), &[]);
    assert_eq!(listed["terminals"], json!([]));
    drop(hung);
}
