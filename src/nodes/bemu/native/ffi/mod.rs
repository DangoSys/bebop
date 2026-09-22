use bebop_bank_hash::{combine_bank_hash, BTraceBank, INVALID_VBANK};
use bebop_bemu_profile::{BemuProfile, BemuProfileReport};
use bebop_clint::Clint;
use bebop_dtb::DtbBuilder;
use bebop_elf::{load_elf, LoadInfo, TlsInfo};
use bebop_plic::Plic;
use bebop_rushb::{FUNCT7_MSET, FUNCT7_MVIN, FUNCT7_MVIN_MMIO, FUNCT7_MVOUT};
use bebop_syscall::{add_guest_mapping, handle_syscall_with_state, set_guest_mappings, SyscallState};
use bebop_uart::Uart;
use std::collections::HashMap;
use std::os::raw::{c_char, c_void};
use std::path::Path;
use std::slice;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::bank::{
    bank_num, bank_row_bytes, bank_size, mmio_bank_num, mmio_bank_size, mmio_total_size, virtual_bank_num, BankConfig,
    BankMap, MATRIX_SIZE,
};
use crate::inst;
use crate::trace::{with_trace_ptr, TraceConfig, TraceState};

const DRAM_BASE: u64 = 0x80000000;
// UART base address (matches test workloads)
const UART_BASE: u64 = 0x60020000;
const PAGE_SIZE: u64 = 4096;
const USER_TOP: u64 = 0x40_0000_0000;
// musl __mmap64 treats syscall results with a0 > 0xfffff000 as errors.
const PK_MMAP_CEILING: u64 = 0x8000_0000;
const USER_STACK_SIZE: u64 = 8 * 1024 * 1024;
const PK_PT_RESERVE: u64 = 16 * 1024 * 1024;
const PK_HIGH_RESERVE: u64 = 64 * 1024 * 1024;
const SYS_BRK: u64 = 214;
const SYS_MUNMAP: u64 = 215;
const SYS_MMAP: u64 = 222;

mod callbacks;
mod pk;
mod rushb;
mod spike;
mod state;

extern "C" {
    fn spike_mmu_load_u8(addr: u64, value: *mut u8) -> bool;
    fn spike_mmu_store_u8(addr: u64, value: u8) -> bool;
}

pub(crate) fn dma_read(addr: u64) -> u8 {
    let mut value = 0;
    assert!(
        unsafe { spike_mmu_load_u8(addr, &mut value) },
        "BEMU DMA load failed at 0x{addr:x}"
    );
    value
}

pub(crate) fn dma_write(addr: u64, value: u8) {
    assert!(
        unsafe { spike_mmu_store_u8(addr, value) },
        "BEMU DMA store failed at 0x{addr:x}"
    );
}

pub use spike::{create_spike, NativeSpike};
pub use state::SharedMemory;
