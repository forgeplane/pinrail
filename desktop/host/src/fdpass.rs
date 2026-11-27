//! Messages between the host and a holder over a Unix socket: a JSON
//! value, length-prefixed, that can carry open descriptors with
//! `SCM_RIGHTS`. The descriptors travel with the first byte of the message.

// sendmsg, recvmsg and the control-message macros can only be used with
// `unsafe`; each use says why it is sound.
#![allow(unsafe_code)]

use std::io::{self, Read};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;

use serde_json::Value;

/// What a message carries, owned.
pub type Owned = OwnedFd;
/// What a message is given to carry.
pub type Raw = RawFd;

pub fn raw(owned: &Owned) -> Raw {
    owned.as_raw_fd()
}

/// The most descriptors one message carries: two per terminal, for the
/// 64 terminals a host runs, with room to spare.
pub const MAX_FDS: usize = 160;
/// The longest message, so that a broken peer cannot ask for any amount of
/// memory.
const MAX_MESSAGE: usize = 16 << 20;

/// Sends `value` with `fds`.
pub fn send(stream: &UnixStream, value: &Value, fds: &[RawFd]) -> io::Result<()> {
    if fds.len() > MAX_FDS {
        return Err(io::Error::other("too many descriptors for one message"));
    }
    let body = value.to_string().into_bytes();
    let mut message = (body.len() as u32).to_le_bytes().to_vec();
    message.extend(body);
    let mut sent = 0;
    while sent < message.len() {
        let rest = &message[sent..];
        let n = if sent == 0 && !fds.is_empty() {
            send_with_fds(stream, rest, fds)?
        } else {
            send_plain(stream, rest)?
        };
        sent += n;
    }
    Ok(())
}

/// Receives a message and the descriptors that came with it, or `None` when
/// the peer closed the connection.
pub fn recv(stream: &UnixStream) -> io::Result<Option<(Value, Vec<OwnedFd>)>> {
    let mut header = [0u8; 4];
    let (n, fds) = recv_with_fds(stream, &mut header)?;
    if n == 0 {
        return Ok(None);
    }
    let mut stream_ref = stream;
    if n < header.len() {
        stream_ref.read_exact(&mut header[n..])?;
    }
    let len = u32::from_le_bytes(header) as usize;
    if len > MAX_MESSAGE {
        return Err(io::Error::other("message too long"));
    }
    let mut body = vec![0u8; len];
    stream_ref.read_exact(&mut body)?;
    let value = serde_json::from_slice(&body).map_err(io::Error::other)?;
    Ok(Some((value, fds)))
}

