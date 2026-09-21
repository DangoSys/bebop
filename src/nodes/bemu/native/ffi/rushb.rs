use super::state::EmuState;
use super::*;

enum HostCommand {
    Execute {
        funct7: u32,
        xs1: u64,
        xs2: u64,
        reply: mpsc::Sender<u64>,
    },
    Mvin {
        xs1: u64,
        packed_xs2: u64,
        host_ptr: usize,
        reply: mpsc::Sender<()>,
    },
    Mvout {
        xs1: u64,
        packed_xs2: u64,
        host_ptr: usize,
        reply: mpsc::Sender<()>,
    },
    MvinMmio {
        xs1: u64,
        packed_xs2: u64,
        host_ptr: usize,
        reply: mpsc::Sender<()>,
    },
    Cycles {
        reply: mpsc::Sender<u64>,
    },
    Shutdown,
}

struct HostCore {
    commands: mpsc::Sender<HostCommand>,
    worker: thread::JoinHandle<()>,
}

struct HostState {
    cores: HashMap<u32, HostCore>,
}

static HOST_STATE: once_cell::sync::Lazy<Mutex<Option<HostState>>> = once_cell::sync::Lazy::new(|| Mutex::new(None));

fn spawn_host_core(core_id: u32) -> HostCore {
    let endpoint = crate::config::rushb_endpoint(core_id);
    let (commands, receiver) = mpsc::channel();
    let worker = thread::Builder::new()
        .name(format!("rushb-bemu-core-{core_id}"))
        .spawn(move || {
            crate::config::configure_core_with_virtual_bank_count(endpoint.core_index, endpoint.virtual_bank_count);
            let mut state = EmuState::new_host();
            while let Ok(command) = receiver.recv() {
                match command {
                    HostCommand::Execute {
                        funct7,
                        xs1,
                        xs2,
                        reply,
                    } => {
                        let _ = reply.send(host_execute(&mut state, funct7, xs1, xs2));
                    }
                    HostCommand::Mvin {
                        xs1,
                        packed_xs2,
                        host_ptr,
                        reply,
                    } => {
                        state.total_lat += inst::decode::cycles_after_issue(FUNCT7_MVIN, xs1, packed_xs2);
                        host_mvin(&mut state, xs1, packed_xs2, host_ptr as *const u8);
                        let _ = reply.send(());
                    }
                    HostCommand::Mvout {
                        xs1,
                        packed_xs2,
                        host_ptr,
                        reply,
                    } => {
                        state.total_lat += inst::decode::cycles_after_issue(FUNCT7_MVOUT, xs1, packed_xs2);
                        host_mvout(&mut state, xs1, packed_xs2, host_ptr as *mut u8);
                        let _ = reply.send(());
                    }
                    HostCommand::MvinMmio {
                        xs1,
                        packed_xs2,
                        host_ptr,
                        reply,
                    } => {
                        state.total_lat += inst::decode::cycles_after_issue(FUNCT7_MVIN_MMIO, xs1, packed_xs2);
                        host_mvin_mmio(&mut state, xs1, packed_xs2, host_ptr as *const u8);
                        let _ = reply.send(());
                    }
                    HostCommand::Cycles { reply } => {
                        let _ = reply.send(state.total_lat);
                    }
                    HostCommand::Shutdown => {
                        eprintln!(
                            "[INFO] rushB BEMU Core {core_id}: instructions={} matrix={} cycles={}",
                            state.npu_inst_id, state.matrix_instruction_count, state.total_lat
                        );
                        break;
                    }
                }
            }
        })
        .expect("failed to start rushB BEMU Core worker");
    HostCore { commands, worker }
}

fn with_core<R>(core_id: u32, f: impl FnOnce(&mpsc::Sender<HostCommand>) -> R) -> R {
    let mut guard = HOST_STATE.lock().expect("rushB BEMU state poisoned");
    let state = guard.as_mut().expect("rushB is not initialized; call rushb_init first");
    let core = state.cores.entry(core_id).or_insert_with(|| spawn_host_core(core_id));
    f(&core.commands)
}

pub(super) fn consumes_npu_inst_id(funct7: u32) -> bool {
    !matches!(funct7, 0 | 1)
}

