use super::{
    banks::*,
    instructions::{self, FUNCT7_MSET, FUNCT7_MVIN_MMIO},
    kernels,
    trace::{with_trace_ptr, TraceConfig, TraceState},
};
use crate::{
    ball::BallStates,
    core::{memory::GuestAccess, shared, CoreState},
};
use bebop_bank_hash::{combine_bank_hash, BTraceBank};
use bebop_bemu_profile::BemuProfile;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

pub(crate) struct PrivateState {
    pub(crate) banks: Vec<crate::thread::banks::PrivateBank>,
    pub(crate) bank_cfgs: Vec<BankConfig>,
    pub(crate) bank_map: BankMap,
    pub(crate) row_bytes: usize,
}

/// A Ball accelerator: its banks, its Balls and the RVV kernel engine of its GP
/// domain. A core attaches one to its RoCC port; a hardware thread owns one.
pub struct Accelerator {
    private: Arc<Mutex<PrivateState>>,
    pub(crate) kernel: Option<kernels::Engine>,
    pub(crate) shared_memory: Option<Arc<CoreState>>,
    pub(crate) execution_id: usize,
    pub(crate) endpoint_index: Option<usize>,
    pub(crate) writes: crate::thread::banks::BankWrites,
    pub(crate) deferred_bank_frees: Vec<u32>,
    metadata: Vec<Vec<u8>>,
    ball_states: BallStates,
    pub(crate) npu_inst_id: u64,
    pub(crate) trace: TraceState,
    pub(crate) profile: BemuProfile,
    pub(crate) barrier_hit: bool,
}

impl Accelerator {
    pub(crate) fn finish_bank_frees(&mut self) {
        let mut private = self.private.lock().expect("private banks poisoned");
        for bank_id in self.deferred_bank_frees.drain(..) {
            if crate::configs::is_shared_vbank(bank_id as u64) {
                let mut shared = self
                    .shared_memory
                    .as_ref()
                    .expect("shared banks are unavailable")
                    .banks
                    .lock()
                    .expect("shared banks poisoned");
                shared.map.delete_hart_vbank(
                    self.endpoint_index.expect("shared bank requires compute thread"),
                    bank_id,
                );
                let thread = self.endpoint_index.expect("shared bank requires compute thread");
                let index = thread * shared.virtual_bank_count + bank_id as usize;
                shared.cfgs[index] = BankConfig::default();
            } else {
                private.bank_map.delete_vbank(bank_id);
                private.bank_cfgs[bank_id as usize] = BankConfig::default();
            }
        }
    }

    /// The accelerator a core connects to the RoCC port of its CPU.
    pub fn attached(cpu: &crate::core::Cpu, options: &crate::ModelOptions) -> Result<Self, String> {
        let endpoint = crate::configs::Endpoint::Core(cpu.hart_id());
        crate::configs::configure(endpoint);
        let core = cpu.state();
        Self::new(
            &options.log_dir,
            options.trace.clone(),
            options.profile,
            endpoint.execution_id(),
            Some(core.endpoint_index(endpoint)),
            Some(Arc::clone(core)),
        )
    }

    pub fn take_barrier(&mut self) -> bool {
        std::mem::take(&mut self.barrier_hit)
    }

    pub fn profile_report(
        &self,
        total: std::time::Duration,
        cpu: &crate::core::Cpu,
    ) -> Option<bebop_bemu_profile::BemuProfileReport> {
        self.profile.report(total, cpu.cpu_elapsed())
    }

