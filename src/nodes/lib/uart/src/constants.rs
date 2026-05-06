/// UART register offsets (16550 compatible)
pub const UART_RBR: u64 = 0; // Receiver Buffer Register (read)
pub const UART_THR: u64 = 0; // Transmitter Holding Register (write)
pub const UART_IER: u64 = 1; // Interrupt Enable Register
pub const UART_IIR: u64 = 2; // Interrupt Identification Register (read)
pub const UART_FCR: u64 = 2; // FIFO Control Register (write)
pub const UART_LCR: u64 = 3; // Line Control Register
pub const UART_MCR: u64 = 4; // Modem Control Register
pub const UART_LSR: u64 = 5; // Line Status Register
pub const UART_MSR: u64 = 6; // Modem Status Register
pub const UART_SCR: u64 = 7; // Scratch Register

/// UART Line Status Register bits
pub const UART_LSR_THRE: u8 = 0x20; // Transmitter Holding Register Empty
pub const UART_LSR_TEMT: u8 = 0x40; // Transmitter Empty