fn host_execute(state: &mut EmuState, funct7: u32, xs1: u64, xs2: u64) -> u64 {
    state.barrier_hit = false;
    state.total_lat += inst::decode::cycles_after_issue(funct7, xs1, xs2);
    if consumes_npu_inst_id(funct7) {
        state.npu_inst_id = state.npu_inst_id.wrapping_add(1);
    }
    if matches!(
        crate::config::ball_domain::mnemonic_for_funct(funct7).as_deref(),
        Some("SMATMUL_OS" | "SMATMUL_WS")
    ) {
        state.matrix_instruction_count = state.matrix_instruction_count.wrapping_add(1);
    }
    let inst_id = state.npu_inst_id;
    state.bank_scoreboard.issue(inst_id);
    let mut ctx = inst::instruction::ExecContext {
        hart_id: state.hart_id,
        inst_id,
        memory: &mut state.memory,
        translate_dma: false,
        banks: inst::instruction::TrackedBanks::new(&mut state.banks, Some(&state.bank_scoreboard), inst_id),
        cfgs: &mut state.bank_cfgs,
        bank_map: &mut state.bank_map,
        shared: None,
        deferred_bank_frees: &mut state.deferred_bank_frees,
        mmio_banks: &mut state.mmio_banks,
        barrier_hit: &mut state.barrier_hit,
    };
    let result =
        inst::decode::execute_known(funct7, xs1, xs2, &mut ctx).unwrap_or_else(|| panic!("unknown funct7: {funct7}"));
    drop(ctx);
    state.bank_scoreboard.complete(inst_id);
    finish_deferred_bank_frees(
        &mut state.bank_cfgs,
        &mut state.bank_map,
        None,
        0,
        &mut state.deferred_bank_frees,
    );
    result
}

pub(super) fn finish_deferred_bank_frees(
    bank_cfgs: &mut [BankConfig],
    bank_map: &mut BankMap,
    shared_memory: Option<&Arc<SharedMemory>>,
    hart_id: usize,
    deferred_bank_frees: &mut Vec<u32>,
) {
    for bank_id in deferred_bank_frees.drain(..) {
        if crate::config::is_shared_vbank(bank_id as u64) {
            let shared = shared_memory.expect("shared bank storage is unavailable").banks_mut();
            shared.map.delete_hart_vbank(hart_id, bank_id);
            let core_count = shared.cfgs.len() / shared.virtual_bank_count;
            let core = hart_id % core_count;
            shared.cfgs[core * shared.virtual_bank_count + bank_id as usize] = BankConfig::default();
        } else {
            bank_map.delete_vbank(bank_id);
            bank_cfgs[bank_id as usize] = BankConfig::default();
        }
    }
}

fn host_mvin(state: &mut EmuState, xs1: u64, packed_xs2: u64, host_ptr: *const u8) {
    use crate::inst::decode::{rs1_b2, rs1_iter, xs2_mem_stride};

    assert!(!host_ptr.is_null(), "mvin: null host pointer");
    let bank_id = rs1_b2(xs1);
    let depth = rs1_iter(xs1);
    let (_, stride) = xs2_mem_stride(packed_xs2);
    assert!(
        !crate::config::is_shared_vbank(bank_id),
        "rushB mvin does not support shared banks"
    );
    assert!(depth > 0, "mvin: depth must be > 0");
    assert!(stride > 0, "mvin: stride must be > 0");

    let bi = bank_id as usize;
    assert!(state.bank_cfgs[bi].allocated, "mvin: bank {bank_id} not allocated");
    let cols = state.bank_cfgs[bi].cols;
    let groups = cols.max(1) as usize;

    unsafe {
        if groups > 1 {
            for row in 0..depth as usize {
                for group in 0..groups {
                    let p = state
                        .bank_map
                        .resolve_group(bank_id as u32, group as u32)
                        .unwrap_or_else(|| panic!("mvin: bank {bank_id} group {group} not mapped"));
                    let bank_offset = row * 16;
                    assert!(bank_offset + 16 <= bank_size(), "mvin: bank range");
                    let offset = row * groups * 16 * stride as usize + group * 16;
                    state.banks[p][bank_offset..bank_offset + 16]
                        .copy_from_slice(slice::from_raw_parts(host_ptr.add(offset), 16));
                }
            }
        } else {
            let p = state
                .bank_map
                .resolve(bank_id as u32)
                .unwrap_or_else(|| panic!("mvin: bank {bank_id} not mapped"));
            let matrix_mode_acc = cols == 4 && depth <= MATRIX_SIZE as u64;
            let line_bytes = if matrix_mode_acc { 64usize } else { 16usize };
            for row in 0..depth as usize {
                let bank_offset = row * line_bytes;
                assert!(bank_offset + line_bytes <= bank_size(), "mvin: bank range");
                let offset = row * line_bytes * stride as usize;
                state.banks[p][bank_offset..bank_offset + line_bytes]
                    .copy_from_slice(slice::from_raw_parts(host_ptr.add(offset), line_bytes));
            }
        }
    }
    state.bank_cfgs[bi].valid_rows = depth;
}

