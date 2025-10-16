/// Message protocol definitions for Spike-Bebop communication
/// Matches the C++ structures in customext/include/socket.h

/// Request message from Spike (20 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct SocketMsg {
  pub funct: u32,
  pub xs1: u64,
  pub xs2: u64,
}

impl SocketMsg {
  pub const SIZE: usize = 20; // 4 + 8 + 8

  /// Parse from raw bytes (little-endian)
  pub fn from_bytes(bytes: &[u8; Self::SIZE]) -> Self {
    let funct = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let xs1 = u64::from_le_bytes([
      bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11],
    ]);
    let xs2 = u64::from_le_bytes([
      bytes[12], bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19],
    ]);

    Self { funct, xs1, xs2 }
  }
}

/// Response message to Spike (8 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct SocketResp {
  pub result: u64,
}

impl SocketResp {
  pub const SIZE: usize = 8;

  pub fn new(result: u64) -> Self {
    Self { result }
  }

  /// Convert to raw bytes (little-endian)
  pub fn to_bytes(&self) -> [u8; Self::SIZE] {
    self.result.to_le_bytes()
  }
}
