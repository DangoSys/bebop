//! Must match `src/spike/bebop_shm.h` (same field layout as this module).

use std::mem::size_of;
use std::sync::atomic::{AtomicU64, Ordering};

pub const BEBOP_SHM_SIZE: usize = 4096;
pub const OP_CMD_REQ: u32 = 1;
pub const OP_CMD_RESP: u32 = 2;
pub const OP_MEM_REQ: u32 = 3;
pub const OP_MEM_RESP: u32 = 4;

pub const CMD_DECODE: u32 = 1;
pub const CMD_HANDLE: u32 = 2;
pub const CMD_SHUTDOWN: u32 = 255;

pub const MEM_WRITE: u32 = 1;
pub const MEM_READ: u32 = 2;

#[repr(C)]
pub struct BebopShm {
    pub req: AtomicU64,
    pub ack: AtomicU64,
    pub op: u32,
    pub sender_id: u32,
    pub receiver_id: u32,
    pub cmd_code: u32,
    pub mem_rw: u32,
    pub funct: u32,
    pub size: u32,
    pub err: i32,
    pub _pad0: u32,
    pub msg_id: u64,
    pub txn_id: u64,
    pub xs1: u64,
    pub xs2: u64,
    pub result: u64,
    pub addr: u64,
    pub data: [u8; 16],
    pub sync_flags: u32,
    pub line_blocks: u32,
    pub depth: u32,
    pub _pad1: u32,
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
        (*p).op = OP_CMD_REQ;
        (*p).sender_id = 0;
        (*p).receiver_id = 0;
        (*p).cmd_code = CMD_SHUTDOWN;
        (*p).err = 0;
    }
    s.req.fetch_add(1, Ordering::AcqRel);
    wait_done(s);
}
