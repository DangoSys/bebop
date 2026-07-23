use bebop_bank_hash::{
    bank_hash, submit_runtime_bank_boundary, BankDigest, BankHashSource, BankVersionRef, InstructionBankBoundaryPacket,
};
use bebop_dtb::DtbBuilder;
use bebop_elf::{load_elf, LoadInfo, TlsInfo};
use bebop_syscall::{add_guest_mapping, handle_syscall_with_state, set_guest_mappings, SyscallState};
use bebop_uart::Uart;
use std::collections::BTreeMap;
use std::os::raw::{c_char, c_void};
use std::path::Path;

use crate::bank::{BankConfig, BankMap, BANK_NUM, BANK_SIZE};
use crate::inst;
use crate::trace::{with_trace_ptr, TraceConfig, TraceState};

const DRAM_BASE: u64 = 0x80000000;
// UART base address (matches test workloads)
const UART_BASE: u64 = 0x60020000;
const PAGE_SIZE: u64 = 4096;
const USER_TOP: u64 = 0x40_0000_0000;
const USER_STACK_SIZE: u64 = 8 * 1024 * 1024;
const PK_PT_RESERVE: u64 = 2 * 1024 * 1024;
const PK_HIGH_RESERVE: u64 = 64 * 1024 * 1024;
const SYS_BRK: u64 = 214;
const SYS_MMAP: u64 = 222;

struct EmuState {
    memory: Vec<u8>,
    banks: Vec<Vec<u8>>,
    bank_cfgs: Vec<BankConfig>,
    bank_map: BankMap,
    bank_scoreboard: inst::instruction::BankScoreboard,
    bank_versions: BTreeMap<u32, u32>,
    mmio_banks: [[u8; 1024]; 16],
    mmio_region_table: [crate::inst::instruction::MmioRegion; 32],
    total_lat: u64,
    npu_instruction_id: u64,
    uart: Uart,
    syscall: SyscallState,
    pk_vm: Option<PkVm>,
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
            bank_scoreboard: inst::instruction::BankScoreboard::new(BANK_NUM),
            bank_versions: BTreeMap::new(),
            mmio_banks: [[0u8; 1024]; 16],
            mmio_region_table: [crate::inst::instruction::MmioRegion::default(); 32],
            total_lat: 0,
            npu_instruction_id: 0,
            uart: Uart::new(),
            syscall: SyscallState::new(),
            pk_vm: None,
            trace: TraceState::new(log_dir, trace_config).map_err(|e| e.to_string())?,
        })
    }

    fn reset_accel(&mut self) {
        for b in &mut self.banks {
            b.fill(0);
        }
        self.bank_cfgs.fill(BankConfig::default());
        self.bank_map = BankMap::new(BANK_NUM);
        self.bank_scoreboard.reset();
        self.bank_versions.clear();
        for bank in &mut self.mmio_banks {
            bank.fill(0);
        }
        self.mmio_region_table = [crate::inst::instruction::MmioRegion::default(); 32];
        self.total_lat = 0;
        self.npu_instruction_id = 0;
    }
}

#[derive(Clone, Copy)]
struct GuestMap {
    virt: u64,
    phys: u64,
    len: u64,
}

struct PkVm {
    root: u64,
    next_pt: u64,
    pt_end: u64,
    next_page: u64,
    page_end: u64,
    maps: Vec<GuestMap>,
}

impl PkVm {
    fn new(memory: &mut [u8], root: u64, pt_end: u64, next_page: u64, page_end: u64) -> Result<Self, String> {
        let mut vm = Self {
            root,
            next_pt: root,
            pt_end,
            next_page,
            page_end,
            maps: Vec::new(),
        };
        vm.alloc_table(memory)?;
        Ok(vm)
    }

    fn satp(&self) -> u64 {
        (8u64 << 60) | ((self.root >> 12) & 0x0000_0fff_ffff_ffff)
    }

