//! BEMU sidecar: services RPCs from Spike `bebop_rocc` over shared memory.

use std::env;
use std::ffi::CString;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::Ordering;

use crate::node;
use crate::shm::layout::{
    BebopMsg, BebopShm, BEBOP_SHM_SIZE, CMD_SHUTDOWN, MEM_READ, MEM_WRITE, OP_CMD_REQ, OP_CMD_RESP,
    OP_MEM_REQ, OP_MEM_RESP,
};
use crate::shm::protocol::{decode_req, OpReq, OpResp};
use crate::shm::ShmMap;

use super::bemu::{Bemu, StepCfg};
use super::diff::config::DiffCfg;

const BLK: u32 = 16;

pub(crate) enum Tick {
    Idle,
    Worked,
    Done,
}

#[inline]
fn decode_msg(msg: BebopMsg) -> OpReq {
    decode_req(
        msg.op,
        msg.cmd_code,
        msg.mem_rw,
        msg.size,
        msg.funct,
        msg.xs1,
        msg.xs2,
        msg.addr,
        msg.data,
    )
}

#[inline]
fn fill_resp(msg: &mut BebopMsg, op: u32, sender: u32, receiver: u32, resp: &OpResp) {
    msg.op = op;
    msg.sender_id = sender;
    msg.receiver_id = receiver;
    msg.err = resp.err;
    msg.size = BLK;
    if let Some(v) = resp.result {
        msg.result = v;
    }
    if let Some(d) = resp.data {
        msg.data = d;
    }
}

unsafe fn mem_req_read16(shm: *mut BebopShm, node_id: u32, addr: u64) -> [u8; 16] {
    let mem = &mut (*shm).mem;
    let req0 = mem.req.load(Ordering::Acquire);
    let ack0 = mem.ack.load(Ordering::Acquire);
    if req0 != ack0 {
        panic!("bebop worker: mem lane busy");
    }
    mem.msg.op = OP_MEM_REQ;
    mem.msg.sender_id = node_id;
    mem.msg.receiver_id = 0;
    mem.msg.mem_rw = MEM_READ;
    mem.msg.size = 16;
    mem.msg.addr = addr;
    mem.msg.err = 0;
    mem.req.fetch_add(1, Ordering::AcqRel);
    let target = mem.req.load(Ordering::Acquire);
    while mem.ack.load(Ordering::Acquire) != target {
        std::thread::yield_now();
    }
    if mem.msg.op != OP_MEM_RESP || mem.msg.err != 0 || mem.msg.size != 16 {
        panic!("bebop worker: mem request failed");
    }
    mem.msg.data
}

unsafe fn mem_req_write16(shm: *mut BebopShm, node_id: u32, addr: u64, data: [u8; 16]) {
    let mem = &mut (*shm).mem;
    let req0 = mem.req.load(Ordering::Acquire);
    let ack0 = mem.ack.load(Ordering::Acquire);
    if req0 != ack0 {
        panic!("bebop worker: mem lane busy");
    }
    mem.msg.op = OP_MEM_REQ;
    mem.msg.sender_id = node_id;
    mem.msg.receiver_id = 0;
    mem.msg.mem_rw = MEM_WRITE;
    mem.msg.size = 16;
    mem.msg.addr = addr;
    mem.msg.data = data;
    mem.msg.err = 0;
    mem.req.fetch_add(1, Ordering::AcqRel);
    let target = mem.req.load(Ordering::Acquire);
    while mem.ack.load(Ordering::Acquire) != target {
        std::thread::yield_now();
    }
    if mem.msg.op != OP_MEM_RESP || mem.msg.err != 0 || mem.msg.size != 16 {
        panic!("bebop worker: mem request failed");
    }
}

pub(crate) unsafe fn run_cmd(
    shm: *mut BebopShm,
    node_id: u32,
    bemu: &mut Bemu,
    step: &mut StepCfg,
    diff: &DiffCfg,
    post_handle: &mut impl FnMut(OpReq, &OpResp),
) -> Tick {
    let cmd = &mut (*shm).cmd;
    let req = cmd.req.load(Ordering::Acquire);
    let ack = cmd.ack.load(Ordering::Acquire);
    if req == ack {
        return Tick::Idle;
    }
    if req != ack + 1 {
        panic!("bebop worker: invalid cmd req/ack (req={req} ack={ack})");
    }
    let msg = cmd.msg;
    if msg.sender_id == 0 && !(msg.op == OP_CMD_REQ && msg.cmd_code == CMD_SHUTDOWN) {
        panic!("bebop worker: cmd sender_id must be non-zero");
    }

    let req_op = decode_msg(msg);
    let mut rd = |addr: u64| mem_req_read16(shm, node_id, addr);
    let mut wr = |addr: u64, data: [u8; 16]| {
        mem_req_write16(shm, node_id, addr, data);
    };
    let resp = match catch_unwind(AssertUnwindSafe(|| {
        bemu.handle_req(req_op, step, diff, &mut rd, &mut wr)
    })) {
        Ok(resp) => resp,
        Err(_) => OpResp::err(-1),
    };
    post_handle(req_op, &resp);
    fill_resp(&mut cmd.msg, OP_CMD_RESP, node_id, msg.sender_id, &resp);
    cmd.ack.store(req, Ordering::Release);
    if resp.done {
        Tick::Done
    } else {
        Tick::Worked
    }
}

pub fn bemu_tests(step_on: bool, diff_all_banks: bool) -> Result<(), String> {
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
        on: step_on,
        idx: 0,
    };
    let diff = DiffCfg {
        all_banks: diff_all_banks,
    };

    loop {
        unsafe {
            match run_cmd(
                shm,
                node_id,
                &mut bemu,
                &mut step,
                &diff,
                &mut |_req, _resp| {},
            ) {
                Tick::Done => return Ok(()),
                Tick::Worked => {}
                Tick::Idle => std::thread::yield_now(),
            }
        }
    }
}
