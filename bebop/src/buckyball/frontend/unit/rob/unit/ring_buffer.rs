use crate::buckyball::frontend::unit::rob::bundles::decoder_rob::DecodedInstruction;

/// 环状队列（Ring Buffer）用于 ROB
#[derive(Debug, Clone)]
pub struct RingBuffer {
  buffer: Vec<Option<DecodedInstruction>>,
  head: usize,  // 读指针
  tail: usize,  // 写指针
  size: usize,  // 当前元素数量
  capacity: usize,
}

impl RingBuffer {
  pub fn new(capacity: usize) -> Self {
    Self {
      buffer: vec![None; capacity],
      head: 0,
      tail: 0,
      size: 0,
      capacity,
    }
  }

  /// 入队（push）
  pub fn push_in_rob(&mut self, inst: DecodedInstruction) -> bool {
    if self.is_full() {
      return false;
    }
    
    self.buffer[self.tail] = Some(inst);
    self.tail = (self.tail + 1) % self.capacity;
    self.size += 1;
    true
  }

  /// 出队（pop）
  pub fn pop_out_rob(&mut self) -> Option<DecodedInstruction> {
    if self.is_empty() {
      return None;
    }
    
    let inst = self.buffer[self.head].take();
    self.head = (self.head + 1) % self.capacity;
    self.size -= 1;
    inst
  }

  /// 查看队首元素（不移除）
  pub fn peek(&self) -> Option<&DecodedInstruction> {
    if self.is_empty() {
      None
    } else {
      self.buffer[self.head].as_ref()
    }
  }

  pub fn is_empty(&self) -> bool {
    self.size == 0
  }

  pub fn is_full(&self) -> bool {
    self.size == self.capacity
  }

  pub fn len(&self) -> usize {
    self.size
  }

  pub fn capacity(&self) -> usize {
    self.capacity
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_ring_buffer() {
    let mut rb = RingBuffer::new(4);
    assert!(rb.is_empty());
    
    let inst1 = DecodedInstruction::new(24, 0x100, 0x200, 0);
    let inst2 = DecodedInstruction::new(25, 0x300, 0x400, 1);
    
    assert!(rb.push_in_rob(inst1.clone()));
    assert!(rb.push_in_rob(inst2.clone()));
    assert_eq!(rb.len(), 2);
    
    let popped = rb.pop_out_rob().unwrap();
    assert_eq!(popped.funct, 24);
    assert_eq!(rb.len(), 1);
  }
}
