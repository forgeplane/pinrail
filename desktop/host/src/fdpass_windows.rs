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
use windows_sys::Win32::Foundation::{
    DUPLICATE_CLOSE_SOURCE, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, INVALID_HANDLE_VALUE,
};
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
/// end of `stream`. When the message cannot be sent whole, the duplicates
/// already made are closed again, in the peer, which would otherwise keep
/// them open for as long as it runs.
pub fn send(stream: &Stream, value: &Value, handles: &[Raw]) -> io::Result<()> {
    if handles.len() > MAX_HANDLES {
        return Err(io::Error::other("too many handles for one message"));
    }
    let body = value.to_string().into_bytes();
    let mut message = (body.len() as u32).to_le_bytes().to_vec();
    message.extend(body);
    if handles.is_empty() {
        message.extend(0u32.to_le_bytes());
        return (&*stream).write_all(&message);
    }
    let pid = stream.peer_pid()?;
    // SAFETY: the handle is owned below.
    let peer = unsafe { OpenProcess(PROCESS_DUP_HANDLE, 0, pid) };
    if peer.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `peer` is open and owned here.
    let peer = unsafe { OwnedHandle::from_raw_handle(peer as RawHandle) };
    let mut theirs = Vec::with_capacity(handles.len());
    let sent = (|| {
        for &handle in handles {
            let mut copy: HANDLE = std::ptr::null_mut();
            // SAFETY: duplicates a handle this process holds into the peer;
            // the value is meaningful only there.
            check(unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    handle as HANDLE,
                    crate::win::raw(&peer),
                    &mut copy,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            })?;
            theirs.push(copy);
        }
        message.extend((theirs.len() as u32).to_le_bytes());
        for copy in &theirs {
            message.extend((*copy as u64).to_le_bytes());
        }
        (&*stream).write_all(&message)
    })();
    if sent.is_err() {
        for copy in theirs {
            // SAFETY: closes, in the peer, a copy this call made there and
            // no message names.
            unsafe {
                DuplicateHandle(
                    crate::win::raw(&peer),
                    copy,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                    0,
                    DUPLICATE_CLOSE_SOURCE,
                )
            };
        }
    }
    sent
}

/// Receives a message and the handles that came with it, or `None` when
/// the peer closed the connection.
pub fn recv(stream: &Stream) -> io::Result<Option<(Value, Vec<Owned>)>> {
    recv_up_to(stream, MAX_HANDLES)
}

/// Receives a message that must carry no handle: what a peer sends before
/// it has shown who it is. A message that names handles is refused without
/// taking any, so nothing of this process is closed for it.
pub fn recv_plain(stream: &Stream) -> io::Result<Option<Value>> {
    Ok(recv_up_to(stream, 0)?.map(|(value, _)| value))
}

/// Receives a message with at most `most` handles. The values are checked
/// before any is taken: none is null or INVALID_HANDLE_VALUE, which no
/// duplicate can be.
fn recv_up_to(stream: &Stream, most: usize) -> io::Result<Option<(Value, Vec<Owned>)>> {
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
    if count > most.min(MAX_HANDLES) {
        return Err(io::Error::other("more handles than this message may carry"));
    }
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        let mut value = [0u8; 8];
        reader.read_exact(&mut value)?;
        values.push(u64::from_le_bytes(value));
    }
    if values
        .iter()
        .any(|&v| v == 0 || v as usize as HANDLE == INVALID_HANDLE_VALUE)
    {
        return Err(io::Error::other("a message named no handle"));
    }
    let handles = values
        .into_iter()
        // SAFETY: the sender duplicated the handle into this process for
        // this message, and nothing else owns it.
        .map(|v| unsafe { OwnedHandle::from_raw_handle(v as usize as RawHandle) })
        .collect();
    let value = serde_json::from_slice(&body).map_err(io::Error::other)?;
    Ok(Some((value, handles)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Seek;

    /// Two connected ends of a pipe.
    fn pair() -> (Stream, Stream) {
        let path = std::path::PathBuf::from(format!(
            r"\\.\pipe\pinrail-fdpass-test-{}",
            crate::random_hex(4)
        ));
        let listener = crate::net::listen(&path).unwrap();
        let client = crate::net::connect(&path).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    /// The raw bytes of a message `{}` that names `handles`.
    fn naming(handles: &[u64]) -> Vec<u8> {
        let mut message = 2u32.to_le_bytes().to_vec();
        message.extend(b"{}");
        message.extend((handles.len() as u32).to_le_bytes());
        for handle in handles {
            message.extend(handle.to_le_bytes());
        }
        message
    }

    #[test]
    fn a_message_naming_no_handle_or_an_invalid_one_is_refused() {
        for bad in [0u64, u64::MAX] {
            let (client, server) = pair();
            (&client).write_all(&naming(&[bad])).unwrap();
            assert!(recv(&server).is_err(), "{bad:#x}");
        }
    }

    #[test]
    fn a_send_that_fails_leaves_no_handle_behind_in_the_peer() {
        use windows_sys::Win32::System::Threading::{CreateEventW, EVENT_ALL_ACCESS, OpenEventW};
        let (client, _server) = pair();
        // a named event, which lives as long as a handle to it does
        let name = crate::win::wide(std::ffi::OsStr::new(&format!(
            "pinrail-leak-test-{}",
            crate::random_hex(4)
        )));
        // SAFETY: a new event, owned below.
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, name.as_ptr()) };
        assert!(!event.is_null());
        // SAFETY: just created, and owned from here.
        let event = unsafe { OwnedHandle::from_raw_handle(event as RawHandle) };
        // the second cannot be duplicated, after the first was
        let result = send(&client, &json!({}), &[raw(&event), 0x7ff0 as RawHandle]);
        assert!(result.is_err());
        drop(event);
        // SAFETY: opens the event by name, if anything still holds it.
        let left = unsafe { OpenEventW(EVENT_ALL_ACCESS, 0, name.as_ptr()) };
        if !left.is_null() {
            // SAFETY: the handle just opened.
            unsafe { OwnedHandle::from_raw_handle(left as RawHandle) };
        }
        assert!(left.is_null(), "a copy of the event was left in the peer");
    }

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
