#[derive(Clone)]
pub struct ModelOptions {
    pub log_dir: std::path::PathBuf,
    pub trace: crate::trace::TraceConfig,
    pub disasm: bool,
    pub profile: bool,
}
