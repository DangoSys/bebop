use crate::buckyball::frontend::Decoder;
use crate::buckyball::frontend::Rob;
use crate::buckyball::frontend::Rs;
use crate::buckyball::lib::operation::{ExternalOp, InternalOp};

pub struct Frontend {
  decoder: Decoder,
  rob: Rob,
  rs: Rs,

  decoded_inst: Option<(u32, u64, u64, u32)>,
  rob_dispatched_inst: Option<(u32, u64, u64, u32, u32)>,
  rs_issued_inst: Option<(u32, u64, u64, u32, u32)>,

  decoder_bp: bool,
  rob_bp: bool,
  rs_bp: bool,
}

impl Frontend {
  pub fn new() -> Self {
    Self {
      decoder: Decoder::new(),
      rob: Rob::new(16),
      rs: Rs::new(),

      decoded_inst: None,
      rob_dispatched_inst: None,
      rs_issued_inst: None,

      decoder_bp: false,
      rob_bp: false,
      rs_bp: false,
    }
  }

  // 0.5 -> 1.0 cycle
  pub fn new_inst(&mut self) -> FrontendNewInstExt {
    FrontendNewInstExt(self)
  }
  // 0.0 -> 0.5 cycle
  pub fn issue_inst(&mut self) -> FrontendIssueInstInt {
    FrontendIssueInstInt(self)
  }
}

/// ------------------------------------------------------------
/// --- Operations Definitions ---
/// ------------------------------------------------------------
pub struct FrontendNewInstExt<'a>(&'a mut Frontend);
impl<'a> ExternalOp for FrontendNewInstExt<'a> {
  type Input = Option<(u32, u64, u64)>;
  fn can_input(&self, ctrl: bool) -> bool {
    ctrl && !self.0.decoder_bp
  }

  fn has_input(&self, input: &Self::Input) -> bool {
    input.is_some()
  }
  
  fn execute(&mut self, input: &Self::Input) {
    if !self.has_input(input) {
      return;
    }
    let raw_inst = input.unwrap();

    if self.0.decoder.decode().can_input(!self.0.decoder_bp) {
      self.0.decoder.decode().execute(&Some(raw_inst));
    }

    if self.0.rob.allocate().can_input(!self.0.rob_bp) {
      let decoded = self.0.decoded_inst.unwrap();
      self.0.rob.allocate().execute(&Some((decoded.0, decoded.1, decoded.2, decoded.3)));
    }

    if self.0.rs.dispatch().can_input(!self.0.rs_bp) {
      let dispatched = self.0.rob_dispatched_inst.unwrap();
      self.0.rs.dispatch().execute(&Some((dispatched.0, dispatched.1, dispatched.2, dispatched.3, dispatched.4)));
    }
  }
}

pub struct FrontendIssueInstInt<'a>(&'a mut Frontend);
impl<'a> InternalOp for FrontendIssueInstInt<'a> {
  type Output = Option<(u32, u64, u64, u32, u32)>;
  fn has_output(&self) -> bool {
    self.0.rs_issued_inst.is_some()
  }
// ------------------------------------------------------------
// Update Stage
// ------------------------------------------------------------
  fn update(&mut self) {
    {
      let mut decoder_push = self.0.decoder.push_to_rob();
      decoder_push.update();
      if decoder_push.has_output() {
        self.0.decoded_inst = decoder_push.output();
      }
    }

    {
      let mut rob_dispatch = self.0.rob.dispatch();
      rob_dispatch.update();
      if rob_dispatch.has_output() {
        self.0.rob_dispatched_inst = rob_dispatch.output();
      }
    }

    {
      let mut rs_issue = self.0.rs.issue_to_specific_domain();
      rs_issue.update();
      if rs_issue.has_output() {
        self.0.rs_issued_inst = rs_issue.output();
      }
    }
// ------------------------------------------------------------
// Set Backpressure Stage
// ------------------------------------------------------------
    self.0.rob_bp = self.0.rs.dispatch().can_input(!self.0.rs_bp);
    self.0.decoder_bp = self.0.decoder.decode().can_input(!self.0.rob_bp);
  }
// ------------------------------------------------------------
// Output Stage
// ------------------------------------------------------------
  fn output(&mut self) -> Self::Output {
    if self.has_output() {
      return self.0.rs_issued_inst;
    }
    return None;
  }
}
