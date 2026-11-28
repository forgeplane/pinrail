//! The terminal host: a process that runs agents in terminals and outlives
//! the app. The core finds it through its registration, connects to its
//! socket and speaks the framed protocol of [`frame`] with it.
//!
//! Agents run in PTYs on Unix and on pseudo consoles (ConPTY) on Windows.

pub mod client;
pub mod environment;
pub mod events;
#[cfg(unix)]
pub mod fdpass;
#[cfg(windows)]
#[path = "fdpass_windows.rs"]
pub mod fdpass;
pub mod filter;
pub mod frame;
#[cfg(feature = "server")]
pub mod holder;
#[cfg(feature = "server")]
pub mod holders;
pub mod launch;
pub mod leader;
pub mod link;
pub mod log;
pub mod net;
pub mod outbox;
pub mod paths;
pub mod probe;
pub mod programs;
pub mod protocol;
#[cfg(unix)]
pub mod pty;
#[cfg(windows)]
#[path = "pty_windows.rs"]
pub mod pty;
mod pty_common;
pub mod records;
pub mod registration;
pub mod ring;
#[cfg(feature = "server")]
pub mod server;
#[cfg(unix)]
mod sys;
#[cfg(windows)]
#[path = "sys_windows.rs"]
mod sys;
#[cfg(feature = "server")]
pub mod terminal;
pub mod viewers;
#[cfg(windows)]
mod win;

/// The protocol generation: the framing, `hello`, `host.info`,
/// `terminal.list` and `host.stop`, which never change within one.
pub const GENERATION: u32 = 1;

/// "pinrail-host/<version>"
pub fn name() -> String {
    format!("pinrail-host/{}", env!("CARGO_PKG_VERSION"))
}

/// The current time, as the protocol and the registration write it.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

/// `bytes` random bytes, hex-encoded.
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("the operating system's random numbers");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub use sys::boot_id;

/// Makes the folder `dir` its owner's alone on Windows: a protected ACL of
/// this user and the system, which what is made in it inherits, as mode
/// 0700 is on Unix, where the caller sets that.
#[cfg(windows)]
pub fn make_private(dir: &std::path::Path) -> std::io::Result<()> {
    win::make_private(dir)
}

/// On Windows, keeps this process's standard streams from the processes
/// it starts; Unix closes them on exec already.
pub fn keep_own_output_from_children() {
    #[cfg(windows)]
    sys::keep_own_output_from_children();
}

/// `options` with the file readable by its owner only, as the host keeps
/// its registration, records and log.
pub(crate) fn private(options: &mut std::fs::OpenOptions) -> &mut std::fs::OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}
