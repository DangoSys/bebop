pub mod builder;
pub mod ctb;
pub mod runner;

pub use builder::BitstreamBuilder;
pub use ctb::ffi;
pub use runner::{
    configure_vvac_environment, generate_main_tcl, init_ctb, source_environment, start_vdbg_background,
    wait_for_completion, wait_for_flash, FlashBitstreamStep, InitStep, RunWorkloadStep, SimulationResult,
};

pub type Result<T> = std::result::Result<T, String>;
