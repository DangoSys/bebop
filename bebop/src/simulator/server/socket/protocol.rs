use std::io::{Read, Result, Write};
use std::net::TcpStream;

// Socket configuration
pub const SOCKET_PORT: u16 = 9999;
pub const SOCKET_HOST: &str = "127.0.0.1";

// Message types
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MsgType {
  CmdReq = 0,
  CmdResp = 1,
  DmaReadReq = 2,
  DmaReadResp = 3,
  DmaWriteReq = 4,
  DmaWriteResp = 5,
}

// Message header
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MsgHeader {
  pub msg_type: u32,
  pub reserved: u32,
}

// Command request
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CmdReq {
  pub header: MsgHeader,
  pub funct: u32,
  pub padding: u32,
  pub xs1: u64,
  pub xs2: u64,
}

// Command response
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CmdResp {
  pub header: MsgHeader,
  pub result: u64,
}

// DMA read request
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct DmaReadReq {
  pub header: MsgHeader,
  pub size: u32,
  pub padding: u32,
  pub addr: u64,
}

// DMA read response
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct DmaReadResp {
  pub header: MsgHeader,
  pub data_lo: u64, // low 64 bits
  pub data_hi: u64, // high 64 bits
}

// DMA write request
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct DmaWriteReq {
  pub header: MsgHeader,
  pub size: u32,
  pub padding: u32,
  pub addr: u64,
  pub data_lo: u64, // low 64 bits
  pub data_hi: u64, // high 64 bits
}

// DMA write response
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct DmaWriteResp {
  pub header: MsgHeader,
  pub reserved: u64,
}

// Helper functions for reading/writing structs
pub fn read_struct<T: Sized>(stream: &mut TcpStream) -> Result<T> {
  unsafe {
    let mut data: T = std::mem::zeroed();
    let bytes = std::slice::from_raw_parts_mut(&mut data as *mut T as *mut u8, std::mem::size_of::<T>());
    stream.read_exact(bytes)?;
    Ok(data)
  }
}

pub fn write_struct<T: Sized>(stream: &mut TcpStream, data: &T) -> Result<()> {
  unsafe {
    let bytes = std::slice::from_raw_parts(data as *const T as *const u8, std::mem::size_of::<T>());
    stream.write_all(bytes)?;
    Ok(())
  }
}
