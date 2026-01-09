use super::protocol::*;
use std::io::Result;
use std::net::TcpStream;

#[derive(Debug)]
pub struct DmaReadHandler {
  stream: TcpStream,
}

impl Clone for DmaReadHandler {
  fn clone(&self) -> Self {
    Self {
      stream: self.stream.try_clone().expect("Failed to clone TcpStream"),
    }
  }
}

impl DmaReadHandler {
  pub fn new(stream: TcpStream) -> Self {
    Self { stream }
  }

  /// Send DMA read request to client
  pub fn send_read_request(&mut self, addr: u64, size: u32) -> Result<()> {
    println!("[DmaReadHandler] Sending DMA read request: addr=0x{:x}, size={}", addr, size);
    let req = DmaReadReq {
      header: MsgHeader {
        msg_type: MsgType::DmaReadReq as u32,
        reserved: 0,
      },
      size,
      padding: 0,
      addr,
    };
    write_struct(&mut self.stream, &req)?;
    Ok(())
  }

  /// Receive DMA read response from client
  pub fn recv_read_response(&mut self) -> Result<u128> {
    let resp: DmaReadResp = read_struct(&mut self.stream)?;
    let data = (resp.data_hi as u128) << 64 | (resp.data_lo as u128);
    Ok(data)
  }

  /// Perform DMA read (send request + receive response)
  pub fn read(&mut self, addr: u64, size: u32) -> Result<u128> {
    self.send_read_request(addr, size)?;
    self.recv_read_response()
  }
}

#[derive(Debug)]
pub struct DmaWriteHandler {
  stream: TcpStream,
}

impl Clone for DmaWriteHandler {
  fn clone(&self) -> Self {
    Self {
      stream: self.stream.try_clone().expect("Failed to clone TcpStream"),
    }
  }
}

impl DmaWriteHandler {
  pub fn new(stream: TcpStream) -> Self {
    Self { stream }
  }

  /// Send DMA write request to client
  pub fn send_write_request(&mut self, addr: u64, data: u128, size: u32) -> Result<()> {
    let data_lo = data as u64;
    let data_hi = (data >> 64) as u64;
    let req = DmaWriteReq {
      header: MsgHeader {
        msg_type: MsgType::DmaWriteReq as u32,
        reserved: 0,
      },
      size,
      padding: 0,
      addr,
      data_lo,
      data_hi,
    };
    write_struct(&mut self.stream, &req)
  }

  /// Receive DMA write response from client
  pub fn recv_write_response(&mut self) -> Result<()> {
    let _resp: DmaWriteResp = read_struct(&mut self.stream)?;
    Ok(())
  }

  /// Perform DMA write (send request + receive response)
  pub fn write(&mut self, addr: u64, data: u128, size: u32) -> Result<()> {
    self.send_write_request(addr, data, size)?;
    self.recv_write_response()
  }
}

// Keep DmaHandler for backward compatibility, but it's deprecated
#[derive(Debug)]
pub struct DmaHandler {
  read_handler: DmaReadHandler,
  write_handler: DmaWriteHandler,
}

impl DmaHandler {
  pub fn new(read_stream: TcpStream, write_stream: TcpStream) -> Self {
    Self {
      read_handler: DmaReadHandler::new(read_stream),
      write_handler: DmaWriteHandler::new(write_stream),
    }
  }

  pub fn read(&mut self, addr: u64, size: u32) -> Result<u128> {
    self.read_handler.read(addr, size)
  }

  pub fn write(&mut self, addr: u64, data: u128, size: u32) -> Result<()> {
    self.write_handler.write(addr, data, size)
  }
}
