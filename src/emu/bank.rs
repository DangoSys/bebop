pub const BANK_NUM: usize = 32;
pub const BANK_WIDTH: usize = 128;
pub const BANK_LINES: usize = 1024;
pub const BANK_SIZE: usize = BANK_LINES * (BANK_WIDTH / 8);
pub const MATRIX_SIZE: usize = 16;

#[derive(Default, Clone, Copy, Debug)]
pub struct BankConfig {
    pub allocated: bool,
    pub cols: u64,
}

#[inline]
pub fn mem_read(mem: &[u8], addr: u64) -> u8 {
    mem[(addr as usize) % mem.len()]
}

#[inline]
pub fn mem_write(mem: &mut [u8], addr: u64, v: u8) {
    mem[(addr as usize) % mem.len()] = v;
}