    fn map_range(&mut self, memory: &mut [u8], virt: u64, phys: u64, len: u64, flags: u64) -> Result<(), String> {
        if len == 0 {
            return Ok(());
        }
        let virt_start = align_down(virt, PAGE_SIZE);
        let phys_start = align_down(phys, PAGE_SIZE);
        let virt_end = align_up(virt + len, PAGE_SIZE);
        let mut vaddr = virt_start;
        let mut paddr = phys_start;
        while vaddr < virt_end {
            self.map_page(memory, vaddr, paddr, flags)?;
            vaddr += PAGE_SIZE;
            paddr += PAGE_SIZE;
        }
        self.maps.push(GuestMap {
            virt: virt_start,
            phys: phys_start,
            len: virt_end - virt_start,
        });
        add_guest_mapping(virt_start, phys_start, virt_end - virt_start);
        Ok(())
    }

    fn alloc_user_pages(&mut self, memory: &mut [u8], virt: u64, len: u64, flags: u64) -> Result<u64, String> {
        let phys = self.next_page;
        let size = align_up(len, PAGE_SIZE);
        self.next_page = self
            .next_page
            .checked_add(size)
            .ok_or_else(|| "pk physical page allocator overflow".to_string())?;
        if self.next_page > self.page_end {
            return Err("pk user page reserve exhausted".to_string());
        }
        let off = guest_offset(memory, phys)?;
        let end = off + size as usize;
        if end > memory.len() {
            return Err(format!(
                "pk physical page allocator exceeds memory: addr=0x{phys:x} size={size}"
            ));
        }
        memory[off..end].fill(0);
        self.map_range(memory, virt, phys, size, flags)?;
        Ok(phys)
    }

    fn write_user(&self, memory: &mut [u8], virt: u64, bytes: &[u8]) -> Result<(), String> {
        let phys = self
            .virt_to_phys(virt, bytes.len() as u64)
            .ok_or_else(|| format!("user write to unmapped VA: addr=0x{virt:x} size={}", bytes.len()))?;
        write_guest(memory, phys, bytes)
    }

    fn virt_to_phys(&self, virt: u64, len: u64) -> Option<u64> {
        let end = virt.checked_add(len)?;
        for map in self.maps.iter().rev() {
            let map_end = map.virt.checked_add(map.len)?;
            if virt >= map.virt && end <= map_end {
                return map.phys.checked_add(virt - map.virt);
            }
        }
        None
    }

    fn map_page(&mut self, memory: &mut [u8], virt: u64, phys: u64, flags: u64) -> Result<(), String> {
        let vpn = [(virt >> 12) & 0x1ff, (virt >> 21) & 0x1ff, (virt >> 30) & 0x1ff];
        let l2 = self.ensure_table(memory, self.root, vpn[2])?;
        let l1 = self.ensure_table(memory, l2, vpn[1])?;
        let leaf = ((phys >> 12) << 10) | flags | 0x1 | 0x10 | 0x40 | 0x80;
        self.write_pte(memory, l1, vpn[0], leaf)
    }

    fn ensure_table(&mut self, memory: &mut [u8], table: u64, idx: u64) -> Result<u64, String> {
        let pte = self.read_pte(memory, table, idx)?;
        if pte & 0x1 != 0 {
            return Ok(((pte >> 10) << 12) & !0xfffu64);
        }
        let child = self.alloc_table(memory)?;
        self.write_pte(memory, table, idx, ((child >> 12) << 10) | 0x1)?;
        Ok(child)
    }

    fn alloc_table(&mut self, memory: &mut [u8]) -> Result<u64, String> {
        let table = self.next_pt;
        self.next_pt = self
            .next_pt
            .checked_add(PAGE_SIZE)
            .ok_or_else(|| "pk page table allocator overflow".to_string())?;
        if self.next_pt > self.pt_end {
            return Err("pk page table reserve exhausted".to_string());
        }
        let off = guest_offset(memory, table)?;
        memory[off..off + PAGE_SIZE as usize].fill(0);
        Ok(table)
    }

