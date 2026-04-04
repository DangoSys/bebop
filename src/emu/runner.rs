//! BEMU sidecar: services RPCs from Spike `bebop_rocc` over shared memory.

use std::ffi::CString;
use std::sync::atomic::Ordering;

use crate::node;
use crate::shm::layout::{
    BebopLane, BebopMsg, BebopShm, BEBOP_SHM_SIZE, CMD_SHUTDOWN, MEM_READ, MEM_WRITE, OP_CMD_REQ,
    OP_CMD_RESP, OP_MEM_REQ, OP_MEM_RESP,
};
use crate::shm::protocol::{decode_req, OpReq, OpResp};
use crate::shm::ShmMap;
use crate::utils::env::must_nonempty;

use super::bemu::{Bemu, StepCfg};
use super::diff::config::DiffCfg;

#[cfg(feature = "verilator")]
use crate::verilator::{
    cosim_bank_digest_peek, cosim_issue, cosim_result, cosim_set_digest_all_banks,
};

#[derive(Clone, Copy)]
pub(crate) enum ShmMemLane {
    Bemu,
    Rtl,
}

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

unsafe fn mem_lane_mut(shm: *mut BebopShm, lane: ShmMemLane) -> *mut BebopLane {
    match lane {
        ShmMemLane::Bemu => &mut (*shm).mem_bemu,
        ShmMemLane::Rtl => &mut (*shm).mem_rtl,
    }
}

pub(crate) unsafe fn shm_mem_read16(
    shm: *mut BebopShm,
    mem_lane: ShmMemLane,
    node_id: u32,
    addr: u64,
) -> [u8; 16] {
    let mem = &mut *mem_lane_mut(shm, mem_lane);
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

pub(crate) unsafe fn mem_req_write16(
    shm: *mut BebopShm,
    mem_lane: ShmMemLane,
    node_id: u32,
    addr: u64,
    data: [u8; 16],
) {
    let mem = &mut *mem_lane_mut(shm, mem_lane);
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
    mem_lane: ShmMemLane,
    node_id: u32,
    bemu: &mut Bemu,
    step: &mut StepCfg,
    diff: &DiffCfg,
    post_handle: &mut impl FnMut(OpReq, &OpResp, &mut Bemu, &DiffCfg),
) -> Tick {
    let cmd = &mut (*shm).cmd_bemu;
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
    let mut rd = |addr: u64| shm_mem_read16(shm, mem_lane, node_id, addr);
    let mut wr = |addr: u64, data: [u8; 16]| {
        mem_req_write16(shm, mem_lane, node_id, addr, data);
    };
    let resp = bemu.handle_req(req_op, step, diff, &mut rd, &mut wr);
    post_handle(req_op, &resp, bemu, diff);
    fill_resp(&mut cmd.msg, OP_CMD_RESP, node_id, msg.sender_id, &resp);
    if let OpReq::CmdHandle { .. } = req_op {
        if resp.err == 0 && !resp.done {
            let d = bemu.cosim_bank_digest(diff);
            cmd.msg.bank_digest = d;
            if step.on {
                println!("  bemu_bank_digest=0x{d:016x}");
            }
        }
    }
    cmd.ack.store(req, Ordering::Release);
    if resp.done {
        Tick::Done
    } else {
        Tick::Worked
    }
}

#[cfg(feature = "verilator")]
pub(crate) unsafe fn run_cmd_rtl(
    shm: *mut BebopShm,
    node_id: u32,
    diff: &DiffCfg,
    step_on: bool,
) -> Tick {
    let cmd = &mut (*shm).cmd_rtl;
    let req = cmd.req.load(Ordering::Acquire);
    let ack = cmd.ack.load(Ordering::Acquire);
    if req == ack {
        return Tick::Idle;
    }
    if req != ack + 1 {
        panic!("verilator-engine: invalid cmd req/ack (req={req} ack={ack})");
    }
    let msg = cmd.msg;
    if msg.sender_id == 0 && !(msg.op == OP_CMD_REQ && msg.cmd_code == CMD_SHUTDOWN) {
        panic!("verilator-engine: cmd sender_id must be non-zero");
    }

    let req_op = decode_msg(msg);
    let resp = match req_op {
        OpReq::CmdShutdown => OpResp::done(),
        OpReq::CmdHandle { funct, xs1, xs2 } => {
            cosim_set_digest_all_banks(diff.all_banks);
            cosim_issue(funct, xs1, xs2);
            let rd = cosim_result();
            let digest = cosim_bank_digest_peek();
            if step_on {
                println!("  rtl_bank_digest=0x{digest:016x}");
            }
            cmd.msg.bank_digest = digest;
            OpResp {
                done: false,
                err: 0,
                result: Some(rd),
                data: None,
            }
        }
        _ => panic!("verilator-engine: unexpected cmd op (expected HANDLE or SHUTDOWN)"),
    };

    fill_resp(&mut cmd.msg, OP_CMD_RESP, node_id, msg.sender_id, &resp);
    cmd.ack.store(req, Ordering::Release);
    if resp.done {
        Tick::Done
    } else {
        Tick::Worked
    }
}

pub fn bemu_tests(
    step_on: bool,
    diff_all_banks: bool,
    config: Option<std::path::PathBuf>,
) -> Result<(), String> {
    let node_id = node::node_id();
    if node_id == 0 {
        return Err("node_id must be > 0".to_string());
    }
    let name = must_nonempty("BEBOP_SHM_NAME")?;
    let cs = CString::new(name).map_err(|_| "bemu-tests: name has NUL")?;
    if !cs.as_bytes().starts_with(b"/") {
        return Err("bemu-tests: name must start with '/'".into());
    }
    let map = ShmMap::attach(cs.as_c_str(), BEBOP_SHM_SIZE)
        .map_err(|e| format!("worker shm attach: {e}"))?;
    let shm = map.raw_bebop();
    let mut bemu = Bemu::with_config(config.as_deref());
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
                ShmMemLane::Bemu,
                node_id,
                &mut bemu,
                &mut step,
                &diff,
                &mut |_req, _resp, _bemu, _diff| {},
            ) {
                Tick::Done => return Ok(()),
                Tick::Worked => {}
                Tick::Idle => std::thread::yield_now(),
            }
        }
    }
}
