// ELF magic numbers
pub const EI_MAG0: usize = 0;
pub const EI_MAG1: usize = 1;
pub const EI_MAG2: usize = 2;
pub const EI_MAG3: usize = 3;
pub const ELFMAG0: u8 = 0x7f;
pub const ELFMAG1: u8 = b'E';
pub const ELFMAG2: u8 = b'L';
pub const ELFMAG3: u8 = b'F';

// Program header types
pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_TLS: u32 = 7;
pub const PF_X: u32 = 0x1;

// Dynamic section tags
pub const DT_RELA: i64 = 7;
pub const DT_RELASZ: i64 = 8;
pub const DT_RELAENT: i64 = 9;

// Section header types
pub const SHT_SYMTAB: u32 = 2;
pub const SHT_RELA: u32 = 4;

// Relocation types
pub const R_RISCV_IRELATIVE: u32 = 58;
