use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::super::bank::{BANK_LINES, BANK_NUM, BANK_WIDTH};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EmuMode {
    #[default]
    Iss,
    Fss,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EmuConfig {
    pub bank_num: usize,
    pub bank_width: usize,
    pub bank_lines: usize,
    #[serde(default)]
    pub emu_mode: EmuMode,
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

    /// Default locations only (packaged `share/bebop` or same-machine checkout). Use
    /// [`load_from`](Self::load_from) when passing `bebop --config`.
    pub fn load() -> Result<Self, String> {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let bundled = dir.join("../share/bebop/config.toml");
                if bundled.is_file() {
                    return Self::load_from(&bundled);
                }
            }
        }
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/emu/configs/config.toml");
        if dev.is_file() {
            return Self::load_from(&dev);
        }
        Err(
            "BEMU config not found: use `bebop --config PATH ...`, install share/bebop/config.toml \
             next to the binary, or run from a checkout where src/emu/configs/config.toml exists"
                .into(),
        )
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
        Ok(())
    }
}
