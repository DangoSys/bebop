use crate::constants::*;
use std::io::{self, Write};

pub struct Uart {
    /// Interrupt Enable Register
    ier: u8,
    /// Line Control Register
    lcr: u8,
    /// Modem Control Register
    mcr: u8,
    /// Scratch Register
    scr: u8,
}

impl Uart {
    pub fn new() -> Self {
        Self {
            ier: 0,
            lcr: 0,
            mcr: 0,
            scr: 0,
        }
    }

    /// Handle MMIO load from UART
    pub fn mmio_load(&self, offset: u64, size: usize) -> Option<u64> {
        if size != 1 {
            return None;
        }

        let value = match offset {
            UART_IER => self.ier,
            UART_IIR => 0x01, // No interrupt pending
            UART_LCR => self.lcr,
            UART_MCR => self.mcr,
            UART_LSR => UART_LSR_THRE | UART_LSR_TEMT, // Always ready to transmit
            UART_MSR => 0x00,
            UART_SCR => self.scr,
            _ => 0,
        };

        Some(value as u64)
    }

    /// Handle MMIO store to UART
    pub fn mmio_store(&mut self, offset: u64, size: usize, value: u64) -> bool {
        if size != 1 && size != 2 && size != 4 {
            return false;
        }

        let byte = value as u8;

        match offset {
            UART_THR => {
                print!("{}", byte as char);
                io::stdout().flush().ok();
                true
            }
            UART_IER => {
                self.ier = byte;
                true
            }
            UART_FCR => true,
            UART_LCR => {
                self.lcr = byte;
                true
            }
            UART_MCR => {
                self.mcr = byte;
                true
            }
            UART_SCR => {
                self.scr = byte;
                true
            }
            _ => false,
        }
    }
}

impl Default for Uart {
    fn default() -> Self {
        Self::new()
    }
}
