//! The terminal host: a process that runs agents in terminals and outlives
//! the app. The core finds it through its registration, connects to its
//! socket and speaks the framed protocol of [`frame`] with it.
//!
//! The host runs only on Unix, where agents run in PTYs.
#![cfg(unix)]

pub mod client;
pub mod environment;
pub mod events;
pub mod frame;
pub mod launch;
pub mod log;
pub mod paths;
pub mod probe;
pub mod pty;
pub mod records;
pub mod registration;
pub mod server;
mod sys;
pub mod terminal;

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
