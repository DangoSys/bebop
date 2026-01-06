use crate::buckyball::balldomain::domain_decoder::DomainDecoder;
use crate::buckyball::balldomain::relu::RelBall;
use crate::buckyball::balldomain::transpose::TransBall;
use crate::buckyball::balldomain::vector::VectorBall;

pub struct BallDomain {
  domain_decoder: DomainDecoder,
  vecball: VectorBall,
  transball: TransBall,
  relball: RelBall,
}

impl BallDomain {
  pub fn new() -> Self {
    Self {
      domain_decoder: DomainDecoder::new(),
      vecball: VectorBall::new(),
      transball: TransBall::new(),
      relball: RelBall::new(),
    }
  }

  pub fn new_inst_ext(&mut self, received_inst: Option<(u32, u64, u64, u8, u32)>) -> bool {
    if received_inst.is_some() {
      let (funct, xs1, xs2, domain_id, rob_id) = received_inst.unwrap();
      println!(
        "BallDomain executed instruction: funct={:?}, xs1={:?}, xs2={:?}, domain_id={:?}, rob_id={:?}",
        funct, xs1, xs2, domain_id, rob_id
      );
      return true;
    }
    return false;
  }

  pub fn executed_int(&mut self) -> Option<(u32, u32, u64, u64, u8)> {
    None
  }
}
