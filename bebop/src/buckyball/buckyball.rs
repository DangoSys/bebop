use std::io;
use crate::buckyball::decoder::Decoder;
use crate::buckyball::rob::Rob;
use crate::buckyball::rs::Rs;

pub struct Buckyball {
  decoder: Decoder,
  rob: Rob,
  rs: Rs,

  decoded_inst: Option<(u32, u64, u64, u8)>,
  rob_dispatched_inst: Option<(u32, u32, u64, u64, u8)>,
  rs_issued_inst: Option<(u32, u32, u64, u64, u8, u32)>,

  decoded_inst_stall: bool,
  rob_allocated_stall: bool,
  rs_issued_stall: bool,
}

impl Buckyball {
  pub fn new() -> Self {
    Self {
      decoder: Decoder::new(),
      rob: Rob::new(16),
      rs: Rs::new(),
      
      decoded_inst: None,
      rob_dispatched_inst: None,
      rs_issued_inst: None,

      decoded_inst_stall: false,
      rob_allocated_stall: false,
      rs_issued_stall: false,
    }
  }

  // 0.5 -> 1.0 cycle
  pub fn forward_step(&mut self, raw_inst: Option<(u32, u64, u64)>) -> io::Result<()> {
    self.decoded_inst_stall = !self.decoder.inst_decode_ext(raw_inst);
    self.rob_allocated_stall = !self.rob.rob_allocate_ext(self.decoded_inst);
    self.rs_issued_stall = !self.rs.rs_issue_ext(self.rob_dispatched_inst);
    Ok(())
  }

  // 0.0 -> 0.5 cycle
  pub fn backward_step(&mut self) -> io::Result<()> {
    if !self.decoded_inst_stall {
      self.decoded_inst = self.decoder.push_to_rob_int();
    }
    if !self.rob_allocated_stall {
      self.rob_dispatched_inst = self.rob.rob_dispatch_int();
    }
    if !self.rs_issued_stall {
      self.rs_issued_inst = self.rs.rs_issue_int();
    }
    Ok(())
  }
}
