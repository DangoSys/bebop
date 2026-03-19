use super::configs::config::{BankConfig, BemuStats, EmuConfig, BANK_NUM};
use super::instructions::matmul;
use super::instructions::mset;
use super::instructions::mvin;
use super::instructions::mvout;
use super::instructions::transpose;
use log::{debug, error, info};

pub struct Bemu {
    memory: Vec<u8>,
    banks: Vec<Vec<u8>>,
    stats: BemuStats,
    bank_configs: [BankConfig; BANK_NUM],
}

impl Bemu {
    pub fn new() -> Self {
        let cfg = EmuConfig::load().unwrap_or_else(|e| panic!("BEMU config load failed: {e}"));
        info!(
            "Creating Bemu (Golden Model)\n  Config: {} banks x {} bytes ({}KB total)",
            cfg.bank_num,
            cfg.bank_size(),
            cfg.total_memory_size() / 1024
        );
        Self {
            memory: vec![0; cfg.total_memory_size()],
            banks: (0..cfg.bank_num)
                .map(|_| vec![0; cfg.bank_size()])
                .collect(),
            stats: BemuStats::default(),
            bank_configs: [BankConfig::default(); BANK_NUM],
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

    fn execute_known(&mut self, funct: u32, xs1: u64, xs2: u64) -> Option<u64> {
        let ret = match funct {
            23 => mset::execute_mset(xs1, xs2, &mut self.bank_configs, &mut self.banks),
            24 => mvin::execute_mvin(xs1, xs2, &self.memory, &mut self.banks, &self.bank_configs),
            25 => mvout::execute_mvout(xs1, xs2, &mut self.memory, &self.banks, &self.bank_configs),
            32 => matmul::execute_mul_warp16(xs1, xs2, &mut self.banks),
            34 => transpose::execute_transpose(xs1, xs2, &mut self.banks),
            _ => return None,
        };
        Some(Self::encode_result(funct, ret))
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
        debug!(
            "Bemu executing: funct={}, xs1=0x{:x}, xs2=0x{:x}",
            funct, xs1, xs2
        );

        let result = match self.execute_known(funct, xs1, xs2) {
            Some(v) => v,
            None => {
                error!("Bemu: Unknown funct={}", funct);
                u64::MAX
            }
        };

        debug!("Bemu result: 0x{:x}", result);
        result
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
}

impl Default for Bemu {
    fn default() -> Self {
        Self::new()
    }
}
