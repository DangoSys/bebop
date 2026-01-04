#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
  Func,
  Cycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepMode {
  Continuous,
  Step,
}

#[derive(Debug, Clone, Copy)]
pub struct SimConfig {
  pub run_mode: RunMode,
  pub quiet: bool,
  pub step_mode: StepMode,
}
