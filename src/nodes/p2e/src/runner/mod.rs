mod runworkload;
pub use bebop_memory::DDR_SIZE;

pub use runworkload::{
    configure_vvac_environment, generate_main_tcl, init_ctb, source_environment, start_vdbg_background,
    wait_for_completion, wait_for_flash, SimulationResult, VdbgProcess,
};

// Runner steps - use path attribute to map module names to directories with numeric prefixes
#[path = "0_flashbitstream/mod.rs"]
pub mod flashbitstream;
#[path = "1_init/mod.rs"]
pub mod init;
#[path = "2_runworkload/mod.rs"]
pub mod workload;

// Re-export step types for convenience
pub use flashbitstream::FlashBitstreamStep;
pub use init::InitStep;
pub use workload::RunWorkloadStep;
