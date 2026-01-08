use crate::buckyball::balldomain::BallDomain;
use crate::buckyball::frontend::Frontend;
use crate::buckyball::lib::operation::{ExternalOp, InternalOp};
use crate::buckyball::memdomain::tdma_load::DmaInterface;
use crate::buckyball::memdomain::tdma_store::DmaWriteInterface;
use crate::buckyball::memdomain::MemDomain;
use std::io;

pub struct Buckyball {
  frontend: Frontend,
  memdomain: MemDomain,
  balldomain: BallDomain,

  inst_issued_to_mem: Option<(u32, u64, u64, u32)>,
  inst_issued_to_ball: Option<(u32, u64, u64, u32)>,

  memdomain_load_bp: bool,
  memdomain_store_bp: bool,
  balldomain_bp: bool,
}

impl Buckyball {
  pub fn new() -> Self {
    Self {
      frontend: Frontend::new(),
      memdomain: MemDomain::new(),
      balldomain: BallDomain::new(),

      inst_issued_to_mem: None,
      inst_issued_to_ball: None,

      memdomain_load_bp: false,
      memdomain_store_bp: false,
      balldomain_bp: false,
    }
  }

  // 0.5 -> 1.0 cycle Execute
  pub fn forward_step<D: DmaInterface + DmaWriteInterface>(
    &mut self,
    raw_inst: Option<(u32, u64, u64)>,
    _dma: &D,
  ) -> io::Result<()> {
// ------------------------------------------------------------
// Execute External Ops Stage
// ------------------------------------------------------------
    let mut frontend_new_inst = self.frontend.new_inst();
    let mut balldomain_new_inst = self.balldomain.new_inst();

    if frontend_new_inst.can_input(true) {
      frontend_new_inst.execute(&raw_inst);
    }

    // Handle load operation
    if self.memdomain.load_op().can_input(true) {
      self.memdomain.load_op().execute(&self.inst_issued_to_mem);
    }

    // Handle store operation
    if self.memdomain.store_op().can_input(true) {
      self.memdomain.store_op().execute(&self.inst_issued_to_mem);
    }

    if balldomain_new_inst.can_input(true) {
      balldomain_new_inst.execute(&self.inst_issued_to_ball);
    }
    Ok(())
  }

  // 0.0 -> 0.5 cycle
  pub fn backward_step<D: DmaInterface + DmaWriteInterface>(&mut self, dma: &D) -> io::Result<()> {
// ------------------------------------------------------------
// Update Stage
// ------------------------------------------------------------
    self.frontend.issue_inst().update();
    self.memdomain.execute_load(dma).update();
    self.memdomain.execute_store(dma).update();
    self.balldomain.execute_inst().update();
// ------------------------------------------------------------
// Output Stage    
// ------------------------------------------------------------
    let mut frontend_issue = self.frontend.issue_inst();
    if frontend_issue.has_output() {
      println!("[Buckyball] Issued instruction: {:?}", frontend_issue.output().unwrap().0);
      if let Some((funct, xs1, xs2, domain_id, rob_id)) = frontend_issue.output() {
        if domain_id == 1 {
          if funct == 24 && !self.memdomain.load_busy {
            self.inst_issued_to_mem = Some((funct, xs1, xs2, rob_id));
          } else if funct == 25 && !self.memdomain.store_busy {
            self.inst_issued_to_mem = Some((funct, xs1, xs2, rob_id));
          } 
        } else if domain_id == 2 {
          self.inst_issued_to_ball = Some((funct, xs1, xs2, rob_id));
        } else {
          return Err(io::Error::new(io::ErrorKind::Other, "Invalid domain id"));
        }
      }
    }
// ------------------------------------------------------------
// Set Backpressure Stage    
// ------------------------------------------------------------
    self.memdomain_load_bp = self.memdomain.load_busy;
    self.memdomain_store_bp = self.memdomain.store_busy;
    Ok(())
  }
}
