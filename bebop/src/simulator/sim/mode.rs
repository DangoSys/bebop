#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimMode {
  Step,
  Run,
}

#[derive(Debug, Clone, Copy)]
pub struct SimConfig {
  pub mode: SimMode,
  pub quiet: bool,
}
