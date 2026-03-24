use super::bank::{BankConfig, BankMap, BANK_NUM};
use super::configs::config::{BemuStats, EmuConfig};
use super::diff::hash::bank_hash64;
use super::inst::decode::{self, SyncPlan};

pub struct Bemu {
    memory: Vec<u8>,
    banks: Vec<Vec<u8>>,
    stats: BemuStats,
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
            stats: BemuStats::default(),
            bank_configs: [BankConfig::default(); BANK_NUM],
            bank_map: BankMap::new(cfg.bank_num),
        }
    }

    #[inline]
    fn encode_result(funct: u32, ret: u64) -> u64 {
        if ret == 0 {
            funct as u64
        } else {
            0
        }
    }

    pub fn set_verbose(&mut self, verbose: bool) {
        if verbose {
            log::set_max_level(log::LevelFilter::Debug);
        } else {
            log::set_max_level(log::LevelFilter::Info);
        }
    }

    pub fn execute(&mut self, funct: u32, xs1: u64, xs2: u64) -> u64 {
        self.stats.instructions_executed += 1;
        let ret = match decode::execute_known(
            funct,
            xs1,
            xs2,
            &mut self.memory,
            &mut self.banks,
            &mut self.bank_configs,
            &mut self.bank_map,
        ) {
            Some(v) => v,
            None => panic!("Bemu: unknown funct={funct}"),
        };
        Self::encode_result(funct, ret)
    }

    pub fn decode_sync_plan(&self, funct: u32, xs1: u64, xs2: u64) -> SyncPlan {
        decode::build_sync_plan(funct, xs1, xs2, &self.bank_configs)
    }

    #[inline]
    pub fn bank_allocated(&self, i: usize) -> bool {
        i < BANK_NUM && self.bank_configs[i].allocated
    }

    pub fn get_stats(&self) -> &BemuStats {
        &self.stats
    }

    pub fn reset_stats(&mut self) {
        self.stats = BemuStats::default();
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

    /// One 64-bit hash per bank (16 hex chars each, [`DefaultHasher`](std::collections::hash_map::DefaultHasher)).
    pub fn bank_hashes64_hex(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(self.banks.len());
        for b in &self.banks {
            out.push(bank_hash64(b));
        }
        out
    }
}

impl Default for Bemu {
    fn default() -> Self {
        Self::new()
    }
}
