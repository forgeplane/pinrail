//! `pinrail attach` in a pseudo-terminal, against a mock core that answers
//! the run and ticket requests and serves the attach WebSocket: the
//! snapshot and the output are drawn, credit is returned, keys go to the
//! agent, and Ctrl-\ then d detaches without reaching it. On Windows the
//! terminal is a pseudo console, as Windows Terminal gives a program.
#![allow(unsafe_code)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tungstenite::Message;

/// What the mock core's socket heard from the CLI.
#[derive(Debug, Default)]
struct Heard {
    input: Vec<u8>,
    credit: u64,
    texts: Vec<Value>,
    query: String,
}

/// Serves one attach: the HTTP requests before it, then the socket.
fn mock_core(listener: TcpListener, heard: mpsc::Sender<Heard>) {
    mock_core_with(listener, heard, None);
}

/// [`mock_core`], whose terminal also writes `late` a while after the
/// screen, as a program goes on writing.
fn mock_core_with(listener: TcpListener, heard: mpsc::Sender<Heard>, late: Option<&'static [u8]>) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { break };
        let mut peek = [0u8; 512];
        let n = stream.peek(&mut peek).unwrap_or(0);
        let head = String::from_utf8_lossy(&peek[..n]).to_string();
        if head.contains("/attach?") {
            let query = head
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            heard.send(serve_socket(stream, query, late)).unwrap();
            return;
        }
        let mut request = Vec::new();
        let mut buf = [0u8; 4096];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut buf).unwrap();
            request.extend(&buf[..n]);
        }
        let line = String::from_utf8_lossy(&request)
            .lines()
            .next()
            .unwrap()
            .to_string();
        let body = if line.starts_with("GET /api/v1/runs/run_1") {
            json!({"id": "run_1", "state": "running", "terminal": "local:t_1"})
        } else if line.starts_with("POST /api/v1/terminals/local%3At_1/tickets") {
            json!({"ticket": "tk", "expires_in": 30})
        } else {
            json!({"error": "not_found", "message": line, "violations": []})
        }
        .to_string();
        let status = if body.contains("not_found") { 404 } else { 200 };
        write!(
            stream,
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    }
}

fn serve_socket(stream: TcpStream, query: String, late: Option<&'static [u8]>) -> Heard {
    let mut socket = tungstenite::accept(stream).unwrap();
    let text = |value: Value| Message::Text(value.to_string().into());
    socket
        .send(text(json!({"t": "hello", "v": 1, "terminal": "local:t_1", "cols": 80, "rows": 24, "role": "observer"})))
        .unwrap();
    socket
        .send(text(
            json!({"t": "snapshot_begin", "format": "vt", "cut": 0, "cols": 80, "rows": 24}),
        ))
        .unwrap();
    socket
        .send(Message::Binary(b"\x02SCREEN".to_vec().into()))
        .unwrap();
    socket.send(text(json!({"t": "snapshot_end"}))).unwrap();
    let mut output = vec![0x01];
    output.extend(0u64.to_le_bytes());
    output.extend(b" then output");
    socket.send(Message::Binary(output.into())).unwrap();
    if let Some(late) = late {
        std::thread::sleep(Duration::from_millis(1_500));
        let mut output = vec![0x01];
        output.extend((b"SCREEN then output".len() as u64).to_le_bytes());
        output.extend(late);
        socket.send(Message::Binary(output.into())).unwrap();
    }
    let mut heard = Heard {
        query,
        ..Heard::default()
    };
    stream_timeout(&socket);
    loop {
        match socket.read() {
            Ok(Message::Binary(bytes)) if bytes.first() == Some(&0x10) => {
                heard.input.extend(&bytes[1..]);
            }
            Ok(Message::Text(message)) => {
                let value: Value = serde_json::from_str(&message).unwrap();
                if value["t"] == "credit" {
                    heard.credit += value["bytes"].as_u64().unwrap();
                }
                heard.texts.push(value);
            }
            Ok(Message::Close(_)) | Err(_) => return heard,
            Ok(_) => {}
        }
    }
}

fn stream_timeout(socket: &tungstenite::WebSocket<TcpStream>) {
    socket
        .get_ref()
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
}

/// `pinrail attach run_1` on a terminal of the test's own: what it draws
/// arrives on `output`, and `input` is typed into it.
struct Attached {
    output: mpsc::Receiver<Vec<u8>>,
    input: Box<dyn Write>,
    /// Waits for the command to end; whether it succeeded.
    wait: Box<dyn FnOnce() -> bool>,
}

/// Reads `reader` on a thread of its own, into a channel.
fn pump(mut reader: impl Read + Send + 'static) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    rx
}

