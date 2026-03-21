use super::bank::{BankConfig, BankMap, BANK_NUM};
use super::configs::config::{BemuStats, EmuConfig};
use super::inst::decode::SyncPlan;
use super::inst::decode::{self};

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

    pub fn get_stats(&self) -> &BemuStats {
        &self.stats
    }

    pub fn reset_stats(&mut self) {
        self.stats = BemuStats::default();
    }

    pub fn write_memory(&mut self, addr: u64, data: &[u8]) {
        let len = self.memory.len();
        for (i, &byte) in data.iter().enumerate() {
            let idx = ((addr as usize) + i) % len;
            self.memory[idx] = byte;
        }
    }

    pub fn read_memory(&self, addr: u64, size: usize) -> Vec<u8> {
        let len = self.memory.len();
        (0..size)
            .map(|i| self.memory[((addr as usize) + i) % len])
            .collect()
    }

    /// Deterministic 128-bit hash of all bank bytes (hex string, 32 chars).
    pub fn banks_hash128_hex(&self) -> String {
        let mut h0: u64 = 0x6c62272e07bb0142;
        let mut h1: u64 = 0x62b821756295c58d;
        const P0: u64 = 0x0000_0100_0000_01b3;
        const P1: u64 = 0x9e37_79b1_85eb_ca87;
        for bank in &self.banks {
            for &b in bank {
                h0 ^= b as u64;
                h0 = h0.wrapping_mul(P0);
                h1 ^= (b as u64).wrapping_add(0x9e37_79b9);
                h1 = h1.rotate_left(7).wrapping_mul(P1);
            }
        }
        format!("{h0:016x}{h1:016x}")
    }
}

impl Default for Bemu {
    fn default() -> Self {
        Self::new()
    }
}
