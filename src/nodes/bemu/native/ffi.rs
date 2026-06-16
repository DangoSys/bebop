use bebop_bank_hash::bank_hash;
use bebop_dtb::DtbBuilder;
use bebop_elf::load_elf;
use bebop_syscall::{get_exit_code, handle_syscall, init_mem_layout, reset_syscall_state};
use bebop_uart::Uart;
use once_cell::sync::Lazy;
use std::collections::BTreeSet;
use std::os::raw::c_char;
use std::sync::Mutex;

use crate::bank::{BankConfig, BankMap, BANK_NUM, BANK_SIZE};
use crate::inst;

const DRAM_BASE: u64 = 0x80000000;
const UART_BASE: u64 = 0x60020000; // UART base address (matches test workloads)

static EMU_STATE: Lazy<Mutex<EmuState>> = Lazy::new(|| Mutex::new(EmuState::new()));

struct EmuState {
    memory: Vec<u8>,
    banks: Vec<Vec<u8>>,
    bank_cfgs: Vec<BankConfig>,
    bank_map: BankMap,
    mmio_banks: [[u8; 1024]; 16],
    mmio_region_table: [crate::inst::instruction::MmioRegion; 32],
    total_lat: u64,
    npu_instruction_id: u64,
    uart: Uart,
}

impl EmuState {
    fn new() -> Self {
        // 1GB Here is important, for baremetal mode, when we set this to 4GB,
        // it will running for a long time.
        const MEM_SIZE: usize = 1 << 30;
        Self {
            memory: vec![0; MEM_SIZE],
            banks: vec![vec![0; BANK_SIZE]; BANK_NUM],
            bank_cfgs: vec![BankConfig::default(); BANK_NUM],
            bank_map: BankMap::new(BANK_NUM),
            mmio_banks: [[0u8; 1024]; 16],
            mmio_region_table: [crate::inst::instruction::MmioRegion::default(); 32],
            total_lat: 0,
            npu_instruction_id: 0,
            uart: Uart::new(),
        }
    }

    fn reset_accel(&mut self) {
        for b in &mut self.banks {
            b.fill(0);
        }
        self.bank_cfgs.fill(BankConfig::default());
        self.bank_map = BankMap::new(BANK_NUM);
        for bank in &mut self.mmio_banks {
            bank.fill(0);
        }
        self.mmio_region_table = [crate::inst::instruction::MmioRegion::default(); 32];
        self.total_lat = 0;
        self.npu_instruction_id = 0;
    }
}

fn add_resolved_bank(out: &mut BTreeSet<usize>, bank_map: &BankMap, vbank: u64, group: u64) {
    if vbank < BANK_NUM as u64 {
        if let Some(pbank) = bank_map.resolve_group(vbank as u32, group as u32) {
            out.insert(pbank);
        }
    }
}

fn add_vbank_group0(out: &mut BTreeSet<usize>, bank_map: &BankMap, vbank: u64) {
    add_resolved_bank(out, bank_map, vbank, 0);
}

fn add_vbank_groups(out: &mut BTreeSet<usize>, cfgs: &[BankConfig], bank_map: &BankMap, vbank: u64) {
    if vbank >= cfgs.len() as u64 {
        return;
    }

    let groups = cfgs[vbank as usize].cols.max(1).min(BANK_NUM as u64);
    for group in 0..groups {
        add_resolved_bank(out, bank_map, vbank, group);
    }
}

