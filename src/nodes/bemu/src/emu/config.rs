use std::cell::Cell;

mod chip_config;

pub use chip_config::{workload_placement, core_hart_id, hart_capacity, tile_for_core, core_signature, tile_count, tile_topology, RvvConfig, TileTopology, Topology};

thread_local! {
    static TOPOLOGY: Cell<Option<&'static Topology>> = const { Cell::new(None) };
}

pub fn configure_core(core_index: usize) {
    TOPOLOGY.with(|slot| slot.set(Some(chip_config::topology_for_core(core_index))));
}

fn with_topology<R>(f: impl FnOnce(&Topology) -> R) -> R {
    TOPOLOGY.with(|slot| f(slot.get().unwrap_or_else(|| panic!("BEMU topology is not configured"))))
}

pub fn bank_num() -> usize {
    with_topology(|t| t.mem_config.bank_num)
}
pub fn rvv() -> Option<RvvConfig> {
    with_topology(|topology| topology.rvv.clone())
}

pub fn virtual_bank_num() -> usize {
    with_topology(|topology| topology.virtual_bank_count)
}
pub fn private_vbank_upper_bound() -> usize {
    with_topology(|t| t.mem_config.private_vbank_upper_bound)
}
pub fn shared_vbank_base() -> usize {
    with_topology(|t| t.mem_config.shared_vbank_base)
}
pub fn is_shared_vbank(vbank: u64) -> bool {
    let vbank = usize::try_from(vbank).expect("vbank id exceeds usize");
    if vbank <= private_vbank_upper_bound() {
        return false;
    }
    if vbank >= shared_vbank_base() && vbank < virtual_bank_num() {
        return true;
    }
    panic!("invalid virtual bank id {vbank}");
}
pub fn bank_width() -> usize {
    with_topology(|t| t.mem_config.bank_width)
}
pub fn bank_lines() -> usize {
    with_topology(|t| t.mem_config.bank_entries)
}
pub fn bank_row_bytes() -> usize {
    bank_width() / 8
}
pub fn bank_size() -> usize {
    bank_lines() * bank_row_bytes()
}
pub fn mmio_enable() -> bool {
    with_topology(|t| t.mem_config.mmio_enable)
}
pub fn mmio_bank_num() -> usize {
    with_topology(|t| t.mem_config.mmio_bank_num)
}
pub fn mmio_bank_width() -> usize {
    with_topology(|t| t.mem_config.mmio_bank_width)
}
pub fn mmio_bank_lines() -> usize {
    with_topology(|t| t.mem_config.mmio_bank_entries)
}
pub fn mmio_bank_row_bytes() -> usize {
    mmio_bank_width() / 8
}
pub fn mmio_bank_size() -> usize {
    mmio_bank_lines() * mmio_bank_row_bytes()
}

#[allow(dead_code)]
pub fn mmio_read_width() -> usize {
    with_topology(|t| t.mem_config.mmio_read_width)
}
pub fn mmio_total_size() -> usize {
    mmio_bank_num() * mmio_bank_size()
}

pub mod ball_domain {
    use super::with_topology;

    pub fn ball_class_for_funct(funct7: u32) -> Option<String> {
        with_topology(|topology| {
            let bid = topology
                .ball_domain
                .isa
                .iter()
                .find(|entry| entry.funct7 == funct7)?
                .bid;
            topology
                .ball_domain
                .mappings
                .iter()
                .find(|mapping| mapping.ball_id == bid && mapping.builtin.is_empty())
                .map(|mapping| mapping.ball_class.clone())
        })
    }

    pub fn mnemonic_for_funct(funct7: u32) -> Option<String> {
        with_topology(|topology| {
            topology
                .ball_domain
                .isa
                .iter()
                .find(|entry| entry.funct7 == funct7)
                .map(|entry| entry.mnemonic.clone())
        })
    }

    pub fn param(ball_class: &str, name: &str) -> usize {
        with_topology(|topology| {
            topology
                .ball_domain
                .mappings
                .iter()
                .find(|mapping| mapping.ball_class == ball_class)
                .unwrap_or_else(|| panic!("missing Ball mapping for {ball_class}"))
                .ball_params
                .get(name)
                .unwrap_or_else(|| panic!("missing {name} parameter for {ball_class}"))
                .parse()
                .unwrap_or_else(|_| panic!("{name} parameter for {ball_class} is not an integer"))
        })
    }
}

/// Private-bank geometry used by an in-process RTL DiffTest monitor.
/// Geometry follows chip.pb baked at build time.
pub fn private_bank_geometry() -> (usize, usize) {
    configure_core(0);
    (bank_size(), bank_row_bytes())
}