    pub fn new(
        log_dir: &Path,
        trace_config: TraceConfig,
        profile: bool,
        execution_id: usize,
        endpoint_index: Option<usize>,
        shared_memory: Option<Arc<CoreState>>,
    ) -> Result<Self, String> {
        let btrace = trace_config.btrace;
        if btrace {
            if let Some(shared) = &shared_memory {
                for bank in &mut shared.banks.lock().expect("shared banks poisoned").storage {
                    bank.enable_hash();
                }
            }
        }
        let private = Arc::new(Mutex::new(PrivateState {
            banks: (0..bank_num())
                .map(|_| crate::thread::banks::PrivateBank::new(bank_size(), btrace))
                .collect(),
            bank_cfgs: vec![BankConfig::default(); virtual_bank_num()],
            bank_map: BankMap::new(bank_num()),
            row_bytes: crate::configs::bank_row_bytes(),
        }));
        if let Some(core) = &shared_memory {
            if let Some(thread) = endpoint_index {
                let previous = core
                    .private_endpoints
                    .lock()
                    .expect("private endpoints poisoned")
                    .insert(thread, Arc::clone(&private));
                assert!(previous.is_none(), "duplicate thread bank endpoint");
            }
        }
        Ok(Self {
            private,
            kernel: crate::configs::gpdomain().map(|config| {
                kernels::Engine::new(
                    config.v_len as usize,
                    config.e_len as usize,
                    config.i_buf_words as usize * 4,
                )
            }),
            shared_memory,
            execution_id,
            endpoint_index,
            writes: crate::thread::banks::BankWrites::new(),
            deferred_bank_frees: Vec::new(),
            metadata: vec![vec![0; mmio_bank_size()]; mmio_bank_num()],
            ball_states: BallStates::new(),
            npu_inst_id: 0,
            trace: TraceState::new(log_dir, trace_config).map_err(|e| e.to_string())?,
            profile: BemuProfile::new(profile),
            barrier_hit: false,
        })
    }
}

impl crate::core::Rocc for Accelerator {
    fn custom(
        &mut self,
        guest: GuestAccess<'_>,
        request: crate::core::riscv::hart::CustomInstruction,
        cycle: u64,
    ) -> Result<u64, crate::core::riscv::Trap> {
        execute(
            self,
            guest,
            (request.instruction >> 25) as u8,
            request.rs1,
            request.rs2,
            request.pc,
            cycle,
        )
    }
    fn barrier(&self) -> bool {
        self.barrier_hit
    }
}

