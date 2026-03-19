//! POSIX shared memory via `shm_open` + `mmap` (Linux).
//! Uses `libc` for `shm_open`/`shm_unlink` and `nix` for `ftruncate`/`mmap`/`munmap`/`close`.

use std::ffi::{c_void, CStr, CString};
use std::fmt;
use std::mem::size_of;
use std::num::NonZeroUsize;
use std::os::fd::BorrowedFd;
use std::ptr::NonNull;

use libc::{shm_open, shm_unlink, O_CREAT, O_EXCL, O_RDWR};
use nix::errno::Errno;
use nix::sys::mman::{mmap, munmap, MapFlags, ProtFlags};
use nix::sys::stat::fstat;
use nix::unistd::{close, ftruncate};

use super::layout::BebopShm;

#[derive(Debug)]
pub enum PosixShmErr {
    ShmOpen(Errno),
    Ftruncate(Errno),
    Mmap(Errno),
    Close(Errno),
    Fstat(Errno),
    SizeMismatch { got: i64, need: usize },
    ZeroSize,
}

impl fmt::Display for PosixShmErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PosixShmErr::ShmOpen(e) => write!(f, "shm_open: {e}"),
            PosixShmErr::Ftruncate(e) => write!(f, "ftruncate: {e}"),
            PosixShmErr::Mmap(e) => write!(f, "mmap: {e}"),
            PosixShmErr::Close(e) => write!(f, "close: {e}"),
            PosixShmErr::Fstat(e) => write!(f, "fstat: {e}"),
            PosixShmErr::SizeMismatch { got, need } => {
                write!(f, "shm size {got}, expected at least {need}")
            }
            PosixShmErr::ZeroSize => write!(f, "size must be non-zero"),
        }
    }
}

impl std::error::Error for PosixShmErr {}

/// Mapped POSIX shm. Optionally `shm_unlink` on drop.
pub struct ShmMap {
    name: CString,
    ptr: NonNull<c_void>,
    len: usize,
    unlink_on_drop: bool,
}

impl ShmMap {
    fn map_fd(raw_fd: i32, len: usize) -> Result<NonNull<c_void>, PosixShmErr> {
        let nz = NonZeroUsize::new(len).ok_or(PosixShmErr::ZeroSize)?;
        let fd = unsafe { BorrowedFd::borrow_raw(raw_fd) };
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
        Ok(ptr)
    }

    /// `O_CREAT|O_EXCL`, `ftruncate`, mmap. Caller clears `unlink_on_drop` to keep segment until manual unlink.
    pub fn create_new(name: &CStr, len: usize, unlink_on_drop: bool) -> Result<Self, PosixShmErr> {
        let raw_fd = unsafe {
            let raw = shm_open(name.as_ptr(), O_CREAT | O_EXCL | O_RDWR, 0o600);
            if raw < 0 {
                return Err(PosixShmErr::ShmOpen(Errno::last()));
            }
            raw
        };
        let fd = unsafe { BorrowedFd::borrow_raw(raw_fd) };
        ftruncate(fd, len as i64).map_err(PosixShmErr::Ftruncate)?;
        let ptr = Self::map_fd(raw_fd, len)?;
        Ok(Self {
            name: name.to_owned(),
            ptr,
            len,
            unlink_on_drop,
        })
    }

    /// Attach to an existing segment (Spike / worker).
    pub fn attach(name: &CStr, min_len: usize) -> Result<Self, PosixShmErr> {
        let raw_fd = unsafe {
            let raw = shm_open(name.as_ptr(), O_RDWR, 0);
            if raw < 0 {
                return Err(PosixShmErr::ShmOpen(Errno::last()));
            }
            raw
        };
        let st = fstat(raw_fd).map_err(PosixShmErr::Fstat)?;
        if st.st_size < min_len as i64 {
            close(raw_fd).map_err(PosixShmErr::Close)?;
            return Err(PosixShmErr::SizeMismatch {
                got: st.st_size,
                need: min_len,
            });
        }
        let map_len = st.st_size as usize;
        let ptr = Self::map_fd(raw_fd, map_len)?;
        Ok(Self {
            name: name.to_owned(),
            ptr,
            len: map_len,
            unlink_on_drop: false,
        })
    }

    pub fn as_bebop(&self) -> &BebopShm {
        assert!(self.len >= size_of::<BebopShm>());
        unsafe { &*self.ptr.as_ptr().cast::<BebopShm>() }
    }

    pub fn raw_bebop(&self) -> *mut BebopShm {
        assert!(self.len >= size_of::<BebopShm>());
        self.ptr.as_ptr().cast::<BebopShm>()
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr().cast::<u8>(), self.len) }
    }

    pub fn set_unlink_on_drop(&mut self, v: bool) {
        self.unlink_on_drop = v;
    }
}

impl Drop for ShmMap {
    fn drop(&mut self) {
        unsafe {
            munmap(self.ptr, self.len).unwrap_or_else(|e| panic!("posix shm munmap: {e}"));
            if self.unlink_on_drop && shm_unlink(self.name.as_ptr()) != 0 {
                panic!("posix shm_unlink: {}", Errno::last());
            }
        }
    }
}

/// Smoke test helper: create, unlink on drop.
pub struct PosixShm {
    inner: ShmMap,
}

impl PosixShm {
    pub fn create_exclusive(name: &CStr, len: usize) -> Result<Self, PosixShmErr> {
        Ok(Self {
            inner: ShmMap::create_new(name, len, true)?,
        })
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.inner.as_mut_slice()
    }
}
