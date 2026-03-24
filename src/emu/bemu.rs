use super::bank::{BankConfig, BankMap, BANK_NUM};
use super::configs::config::{EmuConfig, EmuMode};
use super::diff::config::DiffCfg;
use super::fss::fss;
use super::iss::iss;
use crate::shm::protocol::{OpReq, OpResp};

const MEM_BLK: usize = 16;

pub struct StepCfg {
    pub on: bool,
    pub idx: u64,
}

pub struct Bemu {
    memory: Vec<u8>,
    banks: Vec<Vec<u8>>,
    bank_configs: [BankConfig; BANK_NUM],
    bank_map: BankMap,
    emu_mode: EmuMode,
    /// FSS only: cumulative estimated cycles (`exec_latency::inst_cycles`).
    pub latency: u64,
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
            emu_mode: cfg.emu_mode,
            latency: 0,
        }
    }

    pub fn handle_req<R, W>(
        &mut self,
        req: OpReq,
        step: &mut StepCfg,
        diff: &DiffCfg,
        mem_read16: &mut R,
        mem_write16: &mut W,
    ) -> OpResp
    where
        R: FnMut(u64) -> [u8; MEM_BLK],
        W: FnMut(u64, [u8; MEM_BLK]),
    {
        match req {
            OpReq::CmdHandle { funct, xs1, xs2 } => match self.emu_mode {
                EmuMode::Iss => iss::execute_inst(
                    funct,
                    xs1,
                    xs2,
                    &mut self.memory,
                    mem_read16,
                    mem_write16,
                    &mut self.banks,
                    &mut self.bank_configs,
                    &mut self.bank_map,
                    step,
                    diff,
                ),
                EmuMode::Fss => fss::execute_inst(
                    funct,
                    xs1,
                    xs2,
                    &mut self.memory,
                    mem_read16,
                    mem_write16,
                    &mut self.banks,
                    &mut self.bank_configs,
                    &mut self.bank_map,
                    step,
                    diff,
                    &mut self.latency,
                ),
            },
            OpReq::CmdShutdown => OpResp::done(),
            OpReq::MemWrite { addr, data } => self.handle_mem_write(addr, data),
            OpReq::MemRead { addr } => self.handle_mem_read(addr),
            OpReq::Unknown => OpResp::err(-1),
        }
    }

    fn handle_mem_write(&mut self, addr: u64, data: [u8; MEM_BLK]) -> OpResp {
        let len = self.memory.len();
        let base = addr as usize % len;
        let mut off = 0usize;
        while off < data.len() {
            let pos = (base + off) % len;
            let take = (len - pos).min(data.len() - off);
            self.memory[pos..pos + take].copy_from_slice(&data[off..off + take]);
            off += take;
        }
        OpResp::ok()
    }

    fn handle_mem_read(&self, addr: u64) -> OpResp {
        let mut data = [0u8; MEM_BLK];
        let len = self.memory.len();
        let base = addr as usize % len;
        let mut off = 0usize;
        while off < MEM_BLK {
            let pos = (base + off) % len;
            let take = (len - pos).min(MEM_BLK - off);
            data[off..off + take].copy_from_slice(&self.memory[pos..pos + take]);
            off += take;
        }
        let mut resp = OpResp::ok();
        resp.data = Some(data);
        resp
    }
}

impl Default for Bemu {
    fn default() -> Self {
        Self::new()
    }
}
