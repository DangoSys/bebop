use bebop_bank_hash::bank_hash;
use bebop_dtb::DtbBuilder;
use bebop_elf::{load_elf, LoadInfo, TlsInfo};
use bebop_syscall::{handle_syscall_with_state, SyscallState};
use bebop_uart::Uart;
use std::os::raw::{c_char, c_void};
use std::path::Path;

use crate::bank::{BankConfig, BankMap, BANK_NUM, BANK_SIZE};
use crate::inst;
use crate::trace::{with_trace_ptr, TraceConfig, TraceState};

const DRAM_BASE: u64 = 0x80000000;
// UART base address (matches test workloads)
const UART_BASE: u64 = 0x60020000;
const PAGE_SIZE: u64 = 4096;

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
    syscall: SyscallState,
    trace: TraceState,
}

impl EmuState {
    fn new(log_dir: &Path, trace_config: TraceConfig) -> Result<Self, String> {
        // 1GB Here is important, for baremetal mode, when we set this to 4GB,
        // it will running for a long time.
        const MEM_SIZE: usize = 1 << 30;
        Ok(Self {
            // memory is maintained by bemu not spike
            memory: vec![0; MEM_SIZE],
            banks: vec![vec![0; BANK_SIZE]; BANK_NUM],
            bank_cfgs: vec![BankConfig::default(); BANK_NUM],
            bank_map: BankMap::new(BANK_NUM),
            mmio_banks: [[0u8; 1024]; 16],
            mmio_region_table: [crate::inst::instruction::MmioRegion::default(); 32],
            total_lat: 0,
            npu_instruction_id: 0,
            uart: Uart::new(),
            syscall: SyscallState::new(),
            trace: TraceState::new(log_dir, trace_config).map_err(|e| e.to_string())?,
        })
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

unsafe fn state_mut<'a>(state: *mut c_void) -> &'a mut EmuState {
    assert!(!state.is_null(), "null BEMU state pointer");
    &mut *(state as *mut EmuState)
}

#[no_mangle]
pub extern "C" fn buckyball_init(_state: *mut c_void) {}

#[no_mangle]
pub extern "C" fn buckyball_reset(state: *mut c_void) {
    unsafe { state_mut(state) }.reset_accel();
}

#[no_mangle]
pub extern "C" fn buckyball_exec(state: *mut c_void, funct7: u8, xs1: u64, xs2: u64, pc: u64) -> u64 {
    let state = unsafe { state_mut(state) };
    let lat = inst::decode::cycles_after_issue(funct7 as u32, xs1, xs2);
    state.total_lat += lat;
    state.trace.set_bemu_clk(state.total_lat);
    state.npu_instruction_id = state.npu_instruction_id.wrapping_add(1);
    let instruction_id = state.npu_instruction_id;
    let trace = &mut state.trace as *mut TraceState;

    unsafe {
        with_trace_ptr(trace, || {
            crate::trace::itrace(crate::trace::ITraceEvent {
                funct: funct7 as u32,
                pc,
                rs1: xs1,
                rs2: xs2,
            });
        })
    };

    let EmuState {
        memory,
        banks,
        bank_cfgs,
        bank_map,
        mmio_banks,
        mmio_region_table,
        uart: _,
        syscall: _,
        trace: _,
        ..
    } = state;

    let before_hashes: Vec<u64> = banks.iter().map(|bank| bank_hash(bank)).collect();

    let result = unsafe {
        with_trace_ptr(trace, || {
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
        })
    };

    let op_type = format!("funct7_{}", funct7);
    unsafe {
        with_trace_ptr(trace, || {
            for (bank_id, bank) in banks.iter().enumerate() {
                let hash = bank_hash(bank);
                if before_hashes[bank_id] != hash {
                    crate::trace::bemu_bank_hash(instruction_id, bank_id as u32, funct7 as u32, &op_type, hash, pc);
                }
            }
        })
    };

    result
}

/// Handle system call from guest program
/// Returns (result, should_exit)
#[no_mangle]
pub extern "C" fn handle_syscall_ffi(
    state: *mut c_void,
    syscall_num: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
) -> u64 {
    let state = unsafe { state_mut(state) };
    let (result, _should_exit) = handle_syscall_with_state(
        &mut state.syscall,
        syscall_num,
        a0,
        a1,
        a2,
        a3,
        a4,
        a5,
        &mut state.memory,
    );
    result
}

/// Check if program should exit
#[no_mangle]
pub extern "C" fn should_exit_ffi(state: *mut c_void) -> bool {
    unsafe { state_mut(state) }.syscall.exit_code.is_some()
}

/// Get exit code
#[no_mangle]
pub extern "C" fn get_exit_code_ffi(state: *mut c_void) -> i32 {
    unsafe { state_mut(state) }.syscall.exit_code.unwrap_or(0)
}

/// Handle UART MMIO load
/// IMPORTANT: uart_ptr is passed from spike_create_raw to avoid deadlock
#[no_mangle]
pub extern "C" fn uart_mmio_load(uart_ptr: *mut u8, addr: u64, size: usize) -> u64 {
    let uart = unsafe { &mut *(uart_ptr as *mut Uart) };
    let offset = addr - UART_BASE;
    uart.mmio_load(offset, size).unwrap_or(0)
}

/// Handle UART MMIO store
/// IMPORTANT: uart_ptr is passed from spike_create_raw to avoid deadlock
#[no_mangle]
pub extern "C" fn uart_mmio_store(uart_ptr: *mut u8, addr: u64, size: usize, value: u64) -> bool {
    let uart = unsafe { &mut *(uart_ptr as *mut Uart) };
    let offset = addr - UART_BASE;
    uart.mmio_store(offset, size, value)
}

extern "C" {
    fn spike_create_raw(
        isa: *const c_char,
        procs: usize,
        mem_ptr: *mut u8,
        mem_size: usize,
        log_path: *const c_char,
        uart_ptr: *mut u8,
        emu_state: *mut c_void,
    ) -> *mut c_void;
    fn spike_init_hart_raw(
        ctx: *mut c_void,
        entry: u64,
        trap_handler_addr: u64,
        initial_sp: u64,
        initial_a0: u64,
        initial_a1: u64,
        initial_a2: u64,
        tp_value: *const u64,
        pk: bool,
    ) -> bool;
    fn spike_step_raw(ctx: *mut c_void) -> i32;
    fn spike_finished_raw(ctx: *mut c_void) -> bool;
    fn spike_exit_code_raw(ctx: *mut c_void) -> i32;
    fn spike_destroy_raw(ctx: *mut c_void);

}

pub struct NativeSpike {
    ctx: *mut c_void,
    state: Box<EmuState>,
    loaded_elf: Option<LoadInfo>,
}

unsafe impl Send for NativeSpike {}

impl NativeSpike {
    pub fn load_elf(&mut self, elf_path: &str) -> Result<(), String> {
        self.loaded_elf = Some(load_elf_memory(&mut self.state, elf_path)?);
        Ok(())
    }

    pub fn init_hart(&mut self, mem_mb: usize, pk: bool) -> Result<(), String> {
        let load = self
            .loaded_elf
            .take()
            .ok_or_else(|| "cannot initialize hart before loading ELF".to_string())?;
        hart_init(self.ctx, &mut self.state, load, mem_mb, pk)
    }

    pub fn step(&mut self) -> Result<(), String> {
        let ret = unsafe { spike_step_raw(self.ctx) };
        if ret < 0 {
            Err(format!("spike step failed with code {}", self.exit_code()))
        } else {
            Ok(())
        }
    }

    pub fn finished(&self) -> bool {
        unsafe { spike_finished_raw(self.ctx) }
    }

    pub fn exit_code(&self) -> i32 {
        unsafe { spike_exit_code_raw(self.ctx) }
    }

    pub fn total_latency(&self) -> u64 {
        self.state.total_lat
    }
}

impl Drop for NativeSpike {
    fn drop(&mut self) {
        unsafe { spike_destroy_raw(self.ctx) };
    }
}

pub fn create_spike(
    isa: &str,
    procs: usize,
    log_path: &str,
    log_dir: &Path,
    trace_config: TraceConfig,
) -> Result<NativeSpike, String> {
    use std::ffi::CString;

    std::fs::create_dir_all(log_dir)
        .map_err(|e| format!("failed to create BEMU log dir {}: {e}", log_dir.display()))?;
    if log_path.is_empty() {
        return Err("Spike log path is empty".to_string());
    }

    let isa_c = CString::new(isa).map_err(|e| e.to_string())?;
    let log_c = CString::new(log_path).map_err(|e| e.to_string())?;
    let mut state = Box::new(EmuState::new(log_dir, trace_config)?);
    let mem_ptr = state.memory.as_mut_ptr();
    let mem_size = state.memory.len();
    let uart_ptr = &mut state.uart as *mut Uart as *mut u8;
    let state_ptr = &mut *state as *mut EmuState as *mut c_void;

    let ctx = unsafe {
        spike_create_raw(
            isa_c.as_ptr(),
            procs,
            mem_ptr,
            mem_size,
            log_c.as_ptr(),
            uart_ptr,
            state_ptr,
        )
    };
    if ctx.is_null() {
        Err("failed to create spike instance".to_string())
    } else {
        Ok(NativeSpike {
            ctx,
            state,
            loaded_elf: None,
        })
    }
}

struct HartInit {
    entry: u64,
    trap_handler_addr: u64,
    regs: InitialRegs,
    tp: Option<u64>,
    pk: bool,
}

fn load_elf_memory(state: &mut EmuState, elf_path: &str) -> Result<LoadInfo, String> {
    let load = load_elf(elf_path, &mut state.memory, DRAM_BASE)?;
    let entry = load.entry;
    let mem_end = DRAM_BASE + state.memory.len() as u64;

    if entry < DRAM_BASE || entry >= mem_end {
        return Err(format!(
            "ELF entry outside BEMU DRAM: original=0x{:x} entry=0x{:x} valid=0x{:x}..0x{:x}",
            load.analysis.original_entry, entry, DRAM_BASE, mem_end
        ));
    }

    if load.analysis.needs_relocation {
        eprintln!(
            "[INFO] relocated ELF: entry 0x{:x} -> 0x{:x}, image 0x{:x}..0x{:x} -> end 0x{:x}",
            load.analysis.original_entry,
            load.analysis.entry,
            load.analysis.min_vaddr,
            load.analysis.max_vaddr,
            load.analysis.image_end
        );
    }

    Ok(load)
}

fn hart_init(ctx: *mut c_void, state: &mut EmuState, load: LoadInfo, mem_mb: usize, pk: bool) -> Result<(), String> {
    let mem_end = DRAM_BASE + state.memory.len() as u64;
    let brk_start = (load.image_end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let mmap_base = (mem_end - 8 * 1024 * 1024) & !(PAGE_SIZE - 1);
    state.syscall = SyscallState::new();
    state.syscall.init_mem_layout(brk_start, mmap_base);

    let tp = setup_tls(&mut state.memory, load.tls)?;
    let dtb_addr = install_dtb(&mut state.memory, mem_mb)?;

    let trap_handler_addr = if pk {
        install_pk_trap_handler(&mut state.memory)?
    } else {
        0
    };
    let initial_regs = if pk {
        setup_pk_stack(&mut state.memory, &load)?
    } else {
        InitialRegs {
            sp: 0,
            a0: 0,
            a1: dtb_addr,
            a2: 0,
        }
    };

    let hart = HartInit {
        entry: load.entry,
        trap_handler_addr,
        regs: initial_regs,
        tp,
        pk,
    };
    let tp_ptr = hart.tp.as_ref().map(|v| v as *const u64).unwrap_or(std::ptr::null());

    let initialized = unsafe {
        spike_init_hart_raw(
            ctx,
            hart.entry,
            hart.trap_handler_addr,
            hart.regs.sp,
            hart.regs.a0,
            hart.regs.a1,
            hart.regs.a2,
            tp_ptr,
            hart.pk,
        )
    };
    if initialized {
        Ok(())
    } else {
        Err("failed to initialize Spike hart state".to_string())
    }
}

struct InitialRegs {
    sp: u64,
    a0: u64,
    a1: u64,
    a2: u64,
}

fn setup_tls(memory: &mut [u8], tls: Option<TlsInfo>) -> Result<Option<u64>, String> {
    let Some(tls) = tls else {
        return Ok(None);
    };

    let align = tls.align.max(16);
    let tls_size = align_up(tls.memsz, align);
    let total_size = tls_size + align;
    let tls_area_addr = DRAM_BASE + memory.len() as u64 - total_size - 0x10000;
    let tp = align_up(tls_area_addr, align);
    let copy_size = tls.filesz.min(tls.memsz) as usize;

    if copy_size > 0 {
        let src_offset = guest_offset(memory, tls.vaddr)?;
        let dst_offset = guest_offset(memory, tp)?;
        let src = memory
            .get(src_offset..src_offset + copy_size)
            .ok_or_else(|| format!("TLS source exceeds memory: addr=0x{:x} size={copy_size}", tls.vaddr))?
            .to_vec();
        let dst = memory
            .get_mut(dst_offset..dst_offset + copy_size)
            .ok_or_else(|| format!("TLS destination exceeds memory: addr=0x{tp:x} size={copy_size}"))?;
        dst.copy_from_slice(&src);
    }

    if tls.memsz > tls.filesz {
        let bss_start = tp + tls.filesz;
        let bss_offset = guest_offset(memory, bss_start)?;
        let bss_size = (tls.memsz - tls.filesz) as usize;
        memory
            .get_mut(bss_offset..bss_offset + bss_size)
            .ok_or_else(|| format!("TLS BSS exceeds memory: addr=0x{bss_start:x} size={bss_size}"))?
            .fill(0);
    }

    write_guest(memory, tp, &tp.to_le_bytes())?;
    Ok(Some(tp))
}

fn install_dtb(memory: &mut [u8], mem_mb: usize) -> Result<u64, String> {
    let dtb = DtbBuilder::build_minimal(DRAM_BASE, mem_mb as u64 * (1 << 20), None, None);
    let mem_end = DRAM_BASE + memory.len() as u64;
    let dtb_addr = align_down(mem_end - 0x20_0000 - dtb.len() as u64, PAGE_SIZE);
    write_guest(memory, dtb_addr, &dtb)?;
    Ok(dtb_addr)
}

fn install_pk_trap_handler(memory: &mut [u8]) -> Result<u64, String> {
    let trap_handler_addr = DRAM_BASE + memory.len() as u64 - 0x2000;
    let syscall_magic_addr = DRAM_BASE + memory.len() as u64 - 0x1000;
    let offset = syscall_magic_addr as i64 - trap_handler_addr as i64;
    let imm20 = ((offset >> 20) as u32) & 0x1;
    let imm10_1 = ((offset >> 1) as u32) & 0x3ff;
    let imm11 = ((offset >> 11) as u32) & 0x1;
    let imm19_12 = ((offset >> 12) as u32) & 0xff;
    let jal = 0x6f | (imm19_12 << 12) | (imm11 << 20) | (imm10_1 << 21) | (imm20 << 31);
    write_guest(memory, trap_handler_addr, &jal.to_le_bytes())?;
    Ok(trap_handler_addr)
}

fn setup_pk_stack(memory: &mut [u8], load: &LoadInfo) -> Result<InitialRegs, String> {
    const AT_NULL: u64 = 0;
    const AT_PHDR: u64 = 3;
    const AT_PHENT: u64 = 4;
    const AT_PHNUM: u64 = 5;
    const AT_PAGESZ: u64 = 6;
    const AT_BASE: u64 = 7;
    const AT_ENTRY: u64 = 9;
    const AT_UID: u64 = 11;
    const AT_EUID: u64 = 12;
    const AT_GID: u64 = 13;
    const AT_EGID: u64 = 14;
    const AT_HWCAP: u64 = 16;
    const AT_SECURE: u64 = 23;
    const AT_RANDOM: u64 = 25;
    const AT_HWCAP2: u64 = 26;
    const AT_EXECFN: u64 = 31;

    let stack_top = align_down(DRAM_BASE + memory.len() as u64 - 0x400000, 16);
    let prog_name = b"tutorial-linux\0";
    let random_len = 16u64;
    let word_size = 8u64;

    let string_addr = align_down(stack_top - prog_name.len() as u64, 16);
    let random_addr = align_down(string_addr - random_len, 16);
    let phdr = load.program_headers;

    let mut stack_entries = Vec::with_capacity(40);
    stack_entries.push(1);
    stack_entries.push(string_addr);
    stack_entries.push(0);
    let envp_offset_words = stack_entries.len() as u64;
    stack_entries.push(0);
    stack_entries.extend_from_slice(&[
        AT_PHDR,
        phdr.addr,
        AT_PHENT,
        phdr.entry_size,
        AT_PHNUM,
        phdr.count,
        AT_PAGESZ,
        PAGE_SIZE,
        AT_BASE,
        0,
        AT_HWCAP,
        0,
        AT_ENTRY,
        load.entry,
        AT_UID,
        0,
        AT_EUID,
        0,
        AT_GID,
        0,
        AT_EGID,
        0,
        AT_SECURE,
        0,
        AT_RANDOM,
        random_addr,
        AT_HWCAP2,
        0,
        AT_EXECFN,
        string_addr,
        AT_NULL,
        0,
    ]);

    let sp = align_down(random_addr - stack_entries.len() as u64 * word_size, 16);
    write_guest(memory, string_addr, prog_name)?;
    for i in 0..random_len {
        write_guest(memory, random_addr + i, &[0xA5u8 ^ i as u8])?;
    }
    for (i, value) in stack_entries.iter().enumerate() {
        write_guest(memory, sp + i as u64 * word_size, &value.to_le_bytes())?;
    }

    Ok(InitialRegs {
        sp,
        a0: 1,
        a1: sp + word_size,
        a2: sp + envp_offset_words * word_size,
    })
}

fn write_guest(memory: &mut [u8], addr: u64, bytes: &[u8]) -> Result<(), String> {
    let offset = guest_offset(memory, addr)?;
    let end = offset + bytes.len();
    if end > memory.len() {
        return Err(format!(
            "guest write exceeds memory: addr=0x{addr:x} size={}",
            bytes.len()
        ));
    }
    memory[offset..end].copy_from_slice(bytes);
    Ok(())
}

fn guest_offset(memory: &[u8], addr: u64) -> Result<usize, String> {
    if addr < DRAM_BASE {
        return Err(format!("guest address below DRAM: 0x{addr:x}"));
    }
    let offset = (addr - DRAM_BASE) as usize;
    if offset >= memory.len() {
        return Err(format!("guest address outside memory: 0x{addr:x}"));
    }
    Ok(offset)
}

fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}
