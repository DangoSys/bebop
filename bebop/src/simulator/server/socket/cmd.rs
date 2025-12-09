use super::protocol::*;
use std::io::Result;
use std::net::TcpStream;

#[derive(Debug)]
pub struct CmdHandler {
    stream: TcpStream,
}

impl Clone for CmdHandler {
    fn clone(&self) -> Self {
        Self {
            stream: self.stream.try_clone().expect("Failed to clone TcpStream"),
        }
    }
}

impl CmdHandler {
    pub fn new(stream: TcpStream) -> Self {
        Self { stream }
    }

    pub fn recv_request(&mut self) -> Result<CmdReq> {
        read_struct(&mut self.stream)
    }

    pub fn send_response(&mut self, result: u64) -> Result<()> {
        let resp = CmdResp {
            header: MsgHeader {
                msg_type: MsgType::CmdResp as u32,
                reserved: 0,
            },
            result,
        };
        write_struct(&mut self.stream, &resp)
    }
}
