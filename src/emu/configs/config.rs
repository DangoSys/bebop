use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const BANK_NUM: usize = 32;
pub const BANK_WIDTH: usize = 128;
pub const BANK_LINES: usize = 1024;
pub const BANK_SIZE: usize = BANK_LINES * (BANK_WIDTH / 8);
pub const MATRIX_SIZE: usize = 16;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EmuConfig {
    pub bank_num: usize,
    pub bank_width: usize,
    pub bank_lines: usize,
    pub matrix_size: usize,
}

impl EmuConfig {
    pub fn load_from(path: &Path) -> Result<Self, String> {
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("failed to read config {}: {e}", path.display()))?;
        let cfg: EmuConfig = toml::from_str(&raw)
            .map_err(|e| format!("failed to parse config {}: {e}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn load() -> Result<Self, String> {
        let root = std::env::var("BEBOP_PATH").map_err(|_| "BEBOP_PATH is not set".to_string())?;
        let path = Path::new(&root).join("src/emu/configs/config.toml");
        Self::load_from(path.as_path())
    }

    pub fn total_memory_size(&self) -> usize {
        self.bank_num * self.bank_lines * (self.bank_width / 8)
    }

    pub fn bank_size(&self) -> usize {
        self.bank_lines * (self.bank_width / 8)
    }

    fn validate(&self) -> Result<(), String> {
        if self.bank_num != BANK_NUM {
            return Err(format!(
                "bank_num mismatch: got {}, expect {}",
                self.bank_num, BANK_NUM
            ));
        }
        if self.bank_width != BANK_WIDTH {
            return Err(format!(
                "bank_width mismatch: got {}, expect {}",
                self.bank_width, BANK_WIDTH
            ));
        }
        if self.bank_lines != BANK_LINES {
            return Err(format!(
                "bank_lines mismatch: got {}, expect {}",
                self.bank_lines, BANK_LINES
            ));
        }
        if self.matrix_size != MATRIX_SIZE {
            return Err(format!(
                "matrix_size mismatch: got {}, expect {}",
                self.matrix_size, MATRIX_SIZE
            ));
        }
        Ok(())
    }
}

#[derive(Default, Clone, Copy, Debug, Deserialize, Serialize)]
pub struct BemuStats {
    pub instructions_executed: u64,
    pub matmul_count: u64,
    pub mset_count: u64,
    pub mvin_count: u64,
    pub mvout_count: u64,
    pub transpose_count: u64,
}

#[derive(Default, Clone, Copy, Debug, Deserialize, Serialize)]
pub struct BankConfig {
    pub allocated: bool,
    pub rows: u64,
    pub cols: u64,
    pub bank_id: u64,
}
