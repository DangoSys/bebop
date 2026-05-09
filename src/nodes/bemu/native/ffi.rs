use bebop_dtb::DtbBuilder;
use bebop_elf::load_elf;
use bebop_syscall::{get_exit_code, handle_syscall};
use bebop_uart::Uart;
use once_cell::sync::Lazy;
use std::os::raw::c_char;
use std::sync::Mutex;

use crate::bank::{BankConfig, BankMap, BANK_NUM, BANK_SIZE};
use crate::inst;

const DRAM_BASE: u64 = 0x80000000;
const DTB_ADDR: u64 = 0x80000000 + (1 << 20); // 1MB after DRAM_BASE
const UART_BASE: u64 = 0x60020000; // UART base address (matches test workloads)

static EMU_STATE: Lazy<Mutex<EmuState>> = Lazy::new(|| Mutex::new(EmuState::new()));

struct EmuState {
    memory: Vec<u8>,
    banks: Vec<Vec<u8>>,
    bank_cfgs: Vec<BankConfig>,
    bank_map: BankMap,
    total_lat: u64,
    uart: Uart,
}

impl EmuState {
    fn new() -> Self {
        const MEM_SIZE: usize = 1 << 30; // 1GB
        Self {
            memory: vec![0; MEM_SIZE],
            banks: vec![vec![0; BANK_SIZE]; BANK_NUM],
            bank_cfgs: vec![BankConfig::default(); BANK_NUM],
            bank_map: BankMap::new(BANK_NUM),
            total_lat: 0,
            uart: Uart::new(),
        }
    }

    fn reset_accel(&mut self) {
        for b in &mut self.banks {
            b.fill(0);
        }
        self.bank_cfgs.fill(BankConfig::default());
        self.bank_map = BankMap::new(BANK_NUM);
        self.total_lat = 0;
    }
}

#[no_mangle]
pub extern "C" fn buckyball_init() {
    let _guard = EMU_STATE.lock().unwrap();
}

#[no_mangle]
pub extern "C" fn buckyball_reset() {
    let mut state = EMU_STATE.lock().unwrap();
    state.reset_accel();
}

#[no_mangle]
pub extern "C" fn buckyball_exec(funct7: u8, xs1: u64, xs2: u64) -> u64 {
    let mut state = EMU_STATE.lock().unwrap();
    let lat = inst::exec_latency::cycles_after_issue(funct7 as u32, xs1, xs2);
    state.total_lat += lat;
    let EmuState {
        memory,
        banks,
        bank_cfgs,
        bank_map,
        uart: _,
        ..
    } = &mut *state;

    inst::decode::execute_known(funct7 as u32, xs1, xs2, memory, banks, bank_cfgs, bank_map)
        .unwrap_or_else(|| panic!("unknown funct7: {}", funct7))
}

