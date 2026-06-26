mod runworkload;

pub use runworkload::{
    configure_vvac_environment, generate_main_tcl, init_ctb, source_environment, start_vdbg_background,
    wait_for_completion, wait_for_flash, SimulationResult,
};

// Runner steps - use path attribute to map module names to directories with numeric prefixes
#[path = "0_flashbitstream/mod.rs"]
mod flashbitstream_impl;
#[path = "1_init/mod.rs"]
mod init_impl;
#[path = "2_runworkload/mod.rs"]
mod workload_impl;

// Re-export as public modules with clean names
pub mod flashbitstream {
    pub use super::flashbitstream_impl::*;
}
pub mod init {
    pub use super::init_impl::*;
}
pub mod workload {
    pub use super::workload_impl::*;
}

// Re-export step types for convenience
pub use flashbitstream_impl::FlashBitstreamStep;
pub use init_impl::InitStep;
pub use workload_impl::RunWorkloadStep;
