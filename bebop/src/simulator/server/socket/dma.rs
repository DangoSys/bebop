use super::protocol::*;
use std::io::Result;
use std::net::TcpStream;

#[derive(Debug)]
pub struct DmaHandler {
    stream: TcpStream,
}

impl Clone for DmaHandler {
    fn clone(&self) -> Self {
        Self {
            stream: self.stream.try_clone().expect("Failed to clone TcpStream"),
        }
    }
}

impl DmaHandler {
    pub fn new(stream: TcpStream) -> Self {
        Self { stream }
    }

    /// Send DMA read request to client
    pub fn send_read_request(&mut self, addr: u64, size: u32) -> Result<()> {
        let req = DmaReadReq {
            header: MsgHeader {
                msg_type: MsgType::DmaReadReq as u32,
                reserved: 0,
            },
            size,
            padding: 0,
            addr,
        };
        write_struct(&mut self.stream, &req)
    }

    /// Receive DMA read response from client
    pub fn recv_read_response(&mut self) -> Result<u64> {
        let resp: DmaReadResp = read_struct(&mut self.stream)?;
        Ok(resp.data)
    }

    /// Send DMA write request to client
    pub fn send_write_request(&mut self, addr: u64, data: u64, size: u32) -> Result<()> {
        let req = DmaWriteReq {
            header: MsgHeader {
                msg_type: MsgType::DmaWriteReq as u32,
                reserved: 0,
            },
            size,
            padding: 0,
            addr,
            data,
        };
        write_struct(&mut self.stream, &req)
    }

    /// Receive DMA write response from client
    pub fn recv_write_response(&mut self) -> Result<()> {
        let _resp: DmaWriteResp = read_struct(&mut self.stream)?;
        Ok(())
    }

    /// Perform DMA read (send request + receive response)
    pub fn read(&mut self, addr: u64, size: u32) -> Result<u64> {
        self.send_read_request(addr, size)?;
        self.recv_read_response()
    }

    /// Perform DMA write (send request + receive response)
    pub fn write(&mut self, addr: u64, data: u64, size: u32) -> Result<()> {
        self.send_write_request(addr, data, size)?;
        self.recv_write_response()
    }
}
