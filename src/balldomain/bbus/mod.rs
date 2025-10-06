// BBus: interconnect between Ball Domain and Mem Domain

use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct BusTransaction {
  pub src: String,
  pub dst: String,
  pub addr: u64,
  pub data: Vec<f32>,
}

pub struct BBus {
  queue: VecDeque<BusTransaction>,
  total_transfers: usize,
}

impl BBus {
  pub fn new() -> Self {
    Self {
      queue: VecDeque::new(),
      total_transfers: 0,
    }
  }

  pub fn send(&mut self, trans: BusTransaction) {
    println!(
      "[BBus] {} -> {}: addr=0x{:x}, data_len={}",
      trans.src, trans.dst, trans.addr, trans.data.len()
    );
    self.queue.push_back(trans);
    self.total_transfers += 1;
  }

  pub fn recv(&mut self) -> Option<BusTransaction> {
    self.queue.pop_front()
  }

  pub fn has_pending(&self) -> bool {
    !self.queue.is_empty()
  }

  pub fn get_total_transfers(&self) -> usize {
    self.total_transfers
  }
}

