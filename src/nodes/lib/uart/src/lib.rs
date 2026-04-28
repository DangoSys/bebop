/// Simple UART (serial port) emulation
///
/// This provides a minimal UART implementation for console I/O

use std::io::{self, Write};

/// UART register offsets (16550 compatible)
const UART_RBR: u64 = 0; // Receiver Buffer Register (read)
const UART_THR: u64 = 0; // Transmitter Holding Register (write)
const UART_IER: u64 = 1; // Interrupt Enable Register
const UART_IIR: u64 = 2; // Interrupt Identification Register (read)
const UART_FCR: u64 = 2; // FIFO Control Register (write)
const UART_LCR: u64 = 3; // Line Control Register
const UART_MCR: u64 = 4; // Modem Control Register
const UART_LSR: u64 = 5; // Line Status Register
const UART_MSR: u64 = 6; // Modem Status Register
const UART_SCR: u64 = 7; // Scratch Register

/// UART Line Status Register bits
const UART_LSR_THRE: u8 = 0x20; // Transmitter Holding Register Empty
const UART_LSR_TEMT: u8 = 0x40; // Transmitter Empty

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
        // Accept 1, 2, or 4 byte writes, but only use the lowest byte
        // This matches hardware behavior where UART registers are byte-wide
        // but can be accessed with wider stores
        if size != 1 && size != 2 && size != 4 {
            return false;
        }

        let byte = value as u8;

        match offset {
            UART_THR => {
                // Transmit character to stdout
                print!("{}", byte as char);
                io::stdout().flush().ok();
                true
            }
            UART_IER => {
                self.ier = byte;
                true
            }
            UART_FCR => {
                // FIFO Control Register - ignore for now
                true
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uart_status() {
        let uart = Uart::new();
        // LSR should indicate ready to transmit
        let lsr = uart.mmio_load(UART_LSR, 1).unwrap();
        assert_eq!(lsr, (UART_LSR_THRE | UART_LSR_TEMT) as u64);
    }
}