/// Handle system call from guest program
/// Returns (result, should_exit)
#[no_mangle]
pub extern "C" fn handle_syscall_ffi(syscall_num: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64 {
    let mut state = EMU_STATE.lock().unwrap();
    let (result, _should_exit) = handle_syscall(syscall_num, a0, a1, a2, a3, a4, a5, &mut state.memory);
    result
}

/// Check if program should exit
#[no_mangle]
pub extern "C" fn should_exit() -> bool {
    get_exit_code().is_some()
}

/// Get exit code
#[no_mangle]
pub extern "C" fn get_exit_code_ffi() -> i32 {
    get_exit_code().unwrap_or(0)
}

/// Handle UART MMIO load
/// IMPORTANT: uart_ptr is passed from spike_run_raw to avoid deadlock
#[no_mangle]
pub extern "C" fn uart_mmio_load(uart_ptr: *mut u8, addr: u64, size: usize) -> u64 {
    let uart = unsafe { &mut *(uart_ptr as *mut Uart) };
    let offset = addr - UART_BASE;
    uart.mmio_load(offset, size).unwrap_or(0)
}

/// Handle UART MMIO store
/// IMPORTANT: uart_ptr is passed from spike_run_raw to avoid deadlock
#[no_mangle]
pub extern "C" fn uart_mmio_store(uart_ptr: *mut u8, addr: u64, size: usize, value: u64) -> bool {
    let uart = unsafe { &mut *(uart_ptr as *mut Uart) };
    let offset = addr - UART_BASE;
    uart.mmio_store(offset, size, value)
}

extern "C" {
    fn spike_run_raw(
        isa: *const c_char,
        procs: usize,
        mem_ptr: *mut u8,
        mem_size: usize,
        entry: u64,
        dtb_addr: u64,
        log_path: *const c_char,
        tp_value: *const u64,
        uart_ptr: *mut u8,
    ) -> i32;
}

pub fn run_spike(isa: &str, procs: usize, mem_mb: usize, elf_path: &str, log_path: Option<&str>) -> Result<(), String> {
    use std::ffi::CString;

    let mut state = EMU_STATE.lock().unwrap();

    // Load ELF into memory (Rust implementation)
    let (entry, tls_info) = load_elf(elf_path, &mut state.memory, DRAM_BASE)?;

    // Initialize TLS if present
    let tp_value = if let Some(tls) = tls_info {
        // Calculate TLS size with alignment
        // RISC-V TLS variant I: [TLS data] [TCB]
        const TCB_SIZE: u64 = 16; // Minimal TCB (just self-pointer)
        let align = tls.align.max(16);
        let tls_size = (tls.memsz + align - 1) & !(align - 1);
        let total_size = tls_size + TCB_SIZE + align;

        // Allocate TLS area at high memory
        let tls_area_addr = DRAM_BASE + state.memory.len() as u64 - total_size - 0x10000;
        let tls_data_start = tls_area_addr;
        let tcb_start = tls_data_start + tls_size;
        let tp = tcb_start + TCB_SIZE;

        // Copy TLS initialization data
        if tls.vaddr >= DRAM_BASE && tls.vaddr < DRAM_BASE + state.memory.len() as u64 {
            let src_offset = (tls.vaddr - DRAM_BASE) as usize;
            let dst_offset = (tls_data_start - DRAM_BASE) as usize;
            let copy_size = tls.filesz.min(tls.memsz) as usize;

            if src_offset + copy_size <= state.memory.len() && dst_offset + copy_size <= state.memory.len() {
                // Copy from loaded ELF data to TLS area
                let src = state.memory[src_offset..src_offset + copy_size].to_vec();
                state.memory[dst_offset..dst_offset + copy_size].copy_from_slice(&src);

                // Zero out BSS part
                if tls.memsz > tls.filesz {
                    let bss_start = dst_offset + copy_size;
                    let bss_size = (tls.memsz - tls.filesz) as usize;
                    if bss_start + bss_size <= state.memory.len() {
                        state.memory[bss_start..bss_start + bss_size].fill(0);
                    }
                }
            }
        }

        // Initialize TCB (self-pointer)
        let tcb_offset = (tcb_start - DRAM_BASE) as usize;
        if tcb_offset + 8 <= state.memory.len() {
            state.memory[tcb_offset..tcb_offset + 8].copy_from_slice(&tcb_start.to_le_bytes());
        }

        Some(tp)
    } else {
        None
    };

    // Generate DTB and write to memory
    let dtb = DtbBuilder::build_minimal(DRAM_BASE, mem_mb as u64 * (1 << 20), None, None);
    let dtb_offset = (DTB_ADDR - DRAM_BASE) as usize;
    if dtb_offset + dtb.len() > state.memory.len() {
        return Err("DTB too large for memory".to_string());
    }
    state.memory[dtb_offset..dtb_offset + dtb.len()].copy_from_slice(&dtb);

    let isa_c = CString::new(isa).map_err(|e| e.to_string())?;
    let log_c = log_path
        .map(|s| CString::new(s).map_err(|e| e.to_string()))
        .transpose()?;

    let tp_ptr = tp_value.as_ref().map(|v| v as *const u64).unwrap_or(std::ptr::null());

    // IMPORTANT: Drop the lock before calling spike_run_raw to avoid deadlock
    // Spike will call back into Rust FFI functions (uart_mmio_store, etc.)
    // which need to acquire the lock again
    let mem_ptr = state.memory.as_mut_ptr();
    let mem_size = state.memory.len();
    let uart_ptr = &mut state.uart as *mut Uart as *mut u8;
    drop(state); // Explicitly drop the lock

    let ret = unsafe {
        spike_run_raw(
            isa_c.as_ptr(),
            procs,
            mem_ptr,
            mem_size,
            entry,
            DTB_ADDR,
            log_c.as_ref().map(|c| c.as_ptr()).unwrap_or(std::ptr::null()),
            tp_ptr,
            uart_ptr,
        )
    };
    let total_lat = {
        let state = EMU_STATE.lock().unwrap();
        state.total_lat
    };
    eprintln!("[INFO] total latency: {}", total_lat);

    if ret == 0 {
        Ok(())
    } else {
        Err(format!("spike exited with code {}", ret))
    }
}
