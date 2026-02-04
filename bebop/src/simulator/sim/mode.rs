#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepMode {
  Continuous,
  Step,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchType {
  Buckyball,
  Gemmini,
  VerilatorRTL,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostType {
  Spike,
  Gem5,
}
