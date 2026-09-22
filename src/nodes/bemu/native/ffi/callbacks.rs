use super::pk::PkVm;
use super::rushb::{consumes_npu_inst_id, finish_deferred_bank_frees};
use super::spike::align_up;
use super::state::{EmuState, SharedBankState};
use super::*;

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
    state.barrier_hit = false;
    let profile_started = state.profile.begin_npu();
    let lat = inst::decode::cycles_after_issue(funct7 as u32, xs1, xs2);
    state.total_lat += lat;
    state.trace.set_bemu_clk(state.total_lat);
    if consumes_npu_inst_id(funct7 as u32) {
        state.npu_inst_id = state.npu_inst_id.wrapping_add(1);
    }
    let inst_id = state.npu_inst_id;
    let trace = &mut state.trace as *mut TraceState;
    let enable = funct7 >> 4;
    let btrace = state.trace.btrace_enabled()
        && matches!(enable, 1..=4)
        && !matches!(funct7 as u32, FUNCT7_MSET | FUNCT7_MVIN_MMIO);
    if btrace {
        state.bank_scoreboard.issue(inst_id);
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
        shared_memory,
        hart_id,
        bank_scoreboard,
        deferred_bank_frees,
        mmio_banks,
        barrier_hit,
        uart: _,
        syscall: _,
        pk_vm: _,
        trace: _,
        ..
    } = state;

    let result = unsafe {
        with_trace_ptr(trace, || {
            let shared_state = shared_memory.as_ref().map(|memory| memory.banks_mut());
            let (tracked_banks, shared) = match shared_state {
                Some(SharedBankState {
                    storage,
                    cfgs,
                    map,
                    virtual_bank_count,
                }) => (
                    inst::instruction::TrackedBanks::with_shared(
                        banks,
                        storage,
                        btrace.then_some(&*bank_scoreboard),
                        inst_id,
                    ),
                    Some(inst::instruction::SharedBankContext {
                        cfgs,
                        bank_map: map,
                        hart_id: *hart_id,
                        virtual_bank_count: *virtual_bank_count,
                    }),
                ),
                None => (
                    inst::instruction::TrackedBanks::new(banks, btrace.then_some(&*bank_scoreboard), inst_id),
                    None,
                ),
            };
            let mut ctx = inst::instruction::ExecContext {
                hart_id: *hart_id,
                inst_id,
                memory,
                translate_dma: true,
                banks: tracked_banks,
                cfgs: bank_cfgs,
                bank_map,
                shared,
                deferred_bank_frees,
                mmio_banks,
                barrier_hit,
            };

            inst::decode::execute_known(funct7 as u32, xs1, xs2, &mut ctx)
                .unwrap_or_else(|| panic!("unknown funct7: {}", funct7))
        })
    };

    if btrace {
        bank_scoreboard.complete(inst_id);
        let op_type = format!("funct7_{}", funct7);
        let r0_enabled = matches!(enable, 1 | 3 | 4);
        let r1_enabled = enable == 4;
        let w0_enabled = matches!(enable, 2 | 3 | 4);
        let r0_vbank = (xs1 & 0x3ff) as u32;
        let r1_vbank = ((xs1 >> 10) & 0x3ff) as u32;
        let w0_vbank = ((xs1 >> 20) & 0x3ff) as u32;
        let mut hashes = std::collections::BTreeMap::new();
        for (enabled, vbank_id) in [(r0_enabled, r0_vbank), (r1_enabled, r1_vbank), (w0_enabled, w0_vbank)] {
            if !enabled {
                continue;
            }
            if hashes.contains_key(&vbank_id) {
                continue;
            }
            let mut status_hash = 0;
            let cols = if crate::config::is_shared_vbank(vbank_id as u64) {
                let shared = shared_memory
                    .as_ref()
                    .expect("shared bank storage is unavailable")
                    .banks_mut();
                let core = *hart_id % (shared.cfgs.len() / shared.virtual_bank_count);
                shared.cfgs[core * shared.virtual_bank_count + vbank_id as usize].cols
            } else {
                bank_cfgs[vbank_id as usize].cols
            };
            for group_id in 0..cols as u32 {
                let physical_hash = if crate::config::is_shared_vbank(vbank_id as u64) {
                    let shared = shared_memory
                        .as_ref()
                        .expect("shared bank storage is unavailable")
                        .banks_mut();
                    let pbank_id = shared
                        .map
                        .resolve_hart_group(*hart_id, vbank_id, group_id)
                        .unwrap_or_else(|| panic!("unmapped shared vbank {vbank_id} group {group_id}"));
                    shared.storage[pbank_id].status_hash()
                } else {
                    let pbank_id = bank_map
                        .resolve_group(vbank_id, group_id)
                        .unwrap_or_else(|| panic!("unmapped vbank {vbank_id} group {group_id}"));
                    banks[pbank_id].status_hash()
                };
                status_hash = combine_bank_hash(status_hash, group_id, physical_hash);
            }
            hashes.insert(vbank_id, status_hash);
        }
        let slot = |enabled: bool, vbank_id: u32| BTraceBank {
            vbank_id: if enabled { vbank_id } else { INVALID_VBANK },
            hash: if enabled { hashes[&vbank_id] } else { 0 },
        };
        unsafe {
            with_trace_ptr(trace, || {
                crate::trace::bemu_btrace(
                    inst_id,
                    *hart_id as u64,
                    slot(r0_enabled, r0_vbank),
                    slot(r1_enabled, r1_vbank),
                    slot(w0_enabled, w0_vbank),
                    funct7 as u32,
                    &op_type,
                    pc,
                );
            })
        };
    }
    finish_deferred_bank_frees(
        bank_cfgs,
        bank_map,
        shared_memory.as_ref(),
        *hart_id,
        deferred_bank_frees,
    );
    state.profile.end_npu(funct7, profile_started);

    result
}