pub(crate) fn execute(
    state: &mut Accelerator,
    mut memory: GuestAccess<'_>,
    funct7: u8,
    xs1: u64,
    xs2: u64,
    pc: u64,
    cycle: u64,
) -> Result<u64, crate::core::riscv::Trap> {
    preflight(state, &mut memory, funct7, xs1, xs2)?;
    state.barrier_hit = false;
    let profile_started = state.profile.begin_npu();
    state.trace.advance_event();
    if !matches!(funct7, 0 | 1) {
        state.npu_inst_id = state.npu_inst_id.wrapping_add(1);
    }
    let inst_id = state.npu_inst_id;
    let trace = &mut state.trace as *mut TraceState;
    let enable = funct7 >> 4;
    let btrace = state.trace.btrace_enabled()
        && pc != 0
        && (funct7 == 79 || matches!(enable, 2..=4))
        && !matches!(funct7 as u32, FUNCT7_MSET | FUNCT7_MVIN_MMIO)
        && instructions::decode::rs1_b2(xs1) < crate::configs::virtual_bank_num() as u64;
    if btrace {
        let update =
            instructions::decode::rs1_update(xs1) || matches!(funct7, 34 | 79) || (funct7 == 33 && xs2 >> 63 != 0);
        state.writes.issue(inst_id, !update);
    }

    if state.trace.itrace {
        unsafe {
            with_trace_ptr(trace, || {
                crate::thread::trace::itrace(crate::thread::trace::ITraceEvent {
                    cycle,
                    modeled_cycles: match funct7 {
                        44 | 79 => None,
                        _ => Some(instructions::decode::cycles_after_issue(funct7 as u32, xs1, xs2)),
                    },
                    funct: funct7 as u32,
                    pc,
                    rs1: xs1,
                    rs2: xs2,
                });
            })
        };
    }

    if funct7 == 13 {
        let core = state.shared_memory.as_ref().expect("mvover requires a CoreState");
        core.mvover(xs1, xs2).unwrap_or_else(|error| panic!("mvover: {error}"));
        state.profile.end_npu(funct7, profile_started);
        return Ok(0);
    }
    let mut private = state.private.lock().expect("private banks poisoned");
    let PrivateState {
        banks,
        bank_cfgs,
        bank_map,
        ..
    } = &mut *private;
    let Accelerator {
        kernel,
        shared_memory,
        execution_id,
        endpoint_index,
        writes,
        deferred_bank_frees,
        metadata,
        ball_states,
        barrier_hit,
        trace: _,
        ..
    } = state;

    let result = unsafe {
        with_trace_ptr(trace, || {
            let shared_range = crate::configs::shared_vbank_base()..crate::configs::virtual_bank_num();
            let accesses_shared = funct7 >= 16
                && [
                    instructions::decode::rs1_b0(xs1),
                    instructions::decode::rs1_b1(xs1),
                    instructions::decode::rs1_b2(xs1),
                ]
                .into_iter()
                .any(|bank| {
                    let bank = bank as usize;
                    bank > crate::configs::max_private_vbank() && shared_range.contains(&bank)
                });
            let mut shared_state = shared_memory
                .as_ref()
                .filter(|_| accesses_shared)
                .map(|memory| memory.banks.lock().expect("shared banks poisoned"));
            let (tracked_banks, shared) = match shared_state.as_deref_mut() {
                Some(shared::State {
                    storage,
                    cfgs,
                    map,
                    virtual_bank_count,
                }) => (
                    crate::thread::banks::TrackedBanks::with_shared(
                        banks,
                        storage,
                        btrace.then_some(&*writes),
                        inst_id,
                    ),
                    Some(crate::thread::banks::SharedBankContext {
                        cfgs,
                        bank_map: map,
                        local_thread: endpoint_index.expect("shared bank requires compute thread"),
                        virtual_bank_count: *virtual_bank_count,
                    }),
                ),
                None => (
                    crate::thread::banks::TrackedBanks::new(banks, btrace.then_some(&*writes), inst_id),
                    None,
                ),
            };
            let mut ctx = instructions::context::ExecContext {
                hart_id: *execution_id,
                inst_id,
                cycle,
                memory,
                kernel,
                banks: tracked_banks,
                cfgs: bank_cfgs,
                bank_map,
                shared,
                deferred_bank_frees,
                mmio_banks: metadata,
                ball_states: ball_states,
                ball_id: crate::configs::ball_domain::instance_for_funct(funct7 as u32),
                barrier_hit,
            };

            instructions::decode::execute_known(funct7 as u32, xs1, xs2, &mut ctx)
                .unwrap_or_else(|| panic!("unknown funct7: {}", funct7))
        })
    };

    if btrace {
        let (defines_value, written) = writes.complete(inst_id);
        let op_type = format!("funct7_{}", funct7);
        let w0_vbank = if funct7 == 79 {
            result as u32
        } else {
            instructions::decode::rs1_b2(xs1) as u32
        };
        let mut status_hash = 0;
        let cols = if crate::configs::is_shared_vbank(w0_vbank as u64) {
            let shared = shared_memory
                .as_ref()
                .expect("shared bank storage is unavailable")
                .banks
                .lock()
                .expect("shared banks poisoned");
            let thread = endpoint_index.expect("shared bank requires compute thread");
            shared.cfgs[thread * shared.virtual_bank_count + w0_vbank as usize].cols
        } else {
            bank_cfgs[w0_vbank as usize].cols
        };
        for group_id in 0..cols as u32 {
            let physical_hash = if crate::configs::is_shared_vbank(w0_vbank as u64) {
                let mut shared = shared_memory
                    .as_ref()
                    .expect("shared bank storage is unavailable")
                    .banks
                    .lock()
                    .expect("shared banks poisoned");
                let pbank_id = shared
                    .map
                    .resolve_hart_group(
                        endpoint_index.expect("shared bank requires compute thread"),
                        w0_vbank,
                        group_id,
                    )
                    .unwrap_or_else(|| panic!("unmapped shared vbank {w0_vbank} group {group_id}"));
                // A fresh destination version can complete without a write,
                // for example an unfinished internal accumulation. Its old
                // bytes do not belong to the newly defined bank value.
                if defines_value && !written.contains(&(banks.len() + pbank_id)) {
                    shared.storage[pbank_id].define_value();
                }
                shared.storage[pbank_id].status_hash()
            } else {
                let pbank_id = bank_map
                    .resolve_group(w0_vbank, group_id)
                    .unwrap_or_else(|| panic!("unmapped vbank {w0_vbank} group {group_id}"));
                if defines_value && !written.contains(&pbank_id) {
                    banks[pbank_id].define_value();
                }
                banks[pbank_id].status_hash()
            };
            status_hash = combine_bank_hash(status_hash, group_id, physical_hash);
        }
        unsafe {
            with_trace_ptr(trace, || {
                crate::thread::trace::bemu_btrace(
                    inst_id,
                    *execution_id as u64,
                    BTraceBank {
                        owner_hart_id: if crate::configs::is_shared_vbank(w0_vbank as u64) {
                            shared_memory
                                .as_ref()
                                .expect("shared bank storage is unavailable")
                                .bank_owner_id(*execution_id, true) as u64
                        } else {
                            *execution_id as u64
                        },
                        vbank_id: w0_vbank,
                        hash: status_hash,
                    },
                    funct7 as u32,
                    &op_type,
                    pc,
                );
            })
        };
    }
    drop(private);
    state.finish_bank_frees();
    state.profile.end_npu(funct7, profile_started);

    Ok(result)
}

