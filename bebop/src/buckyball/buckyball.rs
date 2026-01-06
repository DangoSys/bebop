use std::io;
use crate::buckyball::decoder::Decoder;
use crate::buckyball::rob::Rob;
use crate::buckyball::rs::Rs;

pub struct Buckyball {
  decoder: Decoder,
  rob: Rob,
  rs: Rs,

  decoded_inst: Option<(u32, u64, u64, u8)>,
  rob_entry: Option<(u32, u32, u64, u64, u8)>,
  rs_entry: Option<(u32, u32, u64, u64, u8, u32)>,
}

impl Buckyball {
  pub fn new() -> Self {
    Self {
      decoder: Decoder::new(),
      rob: Rob::new(),
      rs: Rs::new(),
      
      decoded_inst: None,
      rob_entry: None,
      rs_entry: None,
    }
  }

  pub fn forward_step(&mut self, raw_inst: Option<(u32, u64, u64)>) -> io::Result<()> {
    self.decoder.inst_decode(raw_inst);
    self.rob.rob_allocate(self.decoded_inst);
    self.rs.rs_issue(self.rob_entry);
    Ok(())
  }

  pub fn backward_step(&mut self) -> io::Result<()> {
    self.decoded_inst = self.decoder.inst_decode_bw();
    self.rob_entry = self.rob.rob_allocate_bw();
    self.rs_entry = self.rs.rs_issue_bw();
    Ok(())
  }
}
