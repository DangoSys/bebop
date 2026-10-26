pub mod core_system;
pub(crate) mod input;
pub(crate) mod process;
pub mod run;
pub(crate) mod stdio;
pub(crate) mod user_vm;

#[derive(Clone)]
pub struct ModelOptions {
    pub log_dir: std::path::PathBuf,
    pub trace: crate::TraceConfig,
    pub disasm: bool,
    pub profile: bool,
}
