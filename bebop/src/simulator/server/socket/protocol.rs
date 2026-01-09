use std::io::{Read, Result, Write};
use std::net::TcpStream;

// Socket configuration
pub const SOCKET_CMD_PORT: u16 = 6000;
pub const SOCKET_DMA_READ_PORT: u16 = 6001;
pub const SOCKET_DMA_WRITE_PORT: u16 = 6002;
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

pub fn peek_header(stream: &mut TcpStream) -> Result<MsgHeader> {
  use std::io::{Seek, SeekFrom};
  // We can't actually peek with TcpStream, so we need to read and put back
  // But TcpStream doesn't support seek, so we can't put back
  // Instead, read the header and reconstruct the stream position
  // Actually, we can't do this easily. Let's just read the header
  read_struct::<MsgHeader>(stream)
}

pub fn skip_message_by_type(stream: &mut TcpStream, msg_type: u32) -> Result<()> {
  let size = match msg_type {
    3 => std::mem::size_of::<DmaReadResp>(), // DmaReadResp
    5 => std::mem::size_of::<DmaWriteResp>(), // DmaWriteResp
    _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unknown msg_type: {}", msg_type))),
  };
  // We already read the header, so skip the rest (size - 8 bytes for header)
  let mut buf = vec![0u8; size - 8];
  stream.read_exact(&mut buf)?;
  Ok(())
}

pub fn write_struct<T: Sized>(stream: &mut TcpStream, data: &T) -> Result<()> {
  unsafe {
    let bytes = std::slice::from_raw_parts(data as *const T as *const u8, std::mem::size_of::<T>());
    stream.write_all(bytes)?;
    Ok(())
  }
}
