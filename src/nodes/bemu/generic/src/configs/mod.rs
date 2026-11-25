pub mod design;

pub mod types;
pub use design::{
    alone, ant_config, ddr, ddr_beat_rows, hart_capacity, hart_component, hart_count, hart_ids, hart_parameters, load,
    shared_bank_fabric, thread_is_ant, thread_local_id, thread_signature, tss_config, workload_placement, AntConfig,
    Endpoint, GpDomainConfig,
};
pub use types::{AcceleratorParams, CoreParams};

mod accelerator;
pub use accelerator::with_accelerator;
pub mod ball_domain;

pub use accelerator::{
    bank_lines, bank_num, bank_row_bytes, bank_size, bank_width, configure, configure_thread, gpdomain,
    is_shared_vbank, max_private_vbank, mmio_bank_lines, mmio_bank_num, mmio_bank_row_bytes, mmio_bank_size,
    mmio_bank_width, mmio_enable, mmio_read_width, mmio_total_size, private_bank_geometry, shared_vbank_base,
    virtual_bank_num,
};
