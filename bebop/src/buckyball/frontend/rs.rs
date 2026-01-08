use crate::buckyball::lib::operation::{ExternalOp, InternalOp};

pub struct Rs {
  inst_to_issue: Option<(u32, u64, u64, u32, u32)>,
}

impl Rs {
  pub fn new() -> Self {
    Self { inst_to_issue: None }
  }

  pub fn dispatch(&mut self) -> RsInstDispatchExt {
    RsInstDispatchExt(self)
  }
  pub fn issue_to_specific_domain(&mut self) -> RsIssueToSpecificDomainInt {
    RsIssueToSpecificDomainInt(self)
  }
}

/// ------------------------------------------------------------
/// --- Operations Definitions ---
/// ------------------------------------------------------------
pub struct RsInstDispatchExt<'a>(&'a mut Rs);
impl<'a> ExternalOp for RsInstDispatchExt<'a> {
  type Input = Option<(u32, u64, u64, u32, u32)>;
  fn can_input(&self, ctrl: bool) -> bool {
    ctrl && true
  }
  fn has_input(&self, input: &Self::Input) -> bool {
    input.is_some()
  }
  fn execute(&mut self, input: &Self::Input) {
    if !self.has_input(input) {
      return;
    }
    self.0.inst_to_issue = Some(input.unwrap());
  }
}

pub struct RsIssueToSpecificDomainInt<'a>(&'a mut Rs);
impl<'a> InternalOp for RsIssueToSpecificDomainInt<'a> {
  type Output = Option<(u32, u64, u64, u32, u32)>;

  fn has_output(&self) -> bool {
    self.0.inst_to_issue.is_some()
  }

  fn update(&mut self) {}

  fn output(&mut self) -> Self::Output {
    if self.has_output() {
      return self.0.inst_to_issue;
    }
    return None;
  }
}
