/// The DDR every Buckyball system has, from 0x8000_0000.
pub const DDR_SIZE: u64 = 16 << 30;
mod memory;

pub use memory::Memory;
