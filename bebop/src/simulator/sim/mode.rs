#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepMode {
  Continuous,
  Step,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchType {
  Buckyball,
  Gemmini,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostType {
  Spike,
  Gem5,
}

#[derive(Debug, Clone)]
pub struct SimConfig {
  pub quiet: bool,
  pub step_mode: StepMode,
  pub trace_file: Option<String>,
  pub arch_type: ArchType,
  pub host_type: HostType,
  pub host_config: Option<String>,
}