fn host_mvin_mmio(state: &mut EmuState, xs1: u64, packed_xs2: u64, host_ptr: *const u8) {
    assert!(!host_ptr.is_null(), "mvin_mmio: null host pointer");
    let rows = xs1 >> 30;
    let mmio_addr = ((packed_xs2 >> 39) & 0x1_ffff) as usize;
    let columns = ((packed_xs2 >> 56) & 0xff) as usize;
    let bytes_per_row = bank_row_bytes();
    assert!(rows > 0, "mvin_mmio: row count must be non-zero");
    assert!(
        (1..=bytes_per_row).contains(&columns),
        "mvin_mmio: invalid column count"
    );
    assert!(
        mmio_addr + rows as usize * bytes_per_row <= mmio_total_size(),
        "mvin_mmio: MMIO range out of bounds"
    );
    unsafe {
        for row in 0..rows as usize {
            for byte in 0..bytes_per_row {
                let address = mmio_addr + row * bytes_per_row + byte;
                let bank = address % mmio_bank_num();
                let offset = address / mmio_bank_num();
                state.mmio_banks[bank][offset] = if byte < columns {
                    *host_ptr.add(row * bytes_per_row + byte)
                } else {
                    0
                };
            }
        }
    }
}

fn host_mvout(state: &mut EmuState, xs1: u64, packed_xs2: u64, host_ptr: *mut u8) {
    use crate::inst::decode::{rs1_b0, rs1_iter, xs2_mem_stride};

    assert!(!host_ptr.is_null(), "mvout: null host pointer");
    let bank_id = rs1_b0(xs1);
    let depth = rs1_iter(xs1);
    let (_, stride) = xs2_mem_stride(packed_xs2);
    assert!(
        !crate::config::is_shared_vbank(bank_id),
        "rushB mvout does not support shared banks"
    );
    assert!(depth > 0, "mvout: depth must be > 0");
    assert!(stride > 0, "mvout: stride must be > 0");

    let bi = bank_id as usize;
    assert!(state.bank_cfgs[bi].allocated, "mvout: bank {bank_id} not allocated");
    let cols = state.bank_cfgs[bi].cols;
    let groups = cols.max(1) as usize;

    unsafe {
        if groups > 1 {
            for row in 0..depth as usize {
                for group in 0..groups {
                    let p = state
                        .bank_map
                        .resolve_group(bank_id as u32, group as u32)
                        .unwrap_or_else(|| panic!("mvout: bank {bank_id} group {group} not mapped"));
                    let bank_offset = row * 16;
                    assert!(bank_offset + 16 <= bank_size(), "mvout: bank range");
                    let offset = row * groups * 16 * stride as usize + group * 16;
                    slice::from_raw_parts_mut(host_ptr.add(offset), 16)
                        .copy_from_slice(&state.banks[p][bank_offset..bank_offset + 16]);
                }
            }
        } else {
            let p = state
                .bank_map
                .resolve(bank_id as u32)
                .unwrap_or_else(|| panic!("mvout: bank {bank_id} not mapped"));
            let matrix_mode_acc = cols == 4 && depth <= MATRIX_SIZE as u64;
            let line_bytes = if matrix_mode_acc { 64usize } else { 16usize };
            for row in 0..depth as usize {
                let bank_offset = row * line_bytes;
                assert!(bank_offset + line_bytes <= bank_size(), "mvout: bank range");
                let offset = row * line_bytes * stride as usize;
                slice::from_raw_parts_mut(host_ptr.add(offset), line_bytes)
                    .copy_from_slice(&state.banks[p][bank_offset..bank_offset + line_bytes]);
            }
        }
    }
}

