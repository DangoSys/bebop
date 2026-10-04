pub mod builder;
pub mod ctb;
pub mod runner;

pub use builder::{BitstreamBuilder, BuildOutcome};
pub use ctb::ffi;
pub use runner::{
    configure_vvac_environment, generate_main_tcl, init_ctb, source_environment, start_vdbg_background,
    validate_loads, wait_for_completion, wait_for_flash, FlashBitstreamStep, InitStep, LoadPlan,
    RunWorkloadStep, SimulationResult, VdbgProcess,
};

pub type Result<T> = std::result::Result<T, String>;
