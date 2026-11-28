//! A terminal's recent output in shared memory: the last 1 MiB the
//! viewers were sent. The host writes it and gives its descriptor to the
//! holder, which keeps it and never reads it, so that a host that starts
//! after a crash can rebuild the terminal's screen from it.
//!
//! The object is created with `memfd_create` on Linux and with `shm_open`
//! and an immediate unlink on macOS, or as an unlinked temporary file
//! where neither works; on Windows it is an unnamed file mapping, whose
//! handle the holder keeps as it keeps a descriptor. Its first 8 bytes hold the total count of bytes
//! ever written, updated after the bytes themselves, so a host that
//! crashes in the middle of a write leaves at most its last chunk
//! incomplete.

// mmap, munmap and the shared-memory calls can only be used with
// `unsafe`; each use says why it is sound.
#![allow(unsafe_code)]

use std::io;
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};

/// What keeps the object alive, and is passed to the holder.
#[cfg(unix)]
pub type Object = OwnedFd;
#[cfg(windows)]
pub type Object = std::os::windows::io::OwnedHandle;

/// The output kept.
pub const CAPACITY: usize = 1024 * 1024;
const HEADER: usize = 8;
const SIZE: usize = HEADER + CAPACITY;

/// A mapped ring.
pub struct SharedRing {
    fd: Object,
    map: *mut u8,
}

// SAFETY: the mapping is owned by this value, and written only through
// `&mut self`; the total is read and written atomically.
unsafe impl Send for SharedRing {}

#[cfg(windows)]
impl SharedRing {
    /// A new, empty ring.
    pub fn create() -> io::Result<Self> {
        use std::os::windows::io::{FromRawHandle, RawHandle};
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::Memory::{CreateFileMappingW, PAGE_READWRITE};
        // SAFETY: backed by the paging file, unnamed, not inheritable; the
        // handle is owned below.
        let mapping = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                std::ptr::null(),
                PAGE_READWRITE,
                0,
                SIZE as u32,
                std::ptr::null(),
            )
        };
        if mapping.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: as above.
        Self::map(unsafe { Object::from_raw_handle(mapping as RawHandle) })
    }

    /// The ring behind `handle`, as a holder hands it back.
    pub fn open(handle: Object) -> io::Result<Self> {
        Self::map(handle)
    }

    /// Maps SIZE bytes, which fails for a mapping that is smaller.
    fn map(handle: Object) -> io::Result<Self> {
        use windows_sys::Win32::System::Memory::{FILE_MAP_ALL_ACCESS, MapViewOfFile};
        // SAFETY: maps SIZE bytes of the mapping for reading and writing;
        // unmapped on drop.
        let view =
            unsafe { MapViewOfFile(crate::win::raw(&handle), FILE_MAP_ALL_ACCESS, 0, 0, SIZE) };
        if view.Value.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(SharedRing {
            fd: handle,
            map: view.Value.cast(),
        })
    }
}

#[cfg(unix)]
impl SharedRing {
    /// A new, empty ring.
    pub fn create() -> io::Result<Self> {
        let fd = create_object()?;
        // SAFETY: ftruncate on a descriptor this process owns.
        if unsafe { libc::ftruncate(fd.as_raw_fd(), SIZE as libc::off_t) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Self::map(fd)
    }

    /// The ring behind `fd`, as a holder hands it back.
    pub fn open(fd: OwnedFd) -> io::Result<Self> {
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        // SAFETY: fstat fills the struct it is given.
        if unsafe { libc::fstat(fd.as_raw_fd(), &mut stat) } == -1 {
            return Err(io::Error::last_os_error());
        }
        if (stat.st_size as usize) < SIZE {
            return Err(io::Error::other("the ring is too small"));
        }
        Self::map(fd)
    }

    fn map(fd: OwnedFd) -> io::Result<Self> {
        // SAFETY: maps SIZE bytes of an object at least that large, shared,
        // for reading and writing; unmapped on drop.
        let map = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                SIZE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };
        if map == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(SharedRing {
            fd,
            map: map.cast(),
        })
    }
}

impl SharedRing {
    /// The descriptor, or the handle, to give to the holder.
    pub fn fd(&self) -> &Object {
        &self.fd
    }

    fn total(&self) -> &AtomicU64 {
        // SAFETY: the header's 8 bytes are page-aligned, inside the
        // mapping, and only ever accessed as this atomic.
        unsafe { &*self.map.cast::<AtomicU64>() }
    }

    fn data(&self) -> *mut u8 {
        // SAFETY: the data follows the header inside the mapping.
        unsafe { self.map.add(HEADER) }
    }

