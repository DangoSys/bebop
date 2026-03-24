use super::layout::{
    CMD_DECODE, CMD_HANDLE, CMD_SHUTDOWN, MEM_READ, MEM_WRITE, OP_CMD_REQ, OP_MEM_REQ,
};
use crate::emu::inst::decode::SyncPlan;

pub enum OpReq {
    CmdDecode { funct: u32, xs1: u64, xs2: u64 },
    CmdHandle { funct: u32, xs1: u64, xs2: u64 },
    CmdShutdown,
    MemWrite { addr: u64, data: [u8; 16] },
    MemRead { addr: u64 },
    Unknown { op: u32, cmd: u32, rw: u32 },
}

pub struct OpResp {
    pub done: bool,
    pub err: i32,
    pub result: Option<u64>,
    pub plan: Option<SyncPlan>,
    pub data: Option<[u8; 16]>,
}

impl OpResp {
    pub fn done() -> Self {
        Self {
            done: true,
            err: 0,
            result: None,
            plan: None,
            data: None,
        }
    }

    pub fn ok() -> Self {
        Self {
            done: false,
            err: 0,
            result: None,
            plan: None,
            data: None,
        }
    }

    pub fn err(code: i32) -> Self {
        Self {
            done: false,
            err: code,
            result: None,
            plan: None,
            data: None,
        }
    }
}

pub fn decode_req(
    op: u32,
    cmd: u32,
    rw: u32,
    funct: u32,
    xs1: u64,
    xs2: u64,
    addr: u64,
    data: [u8; 16],
) -> OpReq {
    match op {
        OP_CMD_REQ => match cmd {
            CMD_DECODE => OpReq::CmdDecode { funct, xs1, xs2 },
            CMD_HANDLE => OpReq::CmdHandle { funct, xs1, xs2 },
            CMD_SHUTDOWN => OpReq::CmdShutdown,
            _ => OpReq::Unknown { op, cmd, rw },
        },
        OP_MEM_REQ => match rw {
            MEM_WRITE => OpReq::MemWrite { addr, data },
            MEM_READ => OpReq::MemRead { addr },
            _ => OpReq::Unknown { op, cmd, rw },
        },
        _ => OpReq::Unknown { op, cmd, rw },
    }
}
