use std::cell::RefCell;

mod chip_config;

pub use chip_config::{rushb_endpoint, tile_topology, virtual_bank_count_for_core, TileTopology, Topology};

thread_local! {
    static TOPOLOGY: RefCell<Option<Topology>> = const { RefCell::new(None) };
    static VIRTUAL_BANK_COUNT: RefCell<Option<usize>> = const { RefCell::new(None) };
}

pub fn configure_core(core_index: usize) {
    TOPOLOGY.with(|slot| *slot.borrow_mut() = Some(chip_config::topology_for_core(core_index)));
    VIRTUAL_BANK_COUNT.with(|slot| {
        *slot.borrow_mut() = Some(chip_config::virtual_bank_count_for_core(core_index))
    });
}

pub fn configure_default() {
    configure_core(0);
}

pub fn configure_core_with_virtual_bank_count(core_index: usize, virtual_bank_count: usize) {
    configure_core(core_index);
    assert!(
        shared_vbank_base() > private_vbank_upper_bound(),
        "shared virtual bank range overlaps private virtual banks"
    );
    assert!(
        virtual_bank_count >= shared_vbank_base(),
        "virtual bank count ends before the shared bank base"
    );
    VIRTUAL_BANK_COUNT.with(|slot| *slot.borrow_mut() = Some(virtual_bank_count));
}

fn with_topology<R>(f: impl FnOnce(&Topology) -> R) -> R {
    TOPOLOGY.with(|slot| {
        let borrow = slot.borrow();
        let topology = borrow.as_ref().unwrap_or_else(|| panic!("BEMU topology is not configured"));
        f(topology)
    })
}

pub fn bank_num() -> usize {
    with_topology(|t| t.mem_config.bank_num)
}
pub fn vector_len() -> usize {
    with_topology(|t| t.vector_len)
}
pub fn virtual_bank_num() -> usize {
    VIRTUAL_BANK_COUNT.with(|slot| slot.borrow().unwrap_or_else(|| bank_num()))
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
                .find(|mapping| mapping.ball_id == bid)
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

    pub fn funct_for_mnemonic(mnemonic: &str) -> Option<u32> {
        with_topology(|topology| {
            topology
                .ball_domain
                .isa
                .iter()
                .find(|entry| entry.mnemonic == mnemonic)
                .map(|entry| entry.funct7)
        })
    }

    pub fn out_bw(ball_class: &str) -> usize {
        with_topology(|topology| {
            topology
                .ball_domain
                .mappings
                .iter()
                .find(|mapping| mapping.ball_class == ball_class)
                .unwrap_or_else(|| panic!("missing Ball mapping for {ball_class}"))
                .out_bw as usize
        })
    }
}
