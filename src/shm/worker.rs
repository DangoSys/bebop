//! BEMU sidecar: services RPCs from Spike `bebop_rocc` over shared memory.

use std::env;
use std::ffi::CString;
use std::sync::atomic::Ordering;

use crate::emu::bemu::Bemu;

use super::layout::{BEBOP_SHM_SIZE, OP_DECODE, OP_HANDLE, OP_READ, OP_SHUTDOWN, OP_SYNC};
use super::posix::ShmMap;

pub fn run(name: &CString) -> Result<(), String> {
    let map = ShmMap::attach(name.as_c_str(), BEBOP_SHM_SIZE)
        .map_err(|e| format!("worker shm attach: {e}"))?;
    let s = map.raw_bebop();
    let mut bemu = Bemu::new();
    let step = env::var("BEBOP_STEP").ok().as_deref() == Some("1");
    // Default: only MSET-allocated banks. Set BEBOP_STEP_BANKS=all to print every bank.
    let step_banks_all = env::var("BEBOP_STEP_BANKS").ok().as_deref() == Some("all");
    let mut step_idx: u64 = 0;
    loop {
        let r = unsafe { (*s).req.load(Ordering::Acquire) };
        let a = unsafe { (*s).ack.load(Ordering::Acquire) };
        if r == a {
            std::thread::yield_now();
            continue;
        }
        if r != a + 1 {
            panic!("bebop worker: invalid req/ack (req={r} ack={a})");
        }
        let op = unsafe { (*s).op };
        match op {
            OP_SHUTDOWN => {
                unsafe {
                    (*s).ack.store(r, Ordering::Release);
                }
                return Ok(());
            }
            OP_HANDLE => {
                let funct = unsafe { (*s).funct };
                let xs1 = unsafe { (*s).xs1 };
                let xs2 = unsafe { (*s).xs2 };
                let out = bemu.execute(funct, xs1, xs2);
                if step {
                    step_idx = step_idx.wrapping_add(1);
                    let hs = bemu.bank_hashes64_hex();
                    let parts: Vec<String> = hs
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| step_banks_all || bemu.bank_allocated(*i))
                        .map(|(i, h)| format!("b{i}={h}"))
                        .collect();
                    println!(
                        "step={} funct={} xs1=0x{:x} xs2=0x{:x} out=0x{:x} {}",
                        step_idx,
                        funct,
                        xs1,
                        xs2,
                        out,
                        parts.join(" ")
                    );
                }
                unsafe {
                    (*s).result = out;
                    (*s).err = 0;
                }
            }
            OP_DECODE => {
                let funct = unsafe { (*s).funct };
                let xs1 = unsafe { (*s).xs1 };
                let xs2 = unsafe { (*s).xs2 };
                let p = bemu.decode_sync_plan(funct, xs1, xs2);
                unsafe {
                    (*s).sync_flags = p.flags;
                    (*s).line_blocks = p.line_blocks;
                    (*s).depth = p.depth;
                    (*s).mem_addr = p.mem_addr;
                    (*s).stride = p.stride;
                    (*s).err = 0;
                }
            }
            OP_SYNC => {
                let addr = unsafe { (*s).sync_addr };
                let data = unsafe { (*s).data };
                bemu.write_memory(addr, &data);
                unsafe {
                    (*s).err = 0;
                }
            }
            OP_READ => {
                let addr = unsafe { (*s).sync_addr };
                let v = bemu.read_memory(addr, 16);
                unsafe {
                    (&mut (*s).data)[..16].copy_from_slice(&v[..16]);
                    (*s).err = 0;
                }
            }
            _ => unsafe {
                (*s).err = -1;
            },
        }
        unsafe {
            (*s).ack.store(r, Ordering::Release);
        }
    }
}