    /// Appends output.
    pub fn write(&mut self, bytes: &[u8]) {
        // only the last CAPACITY bytes can be kept
        let bytes = &bytes[bytes.len().saturating_sub(CAPACITY)..];
        let total = self.total().load(Ordering::Acquire);
        let start = (total % CAPACITY as u64) as usize;
        let first = bytes.len().min(CAPACITY - start);
        // SAFETY: both ranges lie inside the data area, and `bytes` does
        // not overlap the mapping.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.data().add(start), first);
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr().add(first),
                self.data(),
                bytes.len() - first,
            );
        }
        self.total()
            .store(total + bytes.len() as u64, Ordering::Release);
    }

    /// The output kept, oldest first.
    pub fn read(&self) -> Vec<u8> {
        let total = self.total().load(Ordering::Acquire);
        let len = (total as usize).min(CAPACITY);
        let end = (total % CAPACITY as u64) as usize;
        let mut out = Vec::with_capacity(len);
        // SAFETY: both ranges lie inside the data area.
        unsafe {
            let data = std::slice::from_raw_parts(self.data(), CAPACITY);
            if len < CAPACITY {
                out.extend_from_slice(&data[..len]);
            } else {
                out.extend_from_slice(&data[end..]);
                out.extend_from_slice(&data[..end]);
            }
        }
        out
    }
}

impl Drop for SharedRing {
    fn drop(&mut self) {
        // SAFETY: unmaps the mapping made in `map`, which nothing uses
        // after this.
        #[cfg(unix)]
        unsafe {
            libc::munmap(self.map.cast(), SIZE)
        };
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::Memory::UnmapViewOfFile(
                windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.map.cast(),
                },
            )
        };
    }
}

#[cfg(target_os = "linux")]
fn create_object() -> io::Result<OwnedFd> {
    // SAFETY: the name is a NUL-terminated literal.
    let fd = unsafe { libc::memfd_create(c"pinrail-ring".as_ptr(), libc::MFD_CLOEXEC) };
    if fd == -1 {
        return unlinked_file();
    }
    // SAFETY: memfd_create returned a new descriptor that nothing else owns.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

#[cfg(all(unix, not(target_os = "linux")))]
fn create_object() -> io::Result<OwnedFd> {
    let name = std::ffi::CString::new(format!("/pinrail-ring-{}", crate::random_hex(8)))
        .map_err(io::Error::other)?;
    // SAFETY: the name is NUL-terminated; the flags and mode are integers.
    let fd = unsafe {
        libc::shm_open(
            name.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            0o600 as libc::c_uint,
        )
    };
    if fd == -1 {
        return unlinked_file();
    }
    // the object lives on as long as a descriptor refers to it
    // SAFETY: as above.
    unsafe { libc::shm_unlink(name.as_ptr()) };
    // SAFETY: shm_open returned a new descriptor that nothing else owns.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    // SAFETY: fcntl on a descriptor this process owns.
    unsafe {
        let flags = libc::fcntl(fd.as_raw_fd(), libc::F_GETFD);
        libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, flags | libc::FD_CLOEXEC);
    }
    Ok(fd)
}

/// A temporary file, removed at once, for where shared memory is missing.
#[cfg(unix)]
fn unlinked_file() -> io::Result<OwnedFd> {
    Ok(OwnedFd::from(tempfile_in_tmp()?))
}

#[cfg(unix)]
fn tempfile_in_tmp() -> io::Result<std::fs::File> {
    let path = std::env::temp_dir().join(format!("pinrail-ring-{}", crate::random_hex(8)));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    std::fs::remove_file(&path)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_keeps_the_last_mebibyte_in_order() {
        let mut ring = SharedRing::create().unwrap();
        assert!(ring.read().is_empty());
        ring.write(b"hello ");
        ring.write(b"world");
        assert_eq!(ring.read(), b"hello world");

        let big: Vec<u8> = (0..CAPACITY + 1000).map(|i| (i % 251) as u8).collect();
        ring.write(&big);
        let kept = ring.read();
        assert_eq!(kept.len(), CAPACITY);
        assert_eq!(kept, big[big.len() - CAPACITY..]);
        ring.write(b"tail");
        assert!(ring.read().ends_with(b"tail"));
    }

    #[test]
    fn another_mapping_of_the_descriptor_sees_the_output() {
        let mut ring = SharedRing::create().unwrap();
        ring.write(b"kept across hosts");
        let other = SharedRing::open(ring.fd().try_clone().unwrap()).unwrap();
        drop(ring);
        assert_eq!(other.read(), b"kept across hosts");
    }
}
