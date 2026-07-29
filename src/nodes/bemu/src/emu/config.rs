use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};

#[allow(dead_code)]
#[path = "../../build_support/config_loader.rs"]
mod config_loader;

use config_loader::Topology;

static TOPOLOGY: Lazy<Topology> = Lazy::new(|| config_loader::parse_topology(&top_config_path()));

fn top_config_path() -> PathBuf {
    let path = Path::new(crate::BEMU_TOP_CONFIG);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(path)
    }
}

pub fn bank_num() -> usize {
    TOPOLOGY.mem_config.bank_num
}

pub fn bank_width() -> usize {
    TOPOLOGY.mem_config.bank_width
}

pub fn bank_lines() -> usize {
    TOPOLOGY.mem_config.bank_entries
}

pub fn bank_row_bytes() -> usize {
    bank_width() / 8
}

pub fn bank_size() -> usize {
    bank_lines() * bank_row_bytes()
}

pub fn mmio_enable() -> bool {
    TOPOLOGY.mem_config.mmio_enable
}

pub fn mmio_bank_num() -> usize {
    TOPOLOGY.mem_config.mmio_bank_num
}

pub fn mmio_bank_width() -> usize {
    TOPOLOGY.mem_config.mmio_bank_width
}

pub fn mmio_bank_lines() -> usize {
    TOPOLOGY.mem_config.mmio_bank_entries
}

pub fn mmio_bank_row_bytes() -> usize {
    mmio_bank_width() / 8
}

pub fn mmio_bank_size() -> usize {
    mmio_bank_lines() * mmio_bank_row_bytes()
}

#[allow(dead_code)]
pub fn mmio_read_width() -> usize {
    TOPOLOGY.mem_config.mmio_read_width
}

pub fn mmio_total_size() -> usize {
    mmio_bank_num() * mmio_bank_size()
}

pub mod ball_domain {
    use super::TOPOLOGY;

    pub fn ball_class_for_funct(funct7: u32) -> Option<&'static str> {
        let bid = TOPOLOGY
            .ball_domain
            .isa
            .iter()
            .find(|entry| entry.funct7 == funct7)?
            .bid;
        TOPOLOGY
            .ball_domain
            .mappings
            .iter()
            .find(|mapping| mapping.ball_id == bid)
            .map(|mapping| mapping.ball_class.as_str())
    }
}