fn preflight(
    state: &Accelerator,
    memory: &mut GuestAccess<'_>,
    funct: u8,
    xs1: u64,
    xs2: u64,
) -> Result<(), crate::core::riscv::Trap> {
    use crate::core::riscv::Access;
    use instructions::decode::{rs1_b0, rs1_b2, DmaRows, Mvin2dGeometry};
    if funct == 44 {
        let bytes = instructions::decode::rs1_iter(xs1) as usize;
        let bank = rs1_b2(xs1) as usize;
        let base = crate::configs::virtual_bank_num();
        assert!(bank >= base && bank < base + 2 && bytes >= 24 && bytes % 4 == 0);
        return memory.check_range(xs2, bytes, Access::Load);
    }
    if !matches!(funct, 16 | 33 | 34) {
        return Ok(());
    }
    let bank = if funct == 16 { rs1_b0(xs1) } else { rs1_b2(xs1) };
    let (groups, bank_rows) = if crate::configs::is_shared_vbank(bank) {
        let core = state.shared_memory.as_ref().expect("shared banks unavailable");
        let shared = core.banks.lock().expect("shared banks poisoned");
        let cfg = &shared.cfgs[state.endpoint_index.expect("shared bank requires compute thread")
            * shared.virtual_bank_count
            + bank as usize];
        assert!(cfg.allocated, "DMA bank not allocated");
        let group = if funct == 34 {
            0
        } else {
            DmaRows::decode(xs1, xs2, cfg.cols.max(1)).selected_group.unwrap_or(0)
        };
        let physical = shared
            .map
            .resolve_hart_group(state.endpoint_index.unwrap(), bank as u32, group as u32)
            .expect("DMA shared bank has no physical mapping");
        (cfg.cols.max(1), shared.storage[physical].len() as u64 / 16)
    } else {
        let private = state.private.lock().expect("private banks poisoned");
        let cfg = &private.bank_cfgs[bank as usize];
        assert!(cfg.allocated, "DMA bank not allocated");
        let group = if funct == 34 {
            0
        } else {
            DmaRows::decode(xs1, xs2, cfg.cols.max(1)).selected_group.unwrap_or(0)
        };
        let physical = private
            .bank_map
            .resolve_group(bank as u32, group as u32)
            .expect("DMA bank has no physical mapping");
        (cfg.cols.max(1), private.banks[physical].len() as u64 / 16)
    };
    if funct == 34 {
        let geometry = Mvin2dGeometry::decode(xs1, xs2);
        assert!(
            geometry.dst_base + geometry.height * geometry.width <= bank_rows,
            "mvin_2d: destination exceeds bank"
        );
        for row in 0..geometry.height {
            for column in 0..geometry.width {
                memory.check_range(geometry.source(row, column), 16, Access::Load)?;
            }
        }
    } else {
        let geometry = DmaRows::decode(xs1, xs2, groups);
        assert!(geometry.depth <= bank_rows, "DMA exceeds bank");
        let access = if funct == 16 { Access::Store } else { Access::Load };
        if geometry.stride == 1 {
            memory.check_range(
                geometry.address,
                (geometry.depth * geometry.groups * 16) as usize,
                access,
            )?;
        } else {
            for row in 0..geometry.depth {
                memory.check_range(geometry.source(row, 0), (geometry.groups * 16) as usize, access)?;
            }
        }
    }
    Ok(())
}
