//! Must match `src/spike/bebop_shm.h` (same field layout as this module).

use std::mem::size_of;
use std::sync::atomic::{AtomicU64, Ordering};

pub const BEBOP_SHM_SIZE: usize = 4096;
pub const OP_HANDLE: u32 = 0;
pub const OP_SYNC: u32 = 1;
pub const OP_READ: u32 = 2;
pub const OP_SHUTDOWN: u32 = 3;
pub const OP_DECODE: u32 = 4;

#[repr(C)]
pub struct BebopShm {
    pub req: AtomicU64,
    pub ack: AtomicU64,
    pub op: u32,
    pub _pad0: u32,
    pub funct: u32,
    pub _pad1: u32,
    pub xs1: u64,
    pub xs2: u64,
    pub result: u64,
    pub sync_addr: u64,
    pub err: i32,
    pub _pad2: u32,
    pub data: [u8; 16],
    pub sync_flags: u32,
    pub line_blocks: u32,
    pub depth: u32,
    pub _pad3: u32,
    pub mem_addr: u64,
    pub stride: u64,
}

const _: () = assert!(size_of::<BebopShm>() <= BEBOP_SHM_SIZE);

pub fn wait_idle(s: &BebopShm) {
    loop {
        let r = s.req.load(Ordering::Acquire);
        let a = s.ack.load(Ordering::Acquire);
        if r == a {
            return;
        }
        std::thread::yield_now();
    }
}

pub fn wait_done(s: &BebopShm) {
    let r = s.req.load(Ordering::Acquire);
    while s.ack.load(Ordering::Acquire) != r {
        std::thread::yield_now();
    }
}

pub fn rpc_shutdown(s: &BebopShm) {
    wait_idle(s);
    unsafe {
        let p = s as *const BebopShm as *mut BebopShm;
        (*p).op = OP_SHUTDOWN;
    }
    s.req.fetch_add(1, Ordering::AcqRel);
    wait_done(s);
}
