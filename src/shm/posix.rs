//! POSIX shared memory via `shm_open` + `mmap` (Linux).
//! Uses `libc` for `shm_open`/`shm_unlink` and `nix` for `ftruncate`/`mmap`/`munmap`/`close`.

use std::ffi::{c_void, CStr, CString};
use std::fmt;
use std::num::NonZeroUsize;
use std::os::fd::BorrowedFd;
use std::ptr::NonNull;

use libc::{shm_open, shm_unlink, O_CREAT, O_EXCL, O_RDWR};
use nix::errno::Errno;
use nix::sys::mman::{mmap, munmap, MapFlags, ProtFlags};
use nix::unistd::{close, ftruncate};

#[derive(Debug)]
pub enum PosixShmErr {
    ShmOpen(Errno),
    Ftruncate(Errno),
    Mmap(Errno),
    Close(Errno),
    ZeroSize,
}

impl fmt::Display for PosixShmErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PosixShmErr::ShmOpen(e) => write!(f, "shm_open: {e}"),
            PosixShmErr::Ftruncate(e) => write!(f, "ftruncate: {e}"),
            PosixShmErr::Mmap(e) => write!(f, "mmap: {e}"),
            PosixShmErr::Close(e) => write!(f, "close: {e}"),
            PosixShmErr::ZeroSize => write!(f, "size must be non-zero"),
        }
    }
}

impl std::error::Error for PosixShmErr {}

/// Exclusive create + map. `Drop` munmaps and `shm_unlink`s the segment.
pub struct PosixShm {
    name: CString,
    ptr: NonNull<c_void>,
    len: usize,
}

impl PosixShm {
    pub fn create_exclusive(name: &CStr, len: usize) -> Result<Self, PosixShmErr> {
        let nz = NonZeroUsize::new(len).ok_or(PosixShmErr::ZeroSize)?;
        let raw_fd = unsafe {
            let raw = shm_open(name.as_ptr(), O_CREAT | O_EXCL | O_RDWR, 0o600);
            if raw < 0 {
                return Err(PosixShmErr::ShmOpen(Errno::last()));
            }
            raw
        };
        let fd = unsafe { BorrowedFd::borrow_raw(raw_fd) };
        ftruncate(fd, len as i64).map_err(PosixShmErr::Ftruncate)?;
        let ptr = unsafe {
            mmap(
                None,
                nz,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_SHARED,
                fd,
                0,
            )
        }
        .map_err(PosixShmErr::Mmap)?;
        close(raw_fd).map_err(PosixShmErr::Close)?;
        Ok(Self {
            name: name.to_owned(),
            ptr,
            len,
        })
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr().cast::<u8>(), self.len) }
    }
}

impl Drop for PosixShm {
    fn drop(&mut self) {
        unsafe {
            munmap(self.ptr, self.len).unwrap_or_else(|e| panic!("posix shm munmap: {e}"));
            if shm_unlink(self.name.as_ptr()) != 0 {
                panic!("posix shm_unlink: {}", Errno::last());
            }
        }
    }
}