    fn read_pte(&self, memory: &[u8], table: u64, idx: u64) -> Result<u64, String> {
        let off = guest_offset(memory, table + idx * 8)?;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&memory[off..off + 8]);
        Ok(u64::from_le_bytes(bytes))
    }

    fn write_pte(&self, memory: &mut [u8], table: u64, idx: u64, value: u64) -> Result<(), String> {
        write_guest(memory, table + idx * 8, &value.to_le_bytes())
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
    // Fence and barrier are handled by the RTL frontend and never allocate a
    // GlobalROB entry. Keep BEMU's semantic sequence aligned to RTL's ROB
    // allocation order by excluding those frontend-only commands.
    let track_bank_boundary = state.trace.btrace_enabled() && !matches!(funct7, 0 | 1);
    if track_bank_boundary {
        state.npu_instruction_id = state.npu_instruction_id.wrapping_add(1);
    }
    let instruction_id = state.npu_instruction_id;
    let boundary_cycle = state.total_lat;
    let trace = &mut state.trace as *mut TraceState;
    if track_bank_boundary {
        state.bank_scoreboard.issue(instruction_id);
    }

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
        bank_scoreboard,
        bank_versions,
        mmio_banks,
        mmio_region_table,
        uart: _,
        syscall: _,
        pk_vm: _,
        trace: _,
        ..
    } = state;

    let result = unsafe {
        with_trace_ptr(trace, || {
            let mut ctx = inst::instruction::ExecContext {
                memory,
                banks: inst::instruction::TrackedBanks::new(
                    banks,
                    track_bank_boundary.then_some(&*bank_scoreboard),
                    instruction_id,
                ),
                cfgs: bank_cfgs,
                bank_map,
                mmio_banks,
                mmio_region_table,
            };

            inst::decode::execute_known(funct7 as u32, xs1, xs2, &mut ctx)
                .unwrap_or_else(|| panic!("unknown funct7: {}", funct7))
        })
    };

    if track_bank_boundary {
        let access = bank_scoreboard.complete(instruction_id);
        let op_type = format!("funct7_{}", funct7);
        let reads: Vec<_> = access
            .reads
            .iter()
            .map(|&pbank| {
                let bank_id = bank_map
                    .logical_bank_for_pbank(pbank)
                    .unwrap_or_else(|| panic!("BEMU read from unmapped physical Bank {pbank}"));
                BankVersionRef {
                    bank_id,
                    version: bank_versions.get(&bank_id).copied().unwrap_or(0),
                }
            })
            .collect();
        let mut writes = Vec::with_capacity(access.writes.len());
        let mut expected_banks = Vec::with_capacity(access.writes.len());
        unsafe {
            with_trace_ptr(trace, || {
                for pbank in access.writes {
                    let bank_id = bank_map
                        .logical_bank_for_pbank(pbank)
                        .unwrap_or_else(|| panic!("BEMU write to unmapped physical Bank {pbank}"));
                    let version = bank_versions.entry(bank_id).or_insert(0);
                    *version = version.wrapping_add(1);
                    let bank = &banks[pbank];
                    let hash = bank_hash(bank);
                    expected_banks.push(bank_id);
                    writes.push(BankDigest {
                        bank_id,
                        version: *version,
                        hash,
                    });
                    crate::trace::bemu_bank_hash(
                        instruction_id,
                        instruction_id,
                        bank_id,
                        *version,
                        funct7 as u32,
                        &op_type,
                        hash,
                        pc,
                    );
                }
            })
        };
        submit_runtime_bank_boundary(InstructionBankBoundaryPacket {
            record_type: "instruction_bank_boundary",
            source: BankHashSource::Bemu,
            instruction_id,
            semantic_seq: instruction_id,
            funct7: funct7 as u32,
            pc,
            expected_banks: expected_banks.clone(),
            actual_banks: expected_banks,
            reads,
            writes,
            cycle: boundary_cycle,
            cancelled: false,
        });
    }

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
    let old_brk = state.syscall.brk_addr;
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
    if let Some(mut pk_vm) = state.pk_vm.take() {
        let map_result = map_syscall_result(&mut state.memory, &mut pk_vm, old_brk, syscall_num, a0, a1, result);
        state.pk_vm = Some(pk_vm);
        if let Err(e) = map_result {
            eprintln!("[ERROR] pk syscall mapping failed: {e}");
            state.syscall.exit_code = Some(1);
            return u64::MAX;
        }
    }
    result
}

