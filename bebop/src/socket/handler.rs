/// TCP connection handler for Spike communication
use std::io::{Read, Write};
use std::net::TcpStream;

use super::protocol::SocketMsg;
use crate::simulator::Simulator;

pub struct ConnectionHandler {
    stream: TcpStream,
    simulator: Simulator,
}

impl ConnectionHandler {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            simulator: Simulator::new(),
        }
    }

    /// Handle the client connection loop
    pub fn handle(mut self) -> std::io::Result<()> {
        let peer_addr = self.stream.peer_addr()?;
        println!("New connection from: {}", peer_addr);

        loop {
            // Read message
            let mut msg_bytes = [0u8; SocketMsg::SIZE];
            match self.stream.read_exact(&mut msg_bytes) {
                Ok(_) => {}
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        println!("Client {} disconnected", peer_addr);
                        return Ok(());
                    }
                    return Err(e);
                }
            }

            // Parse message
            let msg = SocketMsg::from_bytes(&msg_bytes);
            
            // Copy fields to avoid packed struct alignment issues
            let funct = msg.funct;
            let xs1 = msg.xs1;
            let xs2 = msg.xs2;
            
            println!(
                "Received: funct={}, xs1=0x{:016x}, xs2=0x{:016x}",
                funct, xs1, xs2
            );

            // Process instruction
            let resp = self.simulator.process(&msg);
            let result = resp.result;

            // Send response
            let resp_bytes = resp.to_bytes();
            self.stream.write_all(&resp_bytes)?;
            println!("Sent: result=0x{:016x}", result);
        }
    }
}

