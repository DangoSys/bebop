use super::layout::{CMD_HANDLE, CMD_SHUTDOWN, MEM_READ, MEM_WRITE, OP_CMD_REQ, OP_MEM_REQ};

#[derive(Clone, Copy)]
pub enum OpReq {
    CmdHandle { funct: u32, xs1: u64, xs2: u64 },
    CmdShutdown,
    MemWrite { addr: u64, data: [u8; 16] },
    MemRead { addr: u64 },
    Unknown,
}

pub struct OpResp {
    pub done: bool,
    pub err: i32,
    pub result: Option<u64>,
    pub data: Option<[u8; 16]>,
}

impl OpResp {
    pub fn done() -> Self {
        Self {
            done: true,
            err: 0,
            result: None,
            data: None,
        }
    }

    pub fn ok() -> Self {
        Self {
            done: false,
            err: 0,
            result: None,
            data: None,
        }
    }

    pub fn err(code: i32) -> Self {
        Self {
            done: false,
            err: code,
            result: None,
            data: None,
        }
    }
}

pub fn decode_req(
    op: u32,
    cmd: u32,
    rw: u32,
    size: u32,
    funct: u32,
    xs1: u64,
    xs2: u64,
    addr: u64,
    data: [u8; 16],
) -> OpReq {
    match op {
        OP_CMD_REQ => match cmd {
            CMD_HANDLE => OpReq::CmdHandle { funct, xs1, xs2 },
            CMD_SHUTDOWN => OpReq::CmdShutdown,
            _ => {
                let _ = (op, cmd, rw);
                OpReq::Unknown
            }
        },
        OP_MEM_REQ => match rw {
            MEM_WRITE => {
                if size != 16 {
                    OpReq::Unknown
                } else {
                    OpReq::MemWrite { addr, data }
                }
            }
            MEM_READ => {
                if size != 16 {
                    OpReq::Unknown
                } else {
                    OpReq::MemRead { addr }
                }
            }
            _ => {
                let _ = (op, cmd, rw, size);
                OpReq::Unknown
            }
        },
        _ => {
            let _ = (op, cmd, rw, size);
            OpReq::Unknown
        }
    }
}
