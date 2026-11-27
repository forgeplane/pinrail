//! Messages between the host and a holder over a pipe: a JSON value,
//! length-prefixed, that can carry open handles, as `fdpass.rs` carries
//! descriptors on Unix. A sender duplicates each handle into the process at
//! the other end, whose pipe says who it is, and writes the new values
//! after the message: `[len u32][json][count u32][count × u64]`. The
//! receiver owns them from then on.

// DuplicateHandle and handles taken from their values can only be used
// with `unsafe`; each use says why it is sound.
#![allow(unsafe_code)]

use std::io::{self, Read, Write};
use std::os::windows::io::{FromRawHandle, OwnedHandle, RawHandle};

use serde_json::Value;
use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE};

use crate::net::Stream;
use crate::win::check;

/// What a message carries, owned.
pub type Owned = OwnedHandle;
/// What a message is given to carry.
pub type Raw = RawHandle;

pub fn raw(owned: &Owned) -> Raw {
    use std::os::windows::io::AsRawHandle;
    owned.as_raw_handle()
}

/// The most handles one message carries: four per terminal, for the 64
/// terminals a host runs, with room to spare.
pub const MAX_HANDLES: usize = 320;
/// The longest message, so that a broken peer cannot ask for any amount of
/// memory.
const MAX_MESSAGE: usize = 16 << 20;

/// Sends `value` with duplicates of `handles` in the process at the other
/// end of `stream`.
pub fn send(stream: &Stream, value: &Value, handles: &[Raw]) -> io::Result<()> {
    if handles.len() > MAX_HANDLES {
        return Err(io::Error::other("too many handles for one message"));
    }
    let mut values = Vec::with_capacity(handles.len());
    if !handles.is_empty() {
        let pid = stream.peer_pid()?;
        // SAFETY: the handle is owned below.
        let peer = unsafe { OpenProcess(PROCESS_DUP_HANDLE, 0, pid) };
        if peer.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `peer` is open and owned here.
        let peer = unsafe { OwnedHandle::from_raw_handle(peer as RawHandle) };
        for &handle in handles {
            let mut theirs: HANDLE = std::ptr::null_mut();
            // SAFETY: duplicates a handle this process holds into the peer;
            // the value is meaningful only there.
            check(unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    handle as HANDLE,
                    crate::win::raw(&peer),
                    &mut theirs,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            })?;
            values.push(theirs as u64);
        }
    }
    let body = value.to_string().into_bytes();
    let mut message = (body.len() as u32).to_le_bytes().to_vec();
    message.extend(body);
    message.extend((values.len() as u32).to_le_bytes());
    for value in values {
        message.extend(value.to_le_bytes());
    }
    (&*stream).write_all(&message)
}

/// Receives a message and the handles that came with it, or `None` when
/// the peer closed the connection.
pub fn recv(stream: &Stream) -> io::Result<Option<(Value, Vec<Owned>)>> {
    let mut reader = stream;
    let mut header = [0u8; 4];
    match reader.read(&mut header)? {
        0 => return Ok(None),
        n if n < header.len() => reader.read_exact(&mut header[n..])?,
        _ => {}
    }
    let len = u32::from_le_bytes(header) as usize;
    if len > MAX_MESSAGE {
        return Err(io::Error::other("message too long"));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    let mut count = [0u8; 4];
    reader.read_exact(&mut count)?;
    let count = u32::from_le_bytes(count) as usize;
    if count > MAX_HANDLES {
        return Err(io::Error::other("too many handles in a message"));
    }
    let mut handles = Vec::with_capacity(count);
    for _ in 0..count {
        let mut value = [0u8; 8];
        reader.read_exact(&mut value)?;
        // SAFETY: the sender duplicated the handle into this process for
        // this message, and nothing else owns it.
        handles.push(unsafe {
            OwnedHandle::from_raw_handle(u64::from_le_bytes(value) as usize as RawHandle)
        });
    }
    let value = serde_json::from_slice(&body).map_err(io::Error::other)?;
    Ok(Some((value, handles)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Seek;

    #[test]
    fn a_message_carries_handles_that_stay_open_on_the_other_side() {
        let path = std::path::PathBuf::from(format!(
            r"\\.\pipe\pinrail-fdpass-test-{}",
            crate::random_hex(4)
        ));
        let listener = crate::net::listen(&path).unwrap();
        let client = crate::net::connect(&path).unwrap();
        let (server, _) = listener.accept().unwrap();
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(b"shared").unwrap();
        let owned = OwnedHandle::from(file);
        send(&client, &json!({"op": "spawn", "n": 1}), &[raw(&owned)]).unwrap();
        send(&client, &json!({"op": "list"}), &[]).unwrap();
        drop(owned);
        let (value, handles) = recv(&server).unwrap().unwrap();
        assert_eq!(value, json!({"op": "spawn", "n": 1}));
        assert_eq!(handles.len(), 1);
        let mut received = std::fs::File::from(handles.into_iter().next().unwrap());
        received.rewind().unwrap();
        let mut text = String::new();
        received.read_to_string(&mut text).unwrap();
        assert_eq!(text, "shared");
        let (value, handles) = recv(&server).unwrap().unwrap();
        assert_eq!(value, json!({"op": "list"}));
        assert!(handles.is_empty());
        drop(client);
        assert!(recv(&server).unwrap().is_none());
    }
}
