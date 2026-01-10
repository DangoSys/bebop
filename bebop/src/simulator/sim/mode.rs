#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepMode {
  Continuous,
  Step,
}

#[derive(Debug, Clone)]
pub struct SimConfig {
  pub quiet: bool,
  pub step_mode: StepMode,
  pub trace_file: Option<String>,
}
