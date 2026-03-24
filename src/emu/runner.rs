//! BEMU sidecar: services RPCs from Spike `bebop_rocc` over shared memory.

use std::env;
use std::ffi::CString;
use std::sync::atomic::Ordering;

use crate::node;
use crate::shm::layout::{BEBOP_SHM_SIZE, OP_CMD_REQ, OP_CMD_RESP, OP_MEM_REQ, OP_MEM_RESP};
use crate::shm::protocol::decode_req;
use crate::shm::ShmMap;

use super::bemu::{Bemu, StepCfg};

pub fn bemu_tests() -> Result<(), String> {
    let node_id = node::node_id();
    if node_id == 0 {
        return Err("node_id must be > 0".to_string());
    }
    let name = env::var("BEBOP_SHM_NAME").map_err(|_| "missing env BEBOP_SHM_NAME".to_string())?;
    let cs = CString::new(name).map_err(|_| "bemu-tests: name has NUL")?;
    if !cs.as_bytes().starts_with(b"/") {
        return Err("bemu-tests: name must start with '/'".into());
    }
    let map = ShmMap::attach(cs.as_c_str(), BEBOP_SHM_SIZE)
        .map_err(|e| format!("worker shm attach: {e}"))?;
    let shm = map.raw_bebop();
    let mut bemu = Bemu::new();
    let mut step = StepCfg {
        on: env::var("BEBOP_STEP").ok().as_deref() == Some("1"),
        all_banks: env::var("BEBOP_STEP_BANKS").ok().as_deref() == Some("all"),
        idx: 0,
    };

    loop {
        let req = unsafe { (*shm).req.load(Ordering::Acquire) };
        let ack = unsafe { (*shm).ack.load(Ordering::Acquire) };
        if req == ack {
            std::thread::yield_now();
            continue;
        }
        if req != ack + 1 {
            panic!("bebop worker: invalid req/ack (req={req} ack={ack})");
        }

        // step 1. read the request from the shared memory
        let op = unsafe { (*shm).op };
        let sender = unsafe { (*shm).sender_id };
        let cmd = unsafe { (*shm).cmd_code };
        let rw = unsafe { (*shm).mem_rw };
        let funct = unsafe { (*shm).funct };
        let xs1 = unsafe { (*shm).xs1 };
        let xs2 = unsafe { (*shm).xs2 };
        let addr = unsafe { (*shm).addr };
        let data = unsafe { (*shm).data };
        if sender == 0 && !(op == OP_CMD_REQ && cmd == crate::shm::layout::CMD_SHUTDOWN) {
            panic!("bebop worker: sender_id must be non-zero");
        }

        // step 2. decode and handle the request
        let op_req = decode_req(op, cmd, rw, funct, xs1, xs2, addr, data);
        let resp = bemu.handle_op(op_req, &mut step);

        // step 3. write the response to the shared memory
        unsafe {
            (*shm).op = match op {
                OP_CMD_REQ => OP_CMD_RESP,
                OP_MEM_REQ => OP_MEM_RESP,
                _ => (*shm).op,
            };
            (*shm).sender_id = node_id;
            (*shm).receiver_id = sender;
            (*shm).err = resp.err;
            if let Some(v) = resp.result {
                (*shm).result = v;
            }
            if let Some(plan) = resp.plan {
                (*shm).sync_flags = plan.flags;
                (*shm).line_blocks = plan.line_blocks;
                (*shm).depth = plan.depth;
                (*shm).mem_addr = plan.mem_addr;
                (*shm).stride = plan.stride;
            }
            if let Some(d) = resp.data {
                (*shm).data = d;
            }
        }

        if resp.done {
            unsafe {
                (*shm).ack.store(req, Ordering::Release);
            }
            return Ok(());
        }

        unsafe {
            (*shm).ack.store(req, Ordering::Release);
        }
    }
}