fn candidate_affected_banks(funct: u32, xs1: u64, cfgs: &[BankConfig], bank_map: &BankMap) -> BTreeSet<usize> {
    let mut out = BTreeSet::new();
    let b0 = inst::decode::rs1_b0(xs1);
    let b2 = inst::decode::rs1_b2(xs1);

    match funct {
        // mset/mvin write the target virtual bank. mset is included here even
        // when the bank is zero-filled to the same bytes, because allocation is
        // still a bank-affecting operation for hash trace consumers.
        32 | 33 => add_vbank_groups(&mut out, cfgs, bank_map, b0),
        // Single-bank transforms in the current BEMU implementation write only
        // the resolved group-0 physical slot.
        48 | 49 | 50 | 51 | 55 => add_vbank_group0(&mut out, bank_map, b2),
        52 => {
            let src_cols = cfgs.get(b0 as usize).map(|cfg| cfg.cols).unwrap_or(0);
            let dst_cols = cfgs.get(b2 as usize).map(|cfg| cfg.cols).unwrap_or(0);
            if src_cols == 4 && dst_cols == 4 {
                add_vbank_groups(&mut out, cfgs, bank_map, b2);
            } else {
                add_vbank_group0(&mut out, bank_map, b2);
            }
        }
        // Matrix compute paths write all physical groups bound to the output
        // accumulator bank.
        64 | 65 | 66 | 67 => add_vbank_groups(&mut out, cfgs, bank_map, b2),
        _ => {}
    }

    out
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
pub extern "C" fn buckyball_exec(funct7: u8, xs1: u64, xs2: u64, pc: u64) -> u64 {
    let mut state = EMU_STATE.lock().unwrap();
    let lat = inst::decode::cycles_after_issue(funct7 as u32, xs1, xs2);
    state.total_lat += lat;
    crate::trace::set_bemu_clk(state.total_lat);
    state.npu_instruction_id = state.npu_instruction_id.wrapping_add(1);
    let instruction_id = state.npu_instruction_id;

    crate::trace::itrace(crate::trace::ITraceEvent {
        funct: funct7 as u32,
        pc,
        rs1: xs1,
        rs2: xs2,
    });

    let EmuState {
        memory,
        banks,
        bank_cfgs,
        bank_map,
        mmio_banks,
        mmio_region_table,
        uart: _,
        ..
    } = &mut *state;

    let mut affected_banks = candidate_affected_banks(funct7 as u32, xs1, bank_cfgs, bank_map);
    let before_hashes: Vec<u64> = banks.iter().map(|bank| bank_hash(bank)).collect();

    let result = {
        let mut ctx = inst::instruction::ExecContext {
            memory,
            banks,
            cfgs: bank_cfgs,
            bank_map,
            mmio_banks,
            mmio_region_table,
        };

        inst::decode::execute_known(funct7 as u32, xs1, xs2, &mut ctx)
            .unwrap_or_else(|| panic!("unknown funct7: {}", funct7))
    };

    affected_banks.extend(candidate_affected_banks(funct7 as u32, xs1, bank_cfgs, bank_map));
    let after_hashes: Vec<u64> = banks
        .iter()
        .enumerate()
        .map(|(bank_id, bank)| {
            let hash = bank_hash(bank);
            if before_hashes[bank_id] != hash {
                affected_banks.insert(bank_id);
            }
            hash
        })
        .collect();

    let op_type = format!("funct7_{}", funct7);
    for bank_id in affected_banks {
        crate::trace::bemu_bank_hash(
            instruction_id,
            bank_id as u32,
            funct7 as u32,
            &op_type,
            after_hashes[bank_id],
            pc,
        );
    }

    result
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
        pk: bool,
    ) -> i32;
}

pub fn run_spike(
    isa: &str,
    procs: usize,
    mem_mb: usize,
    elf_path: &str,
    log_path: Option<&str>,
    pk: bool,
) -> Result<(), String> {
    use std::ffi::CString;

    let mut state = EMU_STATE.lock().unwrap();

    // Load ELF into memory (Rust implementation)
    let load = load_elf(elf_path, &mut state.memory, DRAM_BASE)?;
    let entry = load.entry;

    const PAGE_SIZE: u64 = 4096;
    let mem_end = DRAM_BASE + state.memory.len() as u64;
    let brk_start = (load.image_end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let mmap_base = (mem_end - 8 * 1024 * 1024) & !(PAGE_SIZE - 1);
    reset_syscall_state();
    init_mem_layout(brk_start, mmap_base);

    // Initialize TLS if present
    let tp_value = if let Some(tls) = load.tls {
        let align = tls.align.max(16);
        let tls_size = (tls.memsz + align - 1) & !(align - 1);
        let total_size = tls_size + align;

        // Allocate TLS area at high memory
        let tls_area_addr = DRAM_BASE + state.memory.len() as u64 - total_size - 0x10000;
        let tp = (tls_area_addr + align - 1) & !(align - 1);
        let tls_data_start = tp;

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

        // RISC-V local-exec TLS addresses are positive offsets from tp.
        let tcb_offset = (tp - DRAM_BASE) as usize;
        if tcb_offset + 8 <= state.memory.len() {
            state.memory[tcb_offset..tcb_offset + 8].copy_from_slice(&tp.to_le_bytes());
        }

        Some(tp)
    } else {
        None
    };

    // Generate DTB and write to memory
    let dtb = DtbBuilder::build_minimal(DRAM_BASE, mem_mb as u64 * (1 << 20), None, None);
    let dtb_addr = (mem_end - 0x20_0000 - dtb.len() as u64) & !(PAGE_SIZE - 1);
    let dtb_offset = (dtb_addr - DRAM_BASE) as usize;
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
            dtb_addr,
            log_c.as_ref().map(|c| c.as_ptr()).unwrap_or(std::ptr::null()),
            tp_ptr,
            uart_ptr,
            pk,
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

#[cfg(test)]
mod tests {
    use bebop_bank_hash::bank_hash;

    #[test]
    fn bank_hash_changes_when_bemu_bank_byte_changes() {
        let mut bank = vec![0u8; crate::bank::BANK_SIZE];
        let before = bank_hash(&bank);

        bank[0] ^= 0x01;

        assert_ne!(before, bank_hash(&bank));
    }
}
