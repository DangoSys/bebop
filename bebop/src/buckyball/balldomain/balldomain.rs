use crate::buckyball::balldomain::domain_decoder::DomainDecoder;
use crate::buckyball::balldomain::relu::RelBall;
use crate::buckyball::balldomain::transpose::TransBall;
use crate::buckyball::balldomain::vector::VectorBall;
use crate::buckyball::lib::operation::{ExternalOp, InternalOp};

pub struct BallDomain {
  domain_decoder: DomainDecoder,
  vecball: VectorBall,
  transball: TransBall,
  relball: RelBall,

  received_inst: Option<(u32, u64, u64, u32)>,
  executed_inst: Option<(u32, u64, u64, u32)>,
}

impl BallDomain {
  pub fn new() -> Self {
    Self {
      domain_decoder: DomainDecoder::new(),
      vecball: VectorBall::new(),
      transball: TransBall::new(),
      relball: RelBall::new(),
      received_inst: None,
      executed_inst: None,
    }
  }

  pub fn new_inst(&mut self) -> BallDomainNewInst {
    BallDomainNewInst(self)
  }

  pub fn execute_inst(&mut self) -> BallDomainExecuteInstInt {
    BallDomainExecuteInstInt(self)
  }

  pub fn new_inst_ext(&mut self, received_inst: Option<(u32, u64, u64, u32)>) -> bool {
    if received_inst.is_some() {
      let (funct, xs1, xs2, rob_id) = received_inst.unwrap();
      println!(
        "BallDomain executed instruction: funct={:?}, xs1={:?}, xs2={:?}, rob_id={:?}",
        funct, xs1, xs2, rob_id
      );
      return true;
    }
    return false;
  }

  pub fn executed_int(&mut self) -> Option<(u32, u32, u64, u64)> {
    None
  }
}

/// ------------------------------------------------------------
/// --- Operations Definitions ---
/// ------------------------------------------------------------
pub struct BallDomainNewInst<'a>(&'a mut BallDomain);
impl<'a> ExternalOp for BallDomainNewInst<'a> {
  type Input = Option<(u32, u64, u64, u32)>;

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
    self.0.received_inst = *input;
    if let Some((_funct, _xs1, _xs2, _rob_id)) = input {
      self.0.domain_decoder.new_inst_ext(*input);
      if let Some((decoded_funct, decoded_xs1, decoded_xs2, decoded_rob_id, ball_id)) = self.0.domain_decoder.exec_int()
      {
        match ball_id {
          0 => {
            self
              .0
              .vecball
              .new_inst_ext(Some((decoded_funct, decoded_xs1, decoded_xs2, decoded_rob_id)));
          },
          1 => {
            self
              .0
              .transball
              .new_inst_ext(Some((decoded_funct, decoded_xs1, decoded_xs2, decoded_rob_id)));
          },
          2 => {
            self
              .0
              .relball
              .new_inst_ext(Some((decoded_funct, decoded_xs1, decoded_xs2, decoded_rob_id)));
          },
          _ => {},
        }
      }
    }
  }
}

pub struct BallDomainExecuteInstInt<'a>(&'a mut BallDomain);
impl<'a> InternalOp for BallDomainExecuteInstInt<'a> {
  type Output = Option<(u32, u64, u64, u32)>;

  fn has_output(&self) -> bool {
    self.0.executed_inst.is_some()
  }

  fn update(&mut self) {
    if let Some((funct, xs1, xs2, rob_id)) = self.0.received_inst {
      if let Some((_decoded_funct, _decoded_xs1, _decoded_xs2, _decoded_rob_id, ball_id)) =
        self.0.domain_decoder.exec_int()
      {
        match ball_id {
          0 => {
            if let Some(_) = self.0.vecball.exec_int() {
              self.0.executed_inst = Some((funct, xs1, xs2, rob_id));
            }
          },
          1 => {
            if let Some(_) = self.0.transball.exec_int() {
              self.0.executed_inst = Some((funct, xs1, xs2, rob_id));
            }
          },
          2 => {
            if let Some(_) = self.0.relball.exec_int() {
              self.0.executed_inst = Some((funct, xs1, xs2, rob_id));
            }
          },
          _ => {},
        }
      }
    }
  }

  fn output(&mut self) -> Self::Output {
    if self.has_output() {
      let result = self.0.executed_inst;
      self.0.executed_inst = None;
      return result;
    }
    return None;
  }
}
