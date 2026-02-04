use super::protocol::*;
use std::io::{self, Read, Write, Result};
use std::net::TcpStream;

// Verilator server ports (different from Bebop's 6000-6002)
const VERILATOR_CMD_PORT: u16 = 7000;
const VERILATOR_DMA_READ_PORT: u16 = 7001;
const VERILATOR_DMA_WRITE_PORT: u16 = 7002;
const VERILATOR_HOST: &str = "127.0.0.1";

pub struct VerilatorClient {
  cmd_stream: TcpStream,
  dma_read_stream: TcpStream,
  dma_write_stream: TcpStream,
}

impl VerilatorClient {
  pub fn connect() -> Result<Self> {
    eprintln!("[VerilatorClient] Connecting to Verilator server...");

    // Connect to CMD port
    let cmd_stream = TcpStream::connect(format!("{}:{}", VERILATOR_HOST, VERILATOR_CMD_PORT))
      .map_err(|e| {
        io::Error::new(
          io::ErrorKind::ConnectionRefused,
          format!("Failed to connect to Verilator CMD port {}: {}", VERILATOR_CMD_PORT, e),
        )
      })?;
    eprintln!("[VerilatorClient] Connected to CMD port {}", VERILATOR_CMD_PORT);

    // Connect to DMA Read port
    let dma_read_stream = TcpStream::connect(format!("{}:{}", VERILATOR_HOST, VERILATOR_DMA_READ_PORT))
      .map_err(|e| {
        io::Error::new(
          io::ErrorKind::ConnectionRefused,
          format!("Failed to connect to Verilator DMA Read port {}: {}", VERILATOR_DMA_READ_PORT, e),
        )
      })?;
    eprintln!("[VerilatorClient] Connected to DMA Read port {}", VERILATOR_DMA_READ_PORT);

    // Connect to DMA Write port
    let dma_write_stream = TcpStream::connect(format!("{}:{}", VERILATOR_HOST, VERILATOR_DMA_WRITE_PORT))
      .map_err(|e| {
        io::Error::new(
          io::ErrorKind::ConnectionRefused,
          format!("Failed to connect to Verilator DMA Write port {}: {}", VERILATOR_DMA_WRITE_PORT, e),
        )
      })?;
    eprintln!("[VerilatorClient] Connected to DMA Write port {}", VERILATOR_DMA_WRITE_PORT);

    Ok(Self {
      cmd_stream,
      dma_read_stream,
      dma_write_stream,
    })
  }

  // Send CMD request and receive response
  pub fn send_cmd(&mut self, funct: u32, xs1: u64, xs2: u64) -> Result<u64> {
    // Send CMD request
    let req = CmdReq {
      header: MsgHeader {
        msg_type: MsgType::CmdReq as u32,
        reserved: 0,
      },
      funct,
      padding: 0,
      xs1,
      xs2,
    };

    write_struct(&mut self.cmd_stream, &req)?;
    self.cmd_stream.flush()?;

    // Receive CMD response
    let resp: CmdResp = read_struct(&mut self.cmd_stream)?;

    Ok(resp.result)
  }

  // Handle DMA read request from Verilator
  pub fn handle_dma_read_request<F>(&mut self, read_cb: F) -> Result<()>
  where
    F: Fn(u64, u32) -> (u64, u64), // (addr, size) -> (data_lo, data_hi)
  {
    // Receive DMA read request
    let req: DmaReadReq = read_struct(&mut self.dma_read_stream)?;

    // Call callback to read from memory
    let (data_lo, data_hi) = read_cb(req.addr, req.size);

    // Send DMA read response
    let resp = DmaReadResp {
      header: MsgHeader {
        msg_type: MsgType::DmaReadResp as u32,
        reserved: 0,
      },
      data_lo,
      data_hi,
    };

    write_struct(&mut self.dma_read_stream, &resp)?;
    self.dma_read_stream.flush()?;

    Ok(())
  }

  // Handle DMA write request from Verilator
  pub fn handle_dma_write_request<F>(&mut self, write_cb: F) -> Result<()>
  where
    F: Fn(u64, u64, u64, u32), // (addr, data_lo, data_hi, size)
  {
    // Receive DMA write request
    let req: DmaWriteReq = read_struct(&mut self.dma_write_stream)?;

    // Call callback to write to memory
    write_cb(req.addr, req.data_lo, req.data_hi, req.size);

    // Send DMA write response
    let resp = DmaWriteResp {
      header: MsgHeader {
        msg_type: MsgType::DmaWriteResp as u32,
        reserved: 0,
      },
      reserved: 0,
    };

    write_struct(&mut self.dma_write_stream, &resp)?;
    self.dma_write_stream.flush()?;

    Ok(())
  }

  // Blocking receive DMA read request
  pub fn recv_dma_read_request(&mut self) -> Result<DmaReadReq> {
    read_struct(&mut self.dma_read_stream)
  }

  // Blocking receive DMA write request
  pub fn recv_dma_write_request(&mut self) -> Result<DmaWriteReq> {
    read_struct(&mut self.dma_write_stream)
  }

  // Try to receive DMA read request (non-blocking)
  pub fn try_recv_dma_read_request(&mut self) -> Result<Option<DmaReadReq>> {
    self.dma_read_stream.set_nonblocking(true)?;

    let result = match read_struct::<DmaReadReq>(&mut self.dma_read_stream) {
      Ok(req) => Ok(Some(req)),
      Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
      Err(e) => Err(e),
    };

    self.dma_read_stream.set_nonblocking(false)?;
    result
  }

  // Try to receive DMA write request (non-blocking)
  pub fn try_recv_dma_write_request(&mut self) -> Result<Option<DmaWriteReq>> {
    self.dma_write_stream.set_nonblocking(true)?;

    let result = match read_struct::<DmaWriteReq>(&mut self.dma_write_stream) {
      Ok(req) => Ok(Some(req)),
      Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
      Err(e) => Err(e),
    };

    self.dma_write_stream.set_nonblocking(false)?;
    result
  }

  pub fn send_dma_read_response(&mut self, data_lo: u64, data_hi: u64) -> Result<()> {
    let resp = DmaReadResp {
      header: MsgHeader {
        msg_type: MsgType::DmaReadResp as u32,
        reserved: 0,
      },
      data_lo,
      data_hi,
    };

    write_struct(&mut self.dma_read_stream, &resp)?;
    self.dma_read_stream.flush()?;
    Ok(())
  }

  pub fn send_dma_write_response(&mut self) -> Result<()> {
    let resp = DmaWriteResp {
      header: MsgHeader {
        msg_type: MsgType::DmaWriteResp as u32,
        reserved: 0,
      },
      reserved: 0,
    };

    write_struct(&mut self.dma_write_stream, &resp)?;
    self.dma_write_stream.flush()?;
    Ok(())
  }
}