#[cfg_attr(not(feature = "difftest"), no_mangle)]
pub extern "C" fn rushb_init() {
    let mut guard = HOST_STATE.lock().expect("rushB BEMU state poisoned");
    assert!(guard.is_none(), "rushB BEMU is already initialized");
    *guard = Some(HostState { cores: HashMap::new() });
}

#[cfg_attr(not(feature = "difftest"), no_mangle)]
pub extern "C" fn rushb_destroy() {
    let mut guard = HOST_STATE.lock().expect("rushB BEMU state poisoned");
    if let Some(state) = guard.take() {
        for core in state.cores.into_values() {
            let _ = core.commands.send(HostCommand::Shutdown);
            core.worker.join().expect("rushB BEMU Core worker panicked");
        }
    }
}

#[cfg_attr(not(feature = "difftest"), no_mangle)]
pub extern "C" fn rushb_mset(core_id: u32, xs1: u64, xs2: u64) {
    with_core(core_id, |commands| {
        let (reply, result) = mpsc::channel();
        commands
            .send(HostCommand::Execute {
                funct7: FUNCT7_MSET,
                xs1,
                xs2,
                reply,
            })
            .expect("rushB BEMU Core worker stopped");
        result.recv().expect("rushB BEMU Core worker stopped");
    });
}

#[cfg_attr(not(feature = "difftest"), no_mangle)]
pub extern "C" fn rushb_mvin(core_id: u32, xs1: u64, packed_xs2: u64, host_ptr: *const c_void) {
    with_core(core_id, |commands| {
        let (reply, result) = mpsc::channel();
        commands
            .send(HostCommand::Mvin {
                xs1,
                packed_xs2,
                host_ptr: host_ptr as usize,
                reply,
            })
            .expect("rushB BEMU Core worker stopped");
        result.recv().expect("rushB BEMU Core worker stopped");
    });
}

#[cfg_attr(not(feature = "difftest"), no_mangle)]
pub extern "C" fn rushb_mvin_mmio(core_id: u32, xs1: u64, packed_xs2: u64, host_ptr: *const c_void) {
    with_core(core_id, |commands| {
        let (reply, result) = mpsc::channel();
        commands
            .send(HostCommand::MvinMmio {
                xs1,
                packed_xs2,
                host_ptr: host_ptr as usize,
                reply,
            })
            .expect("rushB BEMU Core worker stopped");
        result.recv().expect("rushB BEMU Core worker stopped");
    });
}

#[cfg_attr(not(feature = "difftest"), no_mangle)]
pub extern "C" fn rushb_mvout(core_id: u32, xs1: u64, packed_xs2: u64, host_ptr: *mut c_void) {
    with_core(core_id, |commands| {
        let (reply, result) = mpsc::channel();
        commands
            .send(HostCommand::Mvout {
                xs1,
                packed_xs2,
                host_ptr: host_ptr as usize,
                reply,
            })
            .expect("rushB BEMU Core worker stopped");
        result.recv().expect("rushB BEMU Core worker stopped");
    });
}

#[cfg_attr(not(feature = "difftest"), no_mangle)]
pub extern "C" fn rushb_custom(core_id: u32, xs1: u64, xs2: u64, funct7: u32) {
    with_core(core_id, |commands| {
        let (reply, result) = mpsc::channel();
        commands
            .send(HostCommand::Execute {
                funct7,
                xs1,
                xs2,
                reply,
            })
            .expect("rushB BEMU Core worker stopped");
        result.recv().expect("rushB BEMU Core worker stopped");
    });
}

#[cfg_attr(not(feature = "difftest"), no_mangle)]
pub extern "C" fn rushb_cycles(core_id: u32) -> u64 {
    with_core(core_id, |commands| {
        let (reply, result) = mpsc::channel();
        commands
            .send(HostCommand::Cycles { reply })
            .expect("rushB BEMU Core worker stopped");
        result.recv().expect("rushB BEMU Core worker stopped")
    })
}
