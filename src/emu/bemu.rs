use super::bank::{BankConfig, BankMap, BANK_NUM};
use super::configs::config::EmuConfig;
use super::diff::hash::bank_hash;
use super::inst::decode::{self, SyncPlan};
use super::iss::iss;
use crate::shm::protocol::{OpReq, OpResp};

pub struct StepCfg {
    pub on: bool,
    pub all_banks: bool,
    pub idx: u64,
}

pub struct Bemu {
    memory: Vec<u8>,
    banks: Vec<Vec<u8>>,
    bank_configs: [BankConfig; BANK_NUM],
    bank_map: BankMap,
}

impl Bemu {
    pub fn new() -> Self {
        let cfg = EmuConfig::load().unwrap_or_else(|e| panic!("BEMU config load failed: {e}"));
        Self {
            memory: vec![0; cfg.total_memory_size()],
            banks: (0..cfg.bank_num)
                .map(|_| vec![0; cfg.bank_size()])
                .collect(),
            bank_configs: [BankConfig::default(); BANK_NUM],
            bank_map: BankMap::new(cfg.bank_num),
        }
    }

    pub fn execute(&mut self, funct: u32, xs1: u64, xs2: u64) -> u64 {
        iss::execute_inst(
            funct,
            xs1,
            xs2,
            &mut self.memory,
            &mut self.banks,
            &mut self.bank_configs,
            &mut self.bank_map,
        )
    }

    pub fn handle_op(&mut self, req: OpReq, step: &mut StepCfg) -> OpResp {
        match req {
            OpReq::CmdShutdown => OpResp::done(),
            OpReq::CmdHandle { funct, xs1, xs2 } => self.handle_op_handle(funct, xs1, xs2, step),
            OpReq::CmdDecode { funct, xs1, xs2 } => self.handle_op_decode(funct, xs1, xs2),
            OpReq::MemWrite { addr, data } => self.handle_op_sync(addr, data),
            OpReq::MemRead { addr } => self.handle_op_read(addr),
            OpReq::Unknown { op, cmd, rw } => {
                let _ = (op, cmd, rw);
                OpResp::err(-1)
            }
        }
    }

    fn handle_op_handle(&mut self, funct: u32, xs1: u64, xs2: u64, step: &mut StepCfg) -> OpResp {
        let out = self.execute(funct, xs1, xs2);
        if step.on {
            step.idx = step.idx.wrapping_add(1);
            let banks = bank_hash(&self.banks, &self.bank_configs, step.all_banks);
            println!(
                "step={} funct={} xs1=0x{:x} xs2=0x{:x} out=0x{:x} {}",
                step.idx, funct, xs1, xs2, out, banks
            );
        }

        let mut resp = OpResp::ok();
        resp.result = Some(out);
        resp
    }

    fn handle_op_decode(&self, funct: u32, xs1: u64, xs2: u64) -> OpResp {
        let mut resp = OpResp::ok();
        resp.plan = Some(self.decode_sync_plan(funct, xs1, xs2));
        resp
    }

    fn handle_op_sync(&mut self, addr: u64, data: [u8; 16]) -> OpResp {
        self.write_memory(addr, &data);
        OpResp::ok()
    }

    fn handle_op_read(&self, addr: u64) -> OpResp {
        let bytes = self.read_memory(addr, 16);
        let mut data = [0u8; 16];
        data.copy_from_slice(&bytes[..16]);
        let mut resp = OpResp::ok();
        resp.data = Some(data);
        resp
    }

    pub fn decode_sync_plan(&self, funct: u32, xs1: u64, xs2: u64) -> SyncPlan {
        decode::build_sync_plan(funct, xs1, xs2, &self.bank_configs)
    }

    pub fn write_memory(&mut self, addr: u64, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let len = self.memory.len();
        let base = addr as usize % len;
        let mut di = 0usize;
        while di < data.len() {
            let pos = (base + di) % len;
            let take = (len - pos).min(data.len() - di);
            self.memory[pos..pos + take].copy_from_slice(&data[di..di + take]);
            di += take;
        }
    }

    pub fn read_memory(&self, addr: u64, size: usize) -> Vec<u8> {
        if size == 0 {
            return Vec::new();
        }
        let len = self.memory.len();
        let base = addr as usize % len;
        let mut out = Vec::with_capacity(size);
        let mut got = 0usize;
        while got < size {
            let pos = (base + got) % len;
            let take = (len - pos).min(size - got);
            out.extend_from_slice(&self.memory[pos..pos + take]);
            got += take;
        }
        out
    }
}

impl Default for Bemu {
    fn default() -> Self {
        Self::new()
    }
}
