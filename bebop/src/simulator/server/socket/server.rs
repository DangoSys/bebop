use std::net::{TcpListener, TcpStream};
use std::io::{Result, Error, ErrorKind};
use super::protocol::*;

pub type CmdHandler = Box<dyn FnMut(u32, u64, u64, &mut dyn DmaInterface) -> u64 + Send>;

pub trait DmaInterface {
  fn dma_read(&mut self, addr: u64, size: u32) -> Result<u128>;
  fn dma_write(&mut self, addr: u64, data: u128, size: u32) -> Result<()>;
}

pub struct SocketServer {
  listener: TcpListener,
  cmd_handler: Option<CmdHandler>,
}

impl SocketServer {
  pub fn new() -> Result<Self> {
    let addr = format!("{}:{}", SOCKET_HOST, SOCKET_PORT);
    let listener = TcpListener::bind(&addr)?;
    Ok(Self {
      listener,
      cmd_handler: None,
    })
  }

  pub fn set_cmd_handler<F>(&mut self, handler: F)
  where
    F: FnMut(u32, u64, u64, &mut dyn DmaInterface) -> u64 + Send + 'static,
  {
    self.cmd_handler = Some(Box::new(handler));
  }

  pub fn accept_and_serve(&mut self) -> Result<()> {
    let (stream, addr) = self.listener.accept()?;
    
    if let Err(e) = self.serve_client(stream) {
      eprintln!("Error serving client: {}", e);
    }
    
    Ok(())
  }

  fn serve_client(&mut self, mut stream: TcpStream) -> Result<()> {
    loop {
      let cmd_req = read_struct::<CmdReq>(&mut stream)?;
      
      if cmd_req.header.msg_type != MsgType::CmdReq as u32 {
        return Err(Error::new(ErrorKind::InvalidData, "Invalid message type"));
      }

      let mut dma_iface = ClientDma { stream: &mut stream };
      
      let result = if let Some(ref mut handler) = self.cmd_handler {
        handler(cmd_req.funct, cmd_req.xs1, cmd_req.xs2, &mut dma_iface)
      } else {
        0
      };

      let cmd_resp = CmdResp {
        header: MsgHeader {
          msg_type: MsgType::CmdResp as u32,
          reserved: 0,
        },
        result,
      };

      write_struct(&mut stream, &cmd_resp)?;
    }
  }
}

struct ClientDma<'a> {
  stream: &'a mut TcpStream,
}

impl<'a> DmaInterface for ClientDma<'a> {
  fn dma_read(&mut self, addr: u64, size: u32) -> Result<u128> {
    let req = DmaReadReq {
      header: MsgHeader {
        msg_type: MsgType::DmaReadReq as u32,
        reserved: 0,
      },
      size,
      padding: 0,
      addr,
    };

    write_struct(self.stream, &req)?;
    let resp = read_struct::<DmaReadResp>(self.stream)?;

    if resp.header.msg_type != MsgType::DmaReadResp as u32 {
      return Err(Error::new(ErrorKind::InvalidData, "Invalid DMA read response"));
    }

    let data = (resp.data_hi as u128) << 64 | (resp.data_lo as u128);
    Ok(data)
  }

  fn dma_write(&mut self, addr: u64, data: u128, size: u32) -> Result<()> {
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

    write_struct(self.stream, &req)?;
    let resp = read_struct::<DmaWriteResp>(self.stream)?;

    if resp.header.msg_type != MsgType::DmaWriteResp as u32 {
      return Err(Error::new(ErrorKind::InvalidData, "Invalid DMA write response"));
    }

    Ok(())
  }
}