#[no_mangle]
pub extern "C" fn bemu_take_barrier(state: *mut c_void) -> bool {
    let state = unsafe { state_mut(state) };
    std::mem::take(&mut state.barrier_hit)
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
        }
        SYS_MMAP => {
            let len = align_up(a1, PAGE_SIZE);
            if result != 0 && len != 0 {
                pk_vm.alloc_user_pages(memory, result, len, 0x2 | 0x4)?;
            }
        }
        SYS_MUNMAP => pk_vm.free_user_pages(a0, a1)?,
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

#[no_mangle]
pub extern "C" fn clint_mmio_load(clint_ptr: *mut u8, addr: u64, size: usize, value: *mut u64) -> bool {
    let clint = unsafe { &*(clint_ptr as *const Clint) };
    let Some(result) = clint.load(addr - bebop_clint::BASE, size) else {
        return false;
    };
    unsafe { *value = result };
    true
}

#[no_mangle]
pub extern "C" fn clint_mmio_store(clint_ptr: *mut u8, addr: u64, size: usize, value: u64) -> bool {
    let clint = unsafe { &mut *(clint_ptr as *mut Clint) };
    clint.store(addr - bebop_clint::BASE, size, value)
}

#[no_mangle]
pub extern "C" fn clint_tick(clint_ptr: *mut u8, cycles: u64) -> u32 {
    let clint = unsafe { &mut *(clint_ptr as *mut Clint) };
    clint.tick(cycles)
}

#[no_mangle]
pub extern "C" fn clint_time(clint_ptr: *const u8) -> u64 {
    let clint = unsafe { &*(clint_ptr as *const Clint) };
    clint.time()
}

#[no_mangle]
pub extern "C" fn plic_mmio_load(plic_ptr: *mut u8, addr: u64, size: usize, value: *mut u64) -> bool {
    let plic = unsafe { &*(plic_ptr as *const Plic) };
    let Some(result) = plic.load(addr - bebop_plic::BASE, size) else {
        return false;
    };
    unsafe { *value = result };
    true
}

#[no_mangle]
pub extern "C" fn plic_mmio_store(plic_ptr: *mut u8, addr: u64, size: usize, value: u64) -> bool {
    let plic = unsafe { &mut *(plic_ptr as *mut Plic) };
    plic.store(addr - bebop_plic::BASE, size, value)
}

extern "C" {
    pub(super) fn spike_create_raw(
        isa: *const c_char,
        procs: usize,
        hart_id: usize,
        mem_ptr: *mut u8,
        mem_size: usize,
        log_path: *const c_char,
        uart_ptr: *mut u8,
        clint_ptr: *mut u8,
        plic_ptr: *mut u8,
        emu_state: *mut c_void,
        profile: bool,
    ) -> *mut c_void;
    pub(super) fn spike_init_hart_raw(
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
    pub(super) fn spike_step_raw(ctx: *mut c_void, count: u64) -> i32;
    pub(super) fn spike_finished_raw(ctx: *mut c_void) -> bool;
    pub(super) fn spike_exit_code_raw(ctx: *mut c_void) -> i32;
    pub(super) fn spike_stop_raw(ctx: *mut c_void, code: i32);
    pub(super) fn spike_step_elapsed_ns_raw(ctx: *mut c_void) -> u64;
    pub(super) fn spike_destroy_raw(ctx: *mut c_void);

}
