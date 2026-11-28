//! The connections between the core, the host and the holders: Unix
//! sockets on Unix, named pipes on Windows, behind one surface.
//!
//! - [`Stream`]: a blocking connection that one thread can read while
//!   another writes, with a read timeout.
//! - [`Listener`]: blocking accepts, or non-blocking ones that answer
//!   [`io::ErrorKind::WouldBlock`] while no one connects.
//! - [`AsyncListener`], [`connect_async`] and [`split`]: the same for the
//!   host's and the core's tokio runtimes.
//! - [`same_user`]: whether the other end runs as this user.
//!
//! A pipe can only be opened by this user and the system, and a client
//! checks that the pipe's server runs as this user too, so that no other
//! account can take a pipe's name first and receive what is sent to it.

pub use imp::*;

#[cfg(unix)]
mod imp {
    use std::io;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    pub use std::os::unix::net::{UnixListener as Listener, UnixStream as Stream};

    /// Connects to the socket at `path`.
    pub fn connect(path: &Path) -> io::Result<Stream> {
        Stream::connect(path)
    }

    /// Listens on `path`, readable by this user only. A socket left there
    /// by a process that ended is replaced: the caller holds the lock that
    /// says no other listens.
    pub fn listen(path: &Path) -> io::Result<Listener> {
        let _ = std::fs::remove_file(path);
        let listener = Listener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(listener)
    }

    /// Removes what [`listen`] left at `path`.
    pub fn remove(path: &Path) {
        let _ = std::fs::remove_file(path);
    }

    /// Whether the other end of `stream` runs as this user.
    pub fn same_user(stream: &Stream) -> bool {
        crate::fdpass::peer_uid(stream).is_ok_and(|uid| uid == crate::sys::uid())
    }

    pub type AsyncStream = tokio::net::UnixStream;

    pub struct AsyncListener(tokio::net::UnixListener);

    impl AsyncListener {
        /// As [`listen`], for a runtime. Must be called inside one.
        pub fn bind(path: &Path) -> io::Result<Self> {
            let _ = std::fs::remove_file(path);
            let listener = tokio::net::UnixListener::bind(path)?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            Ok(Self(listener))
        }

        pub async fn accept(&mut self) -> io::Result<AsyncStream> {
            self.0.accept().await.map(|(stream, _)| stream)
        }
    }

    pub async fn connect_async(path: &Path) -> io::Result<AsyncStream> {
        AsyncStream::connect(path).await
    }

    /// A connection's two directions, each usable on its own task.
    pub fn split(
        stream: AsyncStream,
    ) -> (
        tokio::net::unix::OwnedReadHalf,
        tokio::net::unix::OwnedWriteHalf,
    ) {
        stream.into_split()
    }
}

#[cfg(windows)]
mod imp {
    // Named pipes, overlapped I/O and security descriptors can only be
    // used with `unsafe`; each use says why it is sound.
    #![allow(unsafe_code)]

