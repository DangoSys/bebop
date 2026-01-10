#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMode {
  Continuous,
  Step,
}

#[derive(Debug, Clone, Copy)]
pub struct SimConfig {
  pub quiet: bool,
  pub step_mode: StepMode,
}
