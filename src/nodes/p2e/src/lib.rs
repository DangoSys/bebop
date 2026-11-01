pub mod builder;
pub mod ctb;
pub mod runner;

mod result;

pub use builder::BitstreamBuilder;
pub use ctb::ffi;
pub use result::Result;
pub use runner::{
    configure_vvac_environment, generate_main_tcl, init_ctb, source_environment, start_vdbg_background,
    wait_for_completion, wait_for_flash, FlashBitstreamStep, InitStep, RunWorkloadStep, SimulationResult, VdbgProcess,
    DDR_SIZE,
};