    use std::io::{self, Read, Write};
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
    use std::path::Path;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{
        ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_NO_DATA, ERROR_OPERATION_ABORTED,
        ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED, GENERIC_READ,
        GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
        PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WriteFile,
    };
    use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
        GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
    };
    use windows_sys::Win32::System::Threading::{
        INFINITE, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
    };

    use crate::win::{check, event, raw, wide};

    /// One end of a pipe, opened for overlapped I/O: a read waiting in one
    /// thread does not hold up a write from another, as it would on a
    /// handle opened for synchronous I/O.
    struct Pipe {
        handle: OwnedHandle,
        /// Set once the connection is shut down: reads end at once.
        stopped: OwnedHandle,
        read_timeout: Mutex<Option<Duration>>,
        server: bool,
        /// Set once this end has shut down its writing.
        written: AtomicBool,
    }

    impl Pipe {
        fn new(handle: HANDLE, server: bool) -> io::Result<Self> {
            // SAFETY: the caller passes a handle it opened and gives up.
            let handle = unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) };
            Ok(Self {
                handle,
                stopped: event()?,
                read_timeout: Mutex::new(None),
                server,
                written: AtomicBool::new(false),
            })
        }

        /// One overlapped operation, waited for. A read also ends when the
        /// connection is shut down, and after the read timeout. The buffer
        /// the operation uses lives until it is complete or cancelled.
        fn io(
            &self,
            reading: bool,
            op: impl FnOnce(HANDLE, *mut OVERLAPPED) -> windows_sys::core::BOOL,
        ) -> io::Result<usize> {
            let h = raw(&self.handle);
            let stopped = raw(&self.stopped);
            // SAFETY: waits on an event this value owns, without blocking.
            if reading && unsafe { WaitForSingleObject(stopped, 0) } == WAIT_OBJECT_0 {
                return Ok(0);
            }
            let done = event()?;
            // SAFETY: OVERLAPPED is plain data; zero is its initial state.
            let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
            ov.hEvent = raw(&done);
            if op(h, &mut ov) == 0 {
                // SAFETY: reads this thread's last error.
                let error = unsafe { GetLastError() };
                if error != ERROR_IO_PENDING {
                    return match error {
                        ERROR_BROKEN_PIPE | ERROR_NO_DATA | ERROR_PIPE_NOT_CONNECTED => Ok(0),
                        _ => Err(io::Error::from_raw_os_error(error as i32)),
                    };
                }
                if reading {
                    let timeout = self
                        .read_timeout
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .map_or(INFINITE, |t| {
                            t.as_millis().min(u128::from(INFINITE - 1)) as u32
                        });
                    // SAFETY: both events are open for the wait.
                    let woke = unsafe {
                        WaitForMultipleObjects(2, [stopped, ov.hEvent].as_ptr(), 0, timeout)
                    };
                    if woke != WAIT_OBJECT_0 + 1 {
                        // the wait's own error, before the calls below
                        // replace it
                        let failed = io::Error::last_os_error();
                        // stopped or timed out: cancel, and wait until the
                        // operation lets go of the buffer
                        let mut n = 0u32;
                        // SAFETY: cancels this operation on our handle, then
                        // waits for it to finish.
                        let completed = unsafe {
                            CancelIoEx(h, &ov);
                            GetOverlappedResult(h, &ov, &mut n, 1)
                        };
                        if completed != 0 && n > 0 {
                            return Ok(n as usize);
                        }
                        return match woke {
                            WAIT_OBJECT_0 => Ok(0),
                            WAIT_TIMEOUT => Err(io::Error::from(io::ErrorKind::WouldBlock)),
                            _ => Err(failed),
                        };
                    }
                }
            }
            let mut n = 0u32;
            // SAFETY: waits for the operation started above to complete.
            if unsafe { GetOverlappedResult(h, &ov, &mut n, 1) } == 0 {
                // SAFETY: reads this thread's last error.
                return match unsafe { GetLastError() } {
                    ERROR_BROKEN_PIPE
                    | ERROR_NO_DATA
                    | ERROR_PIPE_NOT_CONNECTED
                    | ERROR_OPERATION_ABORTED => Ok(0),
                    error => Err(io::Error::from_raw_os_error(error as i32)),
                };
            }
            Ok(n as usize)
        }

        fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
            let len = buf.len().min(u32::MAX as usize) as u32;
            // SAFETY: the buffer is valid for `len` bytes until `io` returns,
            // by which time the operation is complete or cancelled.
            self.io(true, |h, ov| unsafe {
                ReadFile(h, buf.as_mut_ptr(), len, null_mut(), ov)
            })
        }

        fn write(&self, buf: &[u8]) -> io::Result<usize> {
            if self.written.load(Ordering::Relaxed) {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            let len = buf.len().min(u32::MAX as usize) as u32;
            // SAFETY: as for `read`.
            let n = self.io(false, |h, ov| unsafe {
                WriteFile(h, buf.as_ptr(), len, null_mut(), ov)
            })?;
            if n == 0 && !buf.is_empty() {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            Ok(n)
        }
    }

    /// A connection: blocking, and safe to read and write from different
    /// threads at once.
    #[derive(Clone)]
    pub struct Stream(Arc<Pipe>);

    impl Stream {
        pub fn try_clone(&self) -> io::Result<Stream> {
            Ok(self.clone())
        }

        /// Ends the connection, which wakes a read waiting on it. A pipe
        /// cannot close one direction for the other end to see: shutting
        /// down the writing ends this end's writes and leaves its reads,
        /// and the other end learns of it only when the pipe closes.
        pub fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
            // both ways ends the writing too, as it does a socket's
            if how != std::net::Shutdown::Read {
                self.0.written.store(true, Ordering::Relaxed);
            }
            if how == std::net::Shutdown::Write {
                return Ok(());
            }
            let h = raw(&self.0.handle);
            // SAFETY: the event and the pipe are open; cancelling and
            // disconnecting have no memory preconditions.
            unsafe {
                check(SetEvent(raw(&self.0.stopped)))?;
                CancelIoEx(h, null());
                if self.0.server {
                    DisconnectNamedPipe(h);
                }
            }
            Ok(())
        }

        pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
            *self
                .0
                .read_timeout
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = timeout;
            Ok(())
        }

        /// Pipes are always used blocking here; for symmetry with sockets.
        pub fn set_nonblocking(&self, _: bool) -> io::Result<()> {
            Ok(())
        }

        /// The process at the other end.
        pub fn peer_pid(&self) -> io::Result<u32> {
            let mut pid = 0u32;
            let h = raw(&self.0.handle);
            // SAFETY: writes the id into `pid`.
            check(unsafe {
                if self.0.server {
                    GetNamedPipeClientProcessId(h, &mut pid)
                } else {
                    GetNamedPipeServerProcessId(h, &mut pid)
                }
            })?;
            Ok(pid)
        }
    }

    impl AsRawHandle for Stream {
        fn as_raw_handle(&self) -> RawHandle {
            self.0.handle.as_raw_handle()
        }
    }

    impl Read for Stream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }

    impl Read for &Stream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }

    impl Write for Stream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Write for &Stream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Whether the process `pid` runs as this user.
    fn runs_as_us(pid: u32) -> bool {
        match (crate::win::user_of(pid), crate::win::current_user()) {
            (Ok(theirs), Ok(ours)) => !ours.is_empty() && theirs == ours,
            _ => false,
        }
    }

    /// Whether the other end of `stream` runs as this user.
    pub fn same_user(stream: &Stream) -> bool {
        stream.peer_pid().is_ok_and(runs_as_us)
    }

    /// Refuses a pipe whose server runs as another user, which took the
    /// name first.
    fn check_server(pid: io::Result<u32>, path: &Path) -> io::Result<()> {
        if pid.is_ok_and(runs_as_us) {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} is served by another user", path.display()),
        ))
    }

    /// Connects to the pipe `path`, waiting up to five seconds while every
    /// instance is busy.
    pub fn connect(path: &Path) -> io::Result<Stream> {
        let name = wide(path.as_os_str());
        let until = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < until {
            // SAFETY: the name is NUL-terminated. Identification only: the
            // server cannot act as this user with the connection.
            let h = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    null_mut(),
                )
            };
            if h != INVALID_HANDLE_VALUE {
                let stream = Stream(Arc::new(Pipe::new(h, false)?));
                check_server(stream.peer_pid(), path)?;
                return Ok(stream);
            }
            // SAFETY: reads this thread's last error.
            if unsafe { GetLastError() } != ERROR_PIPE_BUSY {
                return Err(io::Error::last_os_error());
            }
            // every instance is taken; one frees up when the server accepts.
            // The wait returns at once while the server is between
            // instances, so it is not counted on to wait at all.
            // SAFETY: the name is NUL-terminated.
            if unsafe { WaitNamedPipeW(name.as_ptr(), 100) } == 0 {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Err(io::Error::from(io::ErrorKind::TimedOut))
    }

    /// A new instance of the pipe `name`. The first fails when the name is
    /// taken, so a pipe is never shared with whoever took it.
    fn instance(name: &[u16], first: bool) -> io::Result<Pipe> {
        let flags = PIPE_ACCESS_DUPLEX
            | FILE_FLAG_OVERLAPPED
            | if first {
                FILE_FLAG_FIRST_PIPE_INSTANCE
            } else {
                0
            };
        let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
        // SAFETY: the name is NUL-terminated; the security attributes live
        // for the life of the process.
        let h = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                flags,
                mode,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                crate::win::pipe_security()?,
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Pipe::new(h, true)
    }

    /// An instance waiting for a client, with its operation's state at a
    /// fixed address until the operation is done.
    struct Waiting {
        /// Taken once a client is connected.
        pipe: Option<Pipe>,
        ov: Box<OVERLAPPED>,
        _event: OwnedHandle,
        /// Whether a client is connected already.
        connected: bool,
    }

    // SAFETY: the OVERLAPPED's pointers are to its own event and are used
    // only through the listener's lock, by one thread at a time.
    unsafe impl Send for Waiting {}

    impl Waiting {
        fn start(name: &[u16], first: bool) -> io::Result<Self> {
            Self::on(instance(name, first)?)
        }

        /// Waits for a client on the instance `pipe`.
        fn on(pipe: Pipe) -> io::Result<Self> {
            let event = event()?;
            // SAFETY: OVERLAPPED is plain data; zero is its initial state.
            let mut ov: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
            ov.hEvent = raw(&event);
            let connected = loop {
                // SAFETY: the pipe and the OVERLAPPED, boxed, outlive the
                // operation: `Drop` cancels it and waits.
                if unsafe { ConnectNamedPipe(raw(&pipe.handle), &mut *ov) } != 0 {
                    break true;
                }
                // SAFETY: reads this thread's last error.
                match unsafe { GetLastError() } {
                    ERROR_PIPE_CONNECTED => break true,
                    ERROR_IO_PENDING => break false,
                    // a client came and left before the wait began: the
                    // instance is let go of, and waits for the next
                    // SAFETY: the pipe is open.
                    ERROR_NO_DATA => check(unsafe { DisconnectNamedPipe(raw(&pipe.handle)) })?,
                    error => return Err(io::Error::from_raw_os_error(error as i32)),
                }
            };
            Ok(Self {
                pipe: Some(pipe),
                ov,
                _event: event,
                connected,
            })
        }

        /// Waits up to `millis` for a client; whether one connected.
        fn wait(&mut self, millis: u32) -> io::Result<bool> {
            if self.connected {
                return Ok(true);
            }
            // SAFETY: waits on the operation's event.
            match unsafe { WaitForSingleObject(self.ov.hEvent, millis) } {
                WAIT_TIMEOUT => return Ok(false),
                WAIT_OBJECT_0 => {}
                _ => return Err(io::Error::last_os_error()),
            }
            let mut n = 0u32;
            let pipe = self.pipe.as_ref().expect("a pipe until connected");
            // SAFETY: the operation has completed; this reads its result.
            check(unsafe { GetOverlappedResult(raw(&pipe.handle), &*self.ov, &mut n, 0) })?;
            self.connected = true;
            Ok(true)
        }
    }

    impl Drop for Waiting {
        fn drop(&mut self) {
            if let (false, Some(pipe)) = (self.connected, &self.pipe) {
                let mut n = 0u32;
                let h = raw(&pipe.handle);
                // SAFETY: cancels the operation, then waits until it lets go
                // of the OVERLAPPED.
                unsafe {
                    CancelIoEx(h, &*self.ov);
                    GetOverlappedResult(h, &*self.ov, &mut n, 1);
                }
            }
        }
    }

    /// A pipe name clients connect to, with blocking accepts, or
    /// non-blocking ones.
    pub struct Listener {
        name: Vec<u16>,
        waiting: Mutex<Option<Waiting>>,
        nonblocking: AtomicBool,
    }

    /// Listens on the pipe `path`. Fails when the name is taken.
    pub fn listen(path: &Path) -> io::Result<Listener> {
        let name = wide(path.as_os_str());
        let first = Waiting::start(&name, true)?;
        Ok(Listener {
            name,
            waiting: Mutex::new(Some(first)),
            nonblocking: AtomicBool::new(false),
        })
    }

    /// A pipe's name ends with its last handle.
    pub fn remove(_: &Path) {}

    impl Listener {
        pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
            self.nonblocking.store(nonblocking, Ordering::Relaxed);
            Ok(())
        }

        /// The next client. While non-blocking, `WouldBlock` when none has
        /// connected.
        pub fn accept(&self) -> io::Result<(Stream, ())> {
            let mut waiting = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
            if waiting.is_none() {
                *waiting = Some(Waiting::start(&self.name, false)?);
            }
            let millis = if self.nonblocking.load(Ordering::Relaxed) {
                0
            } else {
                INFINITE
            };
            let current = waiting.as_mut().expect("an instance waits");
            match current.wait(millis) {
                Ok(true) => {}
                Ok(false) => return Err(io::Error::from(io::ErrorKind::WouldBlock)),
                Err(error) => {
                    // that instance is spent: the next accept starts another
                    *waiting = None;
                    return Err(error);
                }
            }
            let mut done = waiting.take().expect("an instance waits");
            // connected: nothing is pending, so the drop has nothing to cancel
            let pipe = done.pipe.take().expect("a pipe until connected");
            drop(done);
            // the next instance; when it cannot be made now, the next
            // accept tries again
            *waiting = Waiting::start(&self.name, false).ok();
            Ok((Stream(Arc::new(pipe)), ()))
        }
    }

    pub type AsyncStream = AsyncPipe;

    /// A connection in a runtime: either end of a pipe.
    pub enum AsyncPipe {
        Server(tokio::net::windows::named_pipe::NamedPipeServer),
        Client(tokio::net::windows::named_pipe::NamedPipeClient),
    }

    impl tokio::io::AsyncRead for AsyncPipe {
        fn poll_read(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<io::Result<()>> {
            match self.get_mut() {
                AsyncPipe::Server(pipe) => std::pin::Pin::new(pipe).poll_read(cx, buf),
                AsyncPipe::Client(pipe) => std::pin::Pin::new(pipe).poll_read(cx, buf),
            }
        }
    }

    impl tokio::io::AsyncWrite for AsyncPipe {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> std::task::Poll<io::Result<usize>> {
            match self.get_mut() {
                AsyncPipe::Server(pipe) => std::pin::Pin::new(pipe).poll_write(cx, buf),
                AsyncPipe::Client(pipe) => std::pin::Pin::new(pipe).poll_write(cx, buf),
            }
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<io::Result<()>> {
            match self.get_mut() {
                AsyncPipe::Server(pipe) => std::pin::Pin::new(pipe).poll_flush(cx),
                AsyncPipe::Client(pipe) => std::pin::Pin::new(pipe).poll_flush(cx),
            }
        }

        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<io::Result<()>> {
            match self.get_mut() {
                AsyncPipe::Server(pipe) => std::pin::Pin::new(pipe).poll_shutdown(cx),
                AsyncPipe::Client(pipe) => std::pin::Pin::new(pipe).poll_shutdown(cx),
            }
        }
    }

    /// The host's pipe for its controllers, in a runtime.
    pub struct AsyncListener {
        name: std::ffi::OsString,
        next: tokio::net::windows::named_pipe::NamedPipeServer,
    }

    impl AsyncListener {
        /// Listens on the pipe `path`. Fails when the name is taken. Must
        /// be called inside a runtime.
        pub fn bind(path: &Path) -> io::Result<Self> {
            let name = path.as_os_str().to_os_string();
            let next = Self::create(&name, true)?;
            Ok(Self { name, next })
        }

        fn create(
            name: &std::ffi::OsStr,
            first: bool,
        ) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
            let mut options = tokio::net::windows::named_pipe::ServerOptions::new();
            options
                .first_pipe_instance(first)
                .reject_remote_clients(true);
            // SAFETY: the security attributes live for the life of the
            // process.
            unsafe {
                options
                    .create_with_security_attributes_raw(name, crate::win::pipe_security()?.cast())
            }
        }

        pub async fn accept(&mut self) -> io::Result<AsyncStream> {
            self.next.connect().await?;
            let connected = std::mem::replace(&mut self.next, Self::create(&self.name, false)?);
            Ok(AsyncPipe::Server(connected))
        }
    }

    /// Connects to the pipe `path` in a runtime, waiting while every
    /// instance is busy, and refuses one served by another user.
    pub async fn connect_async(path: &Path) -> io::Result<AsyncStream> {
        let mut busy = 0;
        let client = loop {
            match tokio::net::windows::named_pipe::ClientOptions::new()
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .open(path)
            {
                Ok(client) => break client,
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) && busy < 50 => {
                    busy += 1;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(error) => return Err(error),
            }
        };
        let mut pid = 0u32;
        // SAFETY: the client's handle is open; writes the id into `pid`.
        let found = check(unsafe {
            GetNamedPipeServerProcessId(client.as_raw_handle() as HANDLE, &mut pid)
        })
        .map(|()| pid);
        check_server(found, path)?;
        Ok(AsyncPipe::Client(client))
    }

    /// A connection's two directions, each usable on its own task.
    pub fn split(
        stream: AsyncStream,
    ) -> (
        tokio::io::ReadHalf<AsyncStream>,
        tokio::io::WriteHalf<AsyncStream>,
    ) {
        tokio::io::split(stream)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A pipe the host listens on is this user's and the system's
        /// alone, and kept from processes of a lower integrity level.
        #[test]
        fn a_listener_s_pipe_admits_only_this_user_and_the_system() {
            use windows_sys::Win32::Foundation::LocalFree;
            use windows_sys::Win32::Security::Authorization::{
                ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo,
                SDDL_REVISION_1, SE_KERNEL_OBJECT,
            };
            use windows_sys::Win32::Security::{
                DACL_SECURITY_INFORMATION, LABEL_SECURITY_INFORMATION,
            };
            let path = std::path::PathBuf::from(format!(
                r"\\.\pipe\pinrail-net-acl-{}",
                crate::random_hex(4)
            ));
            let listener = listen(&path).unwrap();
            let waiting = listener.waiting.lock().unwrap();
            let pipe = waiting.as_ref().unwrap().pipe.as_ref().unwrap();
            let which = DACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION;
            let mut descriptor = null_mut();
            // SAFETY: reads the security of a pipe this test made; the
            // descriptor and its text are freed below.
            let sddl = unsafe {
                assert_eq!(
                    GetSecurityInfo(
                        raw(&pipe.handle),
                        SE_KERNEL_OBJECT,
                        which,
                        null_mut(),
                        null_mut(),
                        null_mut(),
                        null_mut(),
                        &mut descriptor,
                    ),
                    0
                );
                let mut text = null_mut();
                check(ConvertSecurityDescriptorToStringSecurityDescriptorW(
                    descriptor,
                    SDDL_REVISION_1,
                    which,
                    &mut text,
                    null_mut(),
                ))
                .unwrap();
                let len = (0..).take_while(|&i| *text.add(i) != 0).count();
                let sddl = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
                LocalFree(text.cast());
                LocalFree(descriptor);
                sddl
            };
            let user = crate::win::sid_text(&crate::win::current_user().unwrap());
            // a protected DACL, no other grant, and an integrity label
            assert!(sddl.contains("D:P"), "{sddl}");
            assert!(
                sddl.contains(";;;SY)") && sddl.contains(&format!(";;;{user})")),
                "{sddl}"
            );
            let grants = sddl.matches("(A;").count();
            assert_eq!(grants, 2, "{sddl}");
            // no reading or writing up from a lower level, in either order
            let label = sddl.split("(ML;;").nth(1).unwrap_or_default();
            assert!(label.contains("NW") && label.contains("NR"), "{sddl}");
        }

        /// A second host's listener on a name a host has is refused, as
        /// the first instance of a pipe must be its own.
        #[test]
        fn a_second_listener_on_a_taken_name_is_refused() {
            let path = std::path::PathBuf::from(format!(
                r"\\.\pipe\pinrail-net-twice-{}",
                crate::random_hex(4)
            ));
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let _first = AsyncListener::bind(&path).unwrap();
                assert!(AsyncListener::bind(&path).is_err());
                assert!(listen(&path).is_err());
            });
        }

        /// A client that connected and left before the server waited on
        /// the instance: the instance goes on to take the next one.
        #[test]
        fn an_instance_a_client_left_early_takes_the_next() {
            let path = std::path::PathBuf::from(format!(
                r"\\.\pipe\pinrail-net-early-{}",
                crate::random_hex(4)
            ));
            let name = wide(path.as_os_str());
            let pipe = instance(&name, true).unwrap();
            drop(connect(&path).unwrap());
            let mut waiting = Waiting::on(pipe).unwrap();
            let next = std::thread::spawn({
                let path = path.clone();
                move || connect(&path).map(|_| ())
            });
            assert!(waiting.wait(5_000).unwrap());
            next.join().unwrap().unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::time::Duration;

    fn address(name: &str) -> std::path::PathBuf {
        #[cfg(unix)]
        {
            let dir = std::env::temp_dir().join(format!("pinrail-net-{}", crate::random_hex(4)));
            std::fs::create_dir_all(&dir).unwrap();
            dir.join(format!("{name}.sock"))
        }
        #[cfg(windows)]
        {
            std::path::PathBuf::from(format!(
                r"\\.\pipe\pinrail-net-test-{}-{name}",
                crate::random_hex(4)
            ))
        }
    }

    #[test]
    fn a_connection_carries_bytes_both_ways_at_once_and_knows_its_user() {
        let path = address("both");
        let listener = listen(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            assert!(same_user(&stream));
            // a read waits in one thread while another writes
            let reader = stream.try_clone().unwrap();
            let reading = std::thread::spawn(move || {
                let mut buf = [0u8; 5];
                (&reader).read_exact(&mut buf).unwrap();
                buf
            });
            (&stream).write_all(b"hello").unwrap();
            assert_eq!(&reading.join().unwrap(), b"world");
            listener
        });
        let mut client = connect(&path).unwrap();
        assert!(same_user(&client));
        let mut buf = [0u8; 5];
        client.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello");
        client.write_all(b"world").unwrap();
        let listener = server.join().unwrap();
        remove(&path);
        drop(listener);
    }

    #[test]
    fn clients_that_connect_at_once_each_reach_the_server() {
        const THREADS: usize = 8;
        const EACH: usize = 40;
        let path = address("many");
        let listener = listen(&path).unwrap();
        // the server answers each client it gets, for as long as the test
        // runs: one that a client never reached would wait for it forever
        std::thread::spawn(move || {
            loop {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = (&stream).write_all(b"k");
                    }
                    Err(error) => {
                        eprintln!("the server stopped accepting: {error}");
                        return;
                    }
                }
            }
        });
        let clients: Vec<_> = (0..THREADS)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let mut reached = 0;
                    for _ in 0..EACH {
                        let client = match connect(&path) {
                            Ok(client) => client,
                            Err(error) => {
                                eprintln!("a client could not connect: {error}");
                                continue;
                            }
                        };
                        // macOS refuses a timeout once the server has
                        // written and closed, which is no failure
                        let _ = client.set_read_timeout(Some(Duration::from_secs(5)));
                        let mut buf = [0u8; 1];
                        match (&client).read_exact(&mut buf) {
                            Ok(()) => reached += 1,
                            Err(error) => eprintln!("a client read nothing: {error}"),
                        }
                    }
                    reached
                })
            })
            .collect();
        let reached: usize = clients.into_iter().map(|c| c.join().unwrap()).sum();
        assert_eq!(reached, THREADS * EACH);
    }

    #[test]
    fn a_connection_shut_down_both_ways_takes_no_more_writes() {
        let path = address("both");
        let listener = listen(&path).unwrap();
        // each end: the client's, and the server's
        for side in ["client", "server"] {
            let client = connect(&path).unwrap();
            let (server, _) = listener.accept().unwrap();
            let mut shut = if side == "client" { &client } else { &server };
            shut.shutdown(std::net::Shutdown::Both).unwrap();
            assert!(shut.write_all(b"more").is_err(), "{side}");
        }
        drop(listener);
        remove(&path);
    }

    #[test]
    fn shutting_down_the_writing_leaves_a_read_to_finish() {
        let path = address("half");
        let listener = listen(&path).unwrap();
        let client = connect(&path).unwrap();
        let (server, _) = listener.accept().unwrap();
        let reader = client.try_clone().unwrap();
        let reading = std::thread::spawn(move || {
            let mut buf = [0u8; 4];
            (&reader).read_exact(&mut buf).map(|()| buf)
        });
        std::thread::sleep(Duration::from_millis(100));
        client.shutdown(std::net::Shutdown::Write).unwrap();
        (&server).write_all(b"late").unwrap();
        assert_eq!(&reading.join().unwrap().unwrap(), b"late");
        // and the writing is over
        assert!((&client).write_all(b"more").is_err());
        drop(listener);
        remove(&path);
    }

    #[test]
    fn a_non_blocking_listener_says_when_no_one_connected_and_a_read_times_out() {
        let path = address("waiting");
        let listener = listen(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let error = listener.accept().map(|_| ()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
        let client = connect(&path).unwrap();
        let accepted = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("{e}"),
            }
        };
        // a second client connects to the next instance
        let second = connect(&path).unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut buf = [0u8; 1];
        assert!((&client).read(&mut buf).is_err());
        // and a shut-down connection reads its end at once
        accepted.shutdown(std::net::Shutdown::Both).unwrap();
        client.set_read_timeout(None).unwrap();
        assert_eq!((&client).read(&mut buf).unwrap_or(0), 0);
        drop(second);
        remove(&path);
    }

    #[test]
    fn a_name_that_is_listened_on_cannot_be_taken_again() {
        if cfg!(unix) {
            // a socket's lock, not its name, keeps a second host out
            return;
        }
        let path = address("taken");
        let _first = listen(&path).unwrap();
        assert!(listen(&path).is_err());
    }

    #[test]
    fn async_connections_carry_bytes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let path = address("async");
            let mut listener = AsyncListener::bind(&path).unwrap();
            let accepting = tokio::spawn(async move {
                let stream = listener.accept().await.unwrap();
                let (mut reader, mut writer) = split(stream);
                let mut buf = [0u8; 4];
                reader.read_exact(&mut buf).await.unwrap();
                writer.write_all(&buf).await.unwrap();
                writer.flush().await.unwrap();
                // keeps the instance until the client has read
                tokio::time::sleep(Duration::from_millis(200)).await;
            });
            let stream = connect_async(&path).await.unwrap();
            let (mut reader, mut writer) = split(stream);
            writer.write_all(b"ping").await.unwrap();
            let mut buf = [0u8; 4];
            reader.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping");
            accepting.await.unwrap();
            remove(&path);
        });
    }
}