fn send_plain(stream: &UnixStream, bytes: &[u8]) -> io::Result<usize> {
    loop {
        // SAFETY: the buffer is valid for its length; MSG_NOSIGNAL keeps a
        // closed peer from raising SIGPIPE where it exists.
        let n = unsafe {
            libc::send(
                stream.as_raw_fd(),
                bytes.as_ptr().cast(),
                bytes.len(),
                NOSIGNAL,
            )
        };
        if n >= 0 {
            return Ok(n as usize);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

#[cfg(target_os = "linux")]
const NOSIGNAL: libc::c_int = libc::MSG_NOSIGNAL;
// macOS has no MSG_NOSIGNAL; Rust programs ignore SIGPIPE from the start
#[cfg(not(target_os = "linux"))]
const NOSIGNAL: libc::c_int = 0;

fn send_with_fds(stream: &UnixStream, bytes: &[u8], fds: &[RawFd]) -> io::Result<usize> {
    let fd_bytes = std::mem::size_of_val(fds);
    // SAFETY: CMSG_SPACE only computes a size.
    let space = unsafe { libc::CMSG_SPACE(fd_bytes as u32) } as usize;
    let mut control = vec![0u8; space];
    let mut iov = libc::iovec {
        iov_base: bytes.as_ptr() as *mut libc::c_void,
        iov_len: bytes.len(),
    };
    // SAFETY: msghdr is plain data; the fields set below point at buffers
    // that live through the call.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    msg.msg_controllen = space as _;
    // SAFETY: the control buffer holds CMSG_SPACE bytes, room for one
    // header and the descriptors, which are copied into its data.
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(fd_bytes as u32) as _;
        std::ptr::copy_nonoverlapping(fds.as_ptr().cast::<u8>(), libc::CMSG_DATA(cmsg), fd_bytes);
    }
    loop {
        // SAFETY: `msg` and the buffers it points to are valid.
        let n = unsafe { libc::sendmsg(stream.as_raw_fd(), &msg, NOSIGNAL) };
        if n >= 0 {
            return Ok(n as usize);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

fn recv_with_fds(stream: &UnixStream, buf: &mut [u8]) -> io::Result<(usize, Vec<OwnedFd>)> {
    // SAFETY: CMSG_SPACE only computes a size.
    let space =
        unsafe { libc::CMSG_SPACE((MAX_FDS * std::mem::size_of::<RawFd>()) as u32) } as usize;
    let mut control = vec![0u8; space];
    let mut iov = libc::iovec {
        iov_base: buf.as_mut_ptr().cast(),
        iov_len: buf.len(),
    };
    // SAFETY: msghdr is plain data; the fields set below point at buffers
    // that live through the call.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    msg.msg_controllen = space as _;
    let n = loop {
        // SAFETY: `msg` and the buffers it points to are valid; descriptors
        // received are closed on exec.
        let n = unsafe { libc::recvmsg(stream.as_raw_fd(), &mut msg, CLOEXEC) };
        if n >= 0 {
            break n as usize;
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    };
    let mut fds = Vec::new();
    // SAFETY: the headers and their data lie within the control buffer
    // recvmsg filled, as the macros walk it; each descriptor received is
    // new and owned by nothing else.
    unsafe {
        let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let data = libc::CMSG_DATA(cmsg);
                let len = (*cmsg).cmsg_len as usize - (data as usize - cmsg as usize);
                for i in 0..len / std::mem::size_of::<RawFd>() {
                    let fd = std::ptr::read_unaligned(data.cast::<RawFd>().add(i));
                    fds.push(OwnedFd::from_raw_fd(fd));
                }
            }
            cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
        }
    }
    #[cfg(not(target_os = "linux"))]
    for fd in &fds {
        // macOS has no MSG_CMSG_CLOEXEC
        // SAFETY: fcntl on a descriptor this process owns.
        unsafe {
            let flags = libc::fcntl(fd.as_raw_fd(), libc::F_GETFD);
            libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }
    }
    Ok((n, fds))
}

#[cfg(target_os = "linux")]
const CLOEXEC: libc::c_int = libc::MSG_CMSG_CLOEXEC;
#[cfg(not(target_os = "linux"))]
const CLOEXEC: libc::c_int = 0;

/// The user id of the process at the other end of `stream`.
pub fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: ucred is plain data, written by getsockopt within its size.
        let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: as above.
        let result = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&raw mut cred).cast(),
                &mut len,
            )
        };
        if result == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(cred.uid)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let (mut uid, mut gid) = (0, 0);
        // SAFETY: both out-pointers are valid.
        if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(uid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{Seek, Write};

    #[test]
    fn a_message_carries_descriptors_that_stay_open_on_the_other_side() {
        let (a, b) = UnixStream::pair().unwrap();
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(b"shared").unwrap();
        send(&a, &json!({"op": "spawn", "n": 1}), &[file.as_raw_fd()]).unwrap();
        drop(file);
        let (value, fds) = recv(&b).unwrap().unwrap();
        assert_eq!(value, json!({"op": "spawn", "n": 1}));
        assert_eq!(fds.len(), 1);
        let mut received = std::fs::File::from(fds.into_iter().next().unwrap());
        received.rewind().unwrap();
        let mut text = String::new();
        received.read_to_string(&mut text).unwrap();
        assert_eq!(text, "shared");
    }

    #[test]
    fn messages_without_descriptors_and_a_closed_peer() {
        let (a, b) = UnixStream::pair().unwrap();
        // larger than the socket's buffer, so it is sent in pieces
        let sender = std::thread::spawn(move || {
            send(&a, &json!({"op": "list"}), &[]).unwrap();
            send(&a, &json!({"big": "x".repeat(1_000_000)}), &[]).unwrap();
        });
        assert_eq!(recv(&b).unwrap().unwrap().0, json!({"op": "list"}));
        assert_eq!(
            recv(&b).unwrap().unwrap().0["big"].as_str().unwrap().len(),
            1_000_000
        );
        sender.join().unwrap();
        assert!(recv(&b).unwrap().is_none());
        assert_eq!(peer_uid(&b).unwrap_or(crate::sys::uid()), crate::sys::uid());
    }
}