fn map_syscall_result(
    memory: &mut [u8],
    pk_vm: &mut PkVm,
    old_brk: u64,
    syscall_num: u64,
    a0: u64,
    a1: u64,
    result: u64,
) -> Result<(), String> {
    if (result as i64) < 0 {
        return Ok(());
    }

    match syscall_num {
        SYS_BRK if result > old_brk => {
            let start = align_up(old_brk, PAGE_SIZE);
            let end = align_up(result, PAGE_SIZE);
            if end > start {
                pk_vm.alloc_user_pages(memory, start, end - start, 0x2 | 0x4)?;
            }
            if let Some(first) = pk_vm.maps.first() {
                crate::bank::set_fast_addr_map(first.virt, first.phys, result.saturating_sub(first.virt));
            }
        }
        SYS_MMAP => {
            let len = align_up(a1, PAGE_SIZE);
            if result != 0 && len != 0 {
                pk_vm.alloc_user_pages(memory, result, len, 0x2 | 0x4)?;
            }
        }
        _ => {
            let _ = a0;
        }
    }
    Ok(())
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
        satp: u64,
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

    pub fn scratchpad_snapshot(&self) -> Vec<Vec<u8>> {
        self.state.banks.clone()
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
    satp: u64,
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
    crate::bank::clear_addr_cache();
    state.syscall = SyscallState::new();
    state.pk_vm = None;
    set_guest_mappings(&[]);

    let brk_start = if pk {
        align_up(load.analysis.max_vaddr, PAGE_SIZE)
    } else {
        align_up(load.image_end, PAGE_SIZE)
    };
    let mmap_base = if pk {
        align_down(USER_TOP - USER_STACK_SIZE - 8 * 1024 * 1024, PAGE_SIZE)
    } else {
        align_down(mem_end - 8 * 1024 * 1024, PAGE_SIZE)
    };
    state.syscall.init_mem_layout(brk_start, mmap_base);
    if pk {
        state
            .syscall
            .set_mem_bounds(load.analysis.min_vaddr, USER_TOP - USER_STACK_SIZE);
        crate::bank::set_fast_addr_map(
            load.analysis.min_vaddr,
            DRAM_BASE,
            brk_start.saturating_sub(load.analysis.min_vaddr),
        );
    }

    let tp = if pk {
        None
    } else {
        setup_tls(&mut state.memory, load.tls)?
    };
    let dtb_addr = install_dtb(&mut state.memory, mem_mb)?;

    let trap_handler_addr = if pk {
        install_pk_trap_handler(&mut state.memory)?
    } else {
        0
    };
    let (entry, satp, initial_regs) = if pk {
        let pk_vm = setup_pk_vm(&mut state.memory, &load)?;
        let regs = setup_pk_stack(&mut state.memory, &pk_vm, &load)?;
        let satp = pk_vm.satp();
        state.pk_vm = Some(pk_vm);
        (load.analysis.original_entry, satp, regs)
    } else {
        (
            load.entry,
            0,
            InitialRegs {
                sp: 0,
                a0: 0,
                a1: dtb_addr,
                a2: 0,
            },
        )
    };

    let hart = HartInit {
        entry,
        trap_handler_addr,
        satp,
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
            hart.satp,
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

fn setup_pk_vm(memory: &mut [u8], load: &LoadInfo) -> Result<PkVm, String> {
    let mem_end = DRAM_BASE + memory.len() as u64;
    let pt_root = align_down(mem_end - PK_HIGH_RESERVE, PAGE_SIZE);
    let pt_end = pt_root + PK_PT_RESERVE;
    let stack_phys_bottom = align_down(pt_root - USER_STACK_SIZE, PAGE_SIZE);
    let stack_virt_bottom = USER_TOP - USER_STACK_SIZE;
    let next_page = align_up(load.image_end, PAGE_SIZE);
    let mut vm = PkVm::new(memory, pt_root, pt_end, next_page, stack_phys_bottom)?;

    for seg in &load.analysis.load_segments {
        let phys = if load.analysis.is_pie || load.analysis.needs_relocation {
            DRAM_BASE + (seg.vaddr - load.analysis.min_vaddr)
        } else {
            seg.vaddr
        };
        let mut flags = 0;
        if (seg.flags & 0x4) != 0 {
            flags |= 0x2;
        }
        if (seg.flags & 0x2) != 0 {
            flags |= 0x4;
        }
        if (seg.flags & 0x1) != 0 {
            flags |= 0x8;
        }
        vm.map_range(memory, seg.vaddr, phys, seg.memsz, flags)?;
    }

    vm.map_range(memory, stack_virt_bottom, stack_phys_bottom, USER_STACK_SIZE, 0x2 | 0x4)?;
    let maps: Vec<(u64, u64, u64)> = vm.maps.iter().map(|m| (m.virt, m.phys, m.len)).collect();
    set_guest_mappings(&maps);
    Ok(vm)
}

fn setup_pk_stack(memory: &mut [u8], vm: &PkVm, load: &LoadInfo) -> Result<InitialRegs, String> {
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

    let stack_top = align_down(USER_TOP - 16, 16);
    let prog_name = b"tutorial-linux\0";
    let random_len = 16u64;
    let word_size = 8u64;

    let string_addr = align_down(stack_top - prog_name.len() as u64, 16);
    let random_addr = align_down(string_addr - random_len, 16);
    let phdr_addr = user_image_addr(load, load.program_headers.addr)?;

    let mut stack_entries = Vec::with_capacity(40);
    stack_entries.push(1);
    stack_entries.push(string_addr);
    stack_entries.push(0);
    stack_entries.push(0);
    stack_entries.extend_from_slice(&[
        AT_PHDR,
        phdr_addr,
        AT_PHENT,
        load.program_headers.entry_size,
        AT_PHNUM,
        load.program_headers.count,
        AT_PAGESZ,
        PAGE_SIZE,
        AT_BASE,
        0,
        AT_HWCAP,
        0,
        AT_ENTRY,
        load.analysis.original_entry,
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
    vm.write_user(memory, string_addr, prog_name)?;
    for i in 0..random_len {
        vm.write_user(memory, random_addr + i, &[0xA5u8 ^ i as u8])?;
    }
    for (i, value) in stack_entries.iter().enumerate() {
        vm.write_user(memory, sp + i as u64 * word_size, &value.to_le_bytes())?;
    }

    Ok(InitialRegs {
        sp,
        a0: 0,
        a1: 0,
        a2: 0,
    })
}

fn user_image_addr(load: &LoadInfo, phys_addr: u64) -> Result<u64, String> {
    if !load.analysis.is_pie && !load.analysis.needs_relocation {
        return Ok(phys_addr);
    }
    if phys_addr < DRAM_BASE || phys_addr > load.image_end {
        return Err(format!("loaded image address outside relocated image: 0x{phys_addr:x}"));
    }
    Ok(load.analysis.min_vaddr + (phys_addr - DRAM_BASE))
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
