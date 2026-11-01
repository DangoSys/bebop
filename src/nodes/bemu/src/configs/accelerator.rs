use super::{design, AcceleratorParams, Endpoint, GpDomainConfig};
use std::cell::Cell;

thread_local! {
    static ACTIVE: Cell<Option<Endpoint>> = const { Cell::new(None) };
}
pub fn configure(endpoint: Endpoint) {
    ACTIVE.with(|slot| slot.set(Some(endpoint)));
}
pub fn configure_thread(index: usize) {
    configure(Endpoint::Thread(index));
}
pub(crate) fn with_accelerator<R>(f: impl FnOnce(&AcceleratorParams) -> R) -> R {
    ACTIVE.with(|slot| {
        f(
            design::accelerator_for(slot.get().expect("execution context is not bound"))
                .expect("this execution endpoint has no accelerator"),
        )
    })
}

pub fn bank_num() -> usize {
    with_accelerator(|t| t.mem_config.bank_num)
}
pub fn gpdomain() -> Option<GpDomainConfig> {
    with_accelerator(|topology| topology.gpdomain.clone())
}

pub fn virtual_bank_num() -> usize {
    with_accelerator(|topology| topology.virtual_bank_count)
}
pub fn max_private_vbank() -> usize {
    with_accelerator(|t| t.mem_config.max_private_vbank)
}
pub fn shared_vbank_base() -> usize {
    with_accelerator(|t| t.mem_config.shared_vbank_base)
}
pub fn is_shared_vbank(vbank: u64) -> bool {
    let vbank = usize::try_from(vbank).expect("vbank id exceeds usize");
    if vbank <= max_private_vbank() {
        return false;
    }
    if vbank >= shared_vbank_base() && vbank < virtual_bank_num() {
        return true;
    }
    panic!("invalid virtual bank id {vbank}");
}
pub fn bank_width() -> usize {
    with_accelerator(|t| t.mem_config.bank_width)
}
pub fn bank_lines() -> usize {
    with_accelerator(|t| t.mem_config.bank_entries)
}
pub fn bank_row_bytes() -> usize {
    bank_width() / 8
}
pub fn bank_size() -> usize {
    bank_lines() * bank_row_bytes()
}
pub fn mmio_enable() -> bool {
    with_accelerator(|t| t.mem_config.mmio_enable)
}
pub fn mmio_bank_num() -> usize {
    with_accelerator(|t| t.mem_config.mmio_bank_num)
}
pub fn mmio_bank_width() -> usize {
    with_accelerator(|t| t.mem_config.mmio_bank_width)
}
pub fn mmio_bank_lines() -> usize {
    with_accelerator(|t| t.mem_config.mmio_bank_entries)
}
pub fn mmio_bank_row_bytes() -> usize {
    mmio_bank_width() / 8
}
pub fn mmio_bank_size() -> usize {
    mmio_bank_lines() * mmio_bank_row_bytes()
}

#[allow(dead_code)]
pub fn mmio_read_width() -> usize {
    with_accelerator(|t| t.mem_config.mmio_read_width)
}
pub fn mmio_total_size() -> usize {
    mmio_bank_num() * mmio_bank_size()
}

/// Private-bank geometry used by an in-process RTL DiffTest monitor.
/// Geometry follows design.pb baked at build time.
pub fn private_bank_geometry(endpoint: Endpoint) -> (usize, usize) {
    configure(endpoint);
    (bank_size(), bank_row_bytes())
}
