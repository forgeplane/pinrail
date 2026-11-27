//! The terminal host: a process that runs agents in terminals and outlives
//! the app. The core finds it through its registration, connects to its
//! socket and speaks the framed protocol of [`frame`] with it.
//!
//! On Windows, the connections and the registration are there; the
//! terminals themselves still run only on Unix.

pub mod client;
#[cfg(unix)]
pub mod environment;
pub mod events;
#[cfg(unix)]
pub mod fdpass;
pub mod filter;
pub mod frame;
#[cfg(all(unix, feature = "server"))]
pub mod holder;
#[cfg(all(unix, feature = "server"))]
pub mod holders;
#[cfg(unix)]
pub mod launch;
#[cfg(unix)]
pub mod leader;
#[cfg(unix)]
pub mod link;
pub mod log;
pub mod net;
pub mod outbox;
pub mod paths;
#[cfg(unix)]
pub mod probe;
#[cfg(unix)]
pub mod programs;
pub mod protocol;
#[cfg(unix)]
pub mod pty;
pub mod records;
pub mod registration;
#[cfg(unix)]
pub mod ring;
#[cfg(all(unix, feature = "server"))]
pub mod server;
#[cfg(unix)]
mod sys;
#[cfg(windows)]
#[path = "sys_windows.rs"]
mod sys;
#[cfg(all(unix, feature = "server"))]
pub mod terminal;
#[cfg(unix)]
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