/// On a pseudo-terminal: the side the test reads and writes, and the side
/// the CLI runs on.
#[cfg(unix)]
fn attach(url: &str, data: &std::path::Path) -> Attached {
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::process::{Command, Stdio};
    let (mut leader, mut follower) = (0, 0);
    // SAFETY: openpty writes two descriptors, which the test then owns.
    let ok = unsafe {
        libc::openpty(
            &mut leader,
            &mut follower,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(ok, 0);
    // SAFETY: both descriptors are new and owned by nothing else.
    let (terminal, follower) = unsafe {
        (
            std::fs::File::from_raw_fd(leader),
            OwnedFd::from_raw_fd(follower),
        )
    };
    let child = Command::new(env!("CARGO_BIN_EXE_pinrail"))
        .arg("attach")
        .arg("run_1")
        .env("PINRAIL_URL", url)
        .env("PINRAIL_DATA_DIR", data)
        .env_remove("PINRAIL_JSON")
        .stdin(Stdio::from(follower.try_clone().unwrap()))
        .stdout(Stdio::from(follower.try_clone().unwrap()))
        .stderr(Stdio::from(follower))
        .spawn()
        .unwrap();
    Attached {
        output: pump(terminal.try_clone().unwrap()),
        input: Box::new(terminal),
        wait: Box::new(move || child.wait_with_output().unwrap().status.success()),
    }
}

/// On a pseudo console: the pipes the test reads and writes, and the
/// console the CLI runs on.
#[cfg(windows)]
fn attach(url: &str, data: &std::path::Path) -> Attached {
    use std::os::windows::io::{FromRawHandle, OwnedHandle, RawHandle};
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Console::{
        COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON,
    };
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::System::Threading::{
        CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
        EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE,
        InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
        PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, STARTF_USESTDHANDLES,
        STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
    };
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    // SAFETY: each call gets valid, live arguments; every handle made is
    // owned by a value that closes it, the console by the waiter.
    unsafe {
        let (mut in_read, mut in_write, mut out_read, mut out_write): (
            HANDLE,
            HANDLE,
            HANDLE,
            HANDLE,
        ) = (
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        assert_ne!(
            CreatePipe(&mut in_read, &mut in_write, std::ptr::null(), 0),
            0
        );
        assert_ne!(
            CreatePipe(&mut out_read, &mut out_write, std::ptr::null(), 0),
            0
        );
        let mut console: HPCON = 0;
        assert_eq!(
            CreatePseudoConsole(COORD { X: 80, Y: 24 }, in_read, out_write, 0, &mut console),
            0
        );
        let mut needed = 0usize;
        InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut needed);
        let mut attributes = vec![0u8; needed];
        let list = attributes.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
        assert_ne!(
            InitializeProcThreadAttributeList(list, 1, 0, &mut needed),
            0
        );
        assert_ne!(
            UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                console as *const std::ffi::c_void,
                size_of::<HPCON>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            ),
            0
        );
        let mut startup: STARTUPINFOEXW = std::mem::zeroed();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.lpAttributeList = list;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
        startup.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
        let mut env: Vec<(String, String)> = std::env::vars()
            .filter(|(k, _)| {
                !["PINRAIL_URL", "PINRAIL_DATA_DIR", "PINRAIL_JSON"]
                    .contains(&k.to_uppercase().as_str())
            })
            .collect();
        env.push(("PINRAIL_URL".into(), url.into()));
        env.push(("PINRAIL_DATA_DIR".into(), data.display().to_string()));
        env.sort_by_key(|(k, _)| k.to_uppercase());
        let mut block: Vec<u16> = Vec::new();
        for (k, v) in env {
            block.extend(format!("{k}={v}").encode_utf16());
            block.push(0);
        }
        block.push(0);
        let mut line = wide(&format!(
            "\"{}\" attach run_1",
            env!("CARGO_BIN_EXE_pinrail")
        ));
        let mut info: PROCESS_INFORMATION = std::mem::zeroed();
        let created = CreateProcessW(
            std::ptr::null(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            block.as_mut_ptr().cast(),
            std::ptr::null(),
            &startup.StartupInfo,
            &mut info,
        );
        DeleteProcThreadAttributeList(list);
        assert_ne!(created, 0, "{}", std::io::Error::last_os_error());
        // the console holds its own ends now
        drop((
            OwnedHandle::from_raw_handle(in_read as RawHandle),
            OwnedHandle::from_raw_handle(out_write as RawHandle),
            OwnedHandle::from_raw_handle(info.hThread as RawHandle),
        ));
        let process = OwnedHandle::from_raw_handle(info.hProcess as RawHandle);
        Attached {
            output: pump(std::fs::File::from_raw_handle(out_read as RawHandle)),
            input: Box::new(std::fs::File::from_raw_handle(in_write as RawHandle)),
            wait: Box::new(move || {
                use std::os::windows::io::AsRawHandle;
                let h = process.as_raw_handle() as HANDLE;
                WaitForSingleObject(h, INFINITE);
                let mut code = 1u32;
                GetExitCodeProcess(h, &mut code);
                // the console goes once its program has, which ends the output
                ClosePseudoConsole(console);
                code == 0
            }),
        }
    }
}

/// Reads what the CLI draws until it contains `wanted`.
fn read_until(output: &mpsc::Receiver<Vec<u8>>, seen: &mut Vec<u8>, wanted: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !seen.windows(wanted.len()).any(|w| w == wanted) {
        assert!(
            Instant::now() < deadline,
            "{:?} never came: {:?}",
            String::from_utf8_lossy(wanted),
            String::from_utf8_lossy(seen)
        );
        if let Ok(bytes) = output.recv_timeout(Duration::from_millis(50)) {
            seen.extend(bytes);
        }
    }
}

/// A key that types nothing, such as Volume Up, which a console still reports:
/// what the terminal writes after it keeps coming.
#[cfg(windows)]
#[test]
fn a_key_that_types_nothing_holds_up_no_output() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, heard) = mpsc::channel();
    std::thread::spawn(move || mock_core_with(listener, tx, Some(b" LATE")));
    let data = std::env::temp_dir().join(format!("pinrail-attach-pause-{}", std::process::id()));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("person-token"), "tok").unwrap();
    let Attached {
        output,
        mut input,
        wait,
    } = attach(&url, &data);
    let mut seen = Vec::new();
    read_until(&output, &mut seen, b"SCREEN then output");
    // Volume Up and the menu key, pressed and let go, as win32-input-mode
    // writes a key: neither has a character
    input
        .write_all(
            b"\x1b[175;48;0;1;0;1_\x1b[175;48;0;0;0;1_\x1b[93;93;0;1;0;1_\x1b[93;93;0;0;0;1_",
        )
        .unwrap();
    read_until(&output, &mut seen, b" LATE");
    input.write_all(b"\x1c").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    input.write_all(b"d").unwrap();
    read_until(&output, &mut seen, b"[detached from run_1");
    assert!(wait());
    // the keys typed nothing to send
    let heard = heard.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(heard.input, b"");
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn attach_draws_the_terminal_sends_keys_and_detaches_on_ctrl_backslash_then_d() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, heard) = mpsc::channel();
    std::thread::spawn(move || mock_core(listener, tx));
    let data = std::env::temp_dir().join(format!("pinrail-attach-test-{}", std::process::id()));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("person-token"), "tok").unwrap();

    let Attached {
        output,
        mut input,
        wait,
    } = attach(&url, &data);
    let mut seen = Vec::new();
    // it asks this terminal for its colours, then draws the screen; a
    // pseudo console answers the attributes query itself, and is not asked
    if cfg!(unix) {
        read_until(&output, &mut seen, b"\x1b[c");
    }
    input
        .write_all(b"\x1b]11;rgb:1010/2020/3030\x1b\\\x1b[?62;22c")
        .unwrap();
    read_until(&output, &mut seen, b"SCREEN then output");
    input.write_all(b"ls\r").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    input.write_all(b"\x1c").unwrap();
    std::thread::sleep(Duration::from_millis(200));
    input.write_all(b"d").unwrap();
    read_until(&output, &mut seen, b"[detached from run_1");

    assert!(wait());
    let heard = heard.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(heard.query.contains("ticket=tk"), "{}", heard.query);
    assert!(heard.query.contains("formats=vt"), "{}", heard.query);
    // the keys, without the chord
    assert_eq!(heard.input, b"ls\r");
    // credit for the snapshot and the output
    assert_eq!(heard.credit, ("SCREEN".len() + " then output".len()) as u64);
    // the colours the terminal answered with; Windows' own pseudo console
    // drops an OSC reply typed into it, so there they may not come at all
    let facts = heard.texts.iter().find(|t| t["t"] == "facts");
    if cfg!(unix) || facts.is_some() {
        assert_eq!(facts.unwrap()["bg"], "#102030");
    }
    // the person's terminal is reset on the way out, which a pseudo console
    // draws rather than passes on
    if cfg!(unix) {
        assert!(seen.windows(8).any(|w| w == b"\x1b[?1004l"));
    }
    let _ = std::fs::remove_dir_all(&data);
}
