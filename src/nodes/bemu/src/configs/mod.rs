use std::cell::Cell;

pub(crate) mod design;

pub mod types;
pub use design::{
    ant_config, core_component, core_count, core_parameters, ddr, hart_capacity, hart_ids, load, shared_bank_fabric,
    thread_is_ant, thread_local_id, thread_signature, tss_config, workload_placement, AntConfig, Endpoint,
    GpDomainConfig,
};
pub use types::{AcceleratorParams, CoreParams};

thread_local! {
    static ACTIVE: Cell<Option<Endpoint>> = const { Cell::new(None) };
}
pub fn configure(endpoint: Endpoint) {
    ACTIVE.with(|slot| slot.set(Some(endpoint)));
}
pub fn configure_thread(index: usize) {
    configure(Endpoint::Thread(index));
}
fn with_accelerator<R>(f: impl FnOnce(&AcceleratorParams) -> R) -> R {
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

pub mod ball_domain {
    pub fn instance_for_funct(funct: u32) -> Option<u32> {
        super::with_accelerator(|p| {
            p.ball_domain
                .balls
                .iter()
                .find(|ball| ball.instructions.iter().any(|instruction| instruction.funct7 == funct))
                .map(|ball| ball.id)
        })
    }

    pub use super::design::parameters;
    use super::with_accelerator;

    pub fn ball_class_for_funct(funct7: u32) -> Option<String> {
        with_accelerator(|topology| {
            topology
                .ball_domain
                .balls
                .iter()
                .find(|ball| ball.instructions.iter().any(|entry| entry.funct7 == funct7))
                .map(|ball| ball.rtl_class.clone())
        })
    }

    pub fn mnemonic_for_funct(funct7: u32) -> Option<String> {
        with_accelerator(|topology| {
            topology
                .ball_domain
                .balls
                .iter()
                .flat_map(|ball| ball.instructions.iter())
                .find(|entry| entry.funct7 == funct7)
                .map(|entry| entry.mnemonic.clone())
        })
    }

    pub fn parameters<P: prost::Message + Default>(component: &str, instance: u32) -> P {
        with_accelerator(|topology| {
            let ball = topology
                .ball_domain
                .balls
                .iter()
                .find(|ball| ball.id == instance)
                .unwrap_or_else(|| panic!("missing Ball instance {instance}"));
            let parameter = ball.parameters.as_ref().expect("Ball parameters");
            assert_eq!(
                parameter.type_url,
                format!("type.googleapis.com/buckyball.balls.{component}.Params")
            );
            P::decode(parameter.value.as_slice()).expect("decode typed Ball parameters")
        })
    }
}

/// Private-bank geometry used by an in-process RTL DiffTest monitor.
/// Geometry follows design.pb baked at build time.
pub fn private_bank_geometry(endpoint: Endpoint) -> (usize, usize) {
    configure(endpoint);
    (bank_size(), bank_row_bytes())
}
