use super::instructions::{self, FUNCT7_MSET, FUNCT7_MVIN_MMIO};
use bebop_bank_hash::{combine_bank_hash, BTraceBank};
use bebop_bemu_profile::BemuProfile;
use generic::shared::{self, PrivateState};
use generic::{
    banks::*,
    blink::{BallBus, BallStates},
    guest::GuestAccess,
    operators,
    trace::{with_trace_ptr, TraceConfig, TraceState},
};
use hart::HartState;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

/// A Ball accelerator: its banks, its Balls and the RVV operator engine of its GP
/// domain. A hart attaches one to its RoCC port; a hardware thread owns one.
pub struct Accelerator {
    pub(crate) timing: super::timing::Timing,
    private: Arc<Mutex<PrivateState>>,
    pub(crate) operator: Option<operators::Engine>,
    pub(crate) hart: Arc<HartState>,
    pub(crate) execution_id: usize,
    pub(crate) endpoint_index: usize,
    pub(crate) writes: generic::banks::BankWrites,
    pub(crate) deferred_bank_frees: Vec<u32>,
    metadata: Vec<Vec<u8>>,
    ball_states: BallStates,
    balls: BallBus,
    pub(crate) npu_inst_id: u64,
    pub(crate) trace: TraceState,
    pub(crate) profile: BemuProfile,
    pub(crate) barrier_hit: bool,
}

impl Accelerator {
    pub(crate) fn finish_bank_frees(&mut self) {
        let mut private = self.private.lock().expect("private banks poisoned");
        for bank_id in self.deferred_bank_frees.drain(..) {
            if generic::configs::is_shared_vbank(bank_id as u64) {
                let mut shared = self.hart.banks.lock().expect("shared banks poisoned");
                shared.map.delete_hart_vbank(self.endpoint_index, bank_id);
                let thread = self.endpoint_index;
                let index = thread * shared.virtual_bank_count + bank_id as usize;
                shared.cfgs[index] = BankConfig::default();
            } else {
                private.bank_map.delete_vbank(bank_id);
                private.bank_cfgs[bank_id as usize] = BankConfig::default();
            }
        }
    }

    /// The accelerator a hart connects to the RoCC port of its CPU.
    pub fn attached(
        hart_id: usize,
        hart: Arc<HartState>,
        balls: BallBus,
        options: &generic::options::ModelOptions,
    ) -> Result<Self, String> {
        let endpoint = generic::configs::Endpoint::Hart(hart_id);
        generic::configs::configure(endpoint);
        Self::new(
            &options.log_dir,
            options.trace.clone(),
            options.profile,
            endpoint.execution_id(),
            hart.endpoint_index(endpoint),
            hart,
            balls,
        )
    }

    pub fn take_barrier(&mut self) -> bool {
        std::mem::take(&mut self.barrier_hit)
    }

    pub fn profile_report(
        &self,
        total: std::time::Duration,
        cpu_elapsed: std::time::Duration,
    ) -> Option<bebop_bemu_profile::BemuProfileReport> {
        if let Some(operator) = &self.operator {
            operator.write_profile();
        }
        self.profile.report(total, cpu_elapsed)
    }

    fn new(
        log_dir: &Path,
        trace_config: TraceConfig,
        profile: bool,
        execution_id: usize,
        endpoint_index: usize,
        hart: Arc<HartState>,
        balls: BallBus,
    ) -> Result<Self, String> {
        let btrace = trace_config.btrace;
        if btrace {
            for bank in &mut hart.banks.lock().expect("shared banks poisoned").storage {
                bank.enable_hash();
            }
        }
        let private = Arc::new(Mutex::new(PrivateState {
            banks: (0..bank_num())
                .map(|_| generic::banks::PrivateBank::new(bank_size(), btrace))
                .collect(),
            bank_cfgs: vec![BankConfig::default(); virtual_bank_num()],
            bank_map: BankMap::new(bank_num()),
            row_bytes: generic::configs::bank_row_bytes(),
            shared_vbank_base: generic::configs::shared_vbank_base(),
            max_private_vbank: generic::configs::max_private_vbank(),
        }));
        hart.connect(endpoint_index, Arc::clone(&private));
        Ok(Self {
            timing: generic::configs::with_accelerator(|p| {
                super::timing::Timing::new(
                    p.ball_domain
                        .balls
                        .iter()
                        .map(|ball| ball.id as usize + 1)
                        .max()
                        .unwrap_or(0),
                    p.rob_entries,
                )
            }),
            private,
            operator: generic::configs::gpdomain().map(|config| {
                operators::Engine::new(
                    &config,
                    bank_size(),
                    profile.then(|| log_dir.join("gp-instructions.csv")),
                )
            }),
            hart,
            execution_id,
            endpoint_index,
            writes: generic::banks::BankWrites::new(),
            deferred_bank_frees: Vec::new(),
            metadata: vec![vec![0; mmio_bank_size()]; mmio_bank_num()],
            ball_states: BallStates::new(),
            balls,
            npu_inst_id: 0,
            trace: TraceState::new(log_dir, trace_config).map_err(|e| e.to_string())?,
            profile: BemuProfile::new(profile),
            barrier_hit: false,
        })
    }
}

impl generic::rocc::Rocc for Accelerator {
    fn cycles(&self) -> (u64, u64) {
        (self.timing.accepted, self.timing.complete)
    }
    fn custom(
        &mut self,
        guest: GuestAccess<'_>,
        request: generic::riscv::registers::CustomInstruction,
        cycle: u64,
    ) -> Result<u64, generic::riscv::Trap> {
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
) -> Result<u64, generic::riscv::Trap> {
    preflight(state, &mut memory, funct7, xs1, xs2)?;
    state.barrier_hit = false;
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
        && instructions::decode::rs1_b2(xs1) < generic::configs::virtual_bank_num() as u64;
    if btrace {
        let update =
            instructions::decode::rs1_update(xs1) || matches!(funct7, 34 | 79) || (funct7 == 33 && xs2 >> 63 != 0);
        state.writes.issue(inst_id, !update);
    }

    let itrace = state.trace.itrace;
    if itrace && funct7 == 13 {
        unsafe {
            with_trace_ptr(trace, || {
                generic::trace::itrace(generic::trace::ITraceEvent {
                    cycle,
                    modeled_cycles: Some((((xs2 >> 32) & 0xffff) + 1) * 2),
                    funct: funct7 as u32,
                    pc,
                    operator_entry: None,
                    rs1: xs1,
                    rs2: xs2,
                });
            })
        };
    }

    if funct7 == 13 {
        let profile_started = state.profile.begin_npu();
        crate::transfer::mvover(&state.hart, xs1, xs2).unwrap_or_else(|error| panic!("mvover: {error}"));
        state.profile.end_npu(funct7, profile_started, 0);
        return Ok(0);
    }
    // Submit DMA events in hart acceptance order.
    if matches!(funct7, 16 | 33..=37 | 44) {
        let hart = memory.owner;
        if hart.clint.concurrent() {
            let accepted = state.timing.ready(cycle);
            hart.clint.advance_to(hart.hart_id, accepted);
            if !hart.tasks.shared_access(accepted, &hart.clint) {
                return Ok(0);
            }
        }
    }
    let profile_started = state.profile.begin_npu();
    let mut private = state.private.lock().expect("private banks poisoned");
    let PrivateState {
        banks,
        bank_cfgs,
        bank_map,
        ..
    } = &mut *private;
    let Accelerator {
        timing,
        operator,
        hart: shared_memory,
        execution_id,
        endpoint_index,
        writes,
        deferred_bank_frees,
        metadata,
        ball_states,
        balls,
        barrier_hit,
        trace: _,
        ..
    } = state;

    let (result, duration) = unsafe {
        with_trace_ptr(trace, || {
            let shared_range = generic::configs::shared_vbank_base()..generic::configs::virtual_bank_num();
            let accesses_shared = funct7 >= 16
                && !matches!(funct7, 18 | 19 | 36 | 37)
                && [
                    instructions::decode::rs1_b0(xs1),
                    instructions::decode::rs1_b1(xs1),
                    instructions::decode::rs1_b2(xs1),
                ]
                .into_iter()
                .any(|bank| {
                    let bank = bank as usize;
                    bank > generic::configs::max_private_vbank() && shared_range.contains(&bank)
                });
            let mut shared_state = accesses_shared.then(|| shared_memory.banks.lock().expect("shared banks poisoned"));
            let (tracked_banks, shared) = match shared_state.as_deref_mut() {
                Some(shared::State {
                    storage,
                    cfgs,
                    map,
                    virtual_bank_count,
                    bank_entries,
                    ..
                }) => (
                    generic::banks::TrackedBanks::with_shared(banks, storage, btrace.then_some(&*writes), inst_id),
                    Some(generic::banks::SharedBankContext {
                        bank_entries: *bank_entries,
                        cfgs,
                        bank_map: map,
                        local_thread: *endpoint_index,
                        virtual_bank_count: *virtual_bank_count,
                    }),
                ),
                None => (
                    generic::banks::TrackedBanks::new(banks, btrace.then_some(&*writes), inst_id),
                    None,
                ),
            };
            let mut ctx = instructions::context::ExecContext {
                balls: *balls,
                execute: instructions::execute_known,
                hart_id: *execution_id,
                inst_id,
                cycle,
                memory,
                operator,
                banks: tracked_banks,
                cfgs: bank_cfgs,
                bank_map,
                shared,
                deferred_bank_frees,
                mmio_banks: metadata,
                ball_states: ball_states,
                ball_id: generic::configs::ball_domain::instance_for_funct(funct7 as u32),
                barrier_hit,
            };

            // Operator timing follows its executed instructions and memory
            // operands. Other operations expose their latency before execution.
            let mut operator_result = None;
            let (_, duration) = timing.issue(cycle, funct7, xs1, xs2, ctx.ball_id, |start| {
                ctx.cycle = start;
                if matches!(funct7, 44 | 79) {
                    operator_result = Some(
                        instructions::decode::execute_known(funct7 as u32, xs1, xs2, &mut ctx)
                            .unwrap_or_else(|| panic!("unknown funct7: {}", funct7)),
                    );
                }
                instructions::decode::cycles_after_issue(funct7 as u32, xs1, xs2, &ctx)
            });
            if itrace {
                generic::trace::itrace(generic::trace::ITraceEvent {
                    cycle: ctx.cycle,
                    modeled_cycles: Some(duration),
                    funct: funct7 as u32,
                    pc,
                    operator_entry: (funct7 == 79).then(|| ctx.operator.as_ref().unwrap().entry),
                    rs1: xs1,
                    rs2: xs2,
                });
            }

            let result = if matches!(funct7, 44 | 79) {
                operator_result.unwrap()
            } else {
                instructions::decode::execute_known(funct7 as u32, xs1, xs2, &mut ctx)
                    .unwrap_or_else(|| panic!("unknown funct7: {}", funct7))
            };
            (result, duration)
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
        let cols = if generic::configs::is_shared_vbank(w0_vbank as u64) {
            let shared = shared_memory.banks.lock().expect("shared banks poisoned");
            let thread = *endpoint_index;
            shared.cfgs[thread * shared.virtual_bank_count + w0_vbank as usize].cols
        } else {
            bank_cfgs[w0_vbank as usize].cols
        };
        for group_id in 0..cols as u32 {
            let physical_hash = if generic::configs::is_shared_vbank(w0_vbank as u64) {
                let mut shared = shared_memory.banks.lock().expect("shared banks poisoned");
                let pbank_id = shared
                    .map
                    .resolve_hart_group(*endpoint_index, w0_vbank, group_id)
                    .unwrap_or_else(|| panic!("unmapped shared vbank {w0_vbank} group {group_id}"));
                // A fresh destination version can complete without a write,
                // for example an unfinished internal accumulation. Its old
                // bytes do not belong to the newly defined bank value.
                if defines_value && !written.contains(&(banks.len() + pbank_id)) {
                    shared.storage[pbank_id].define_value();
                }
                if defines_value {
                    shared.storage[pbank_id].forget_undefined();
                }
                shared.storage[pbank_id].status_hash()
            } else {
                let pbank_id = bank_map
                    .resolve_group(w0_vbank, group_id)
                    .unwrap_or_else(|| panic!("unmapped vbank {w0_vbank} group {group_id}"));
                if defines_value && !written.contains(&pbank_id) {
                    banks[pbank_id].define_value();
                }
                if defines_value {
                    banks[pbank_id].forget_undefined();
                }
                banks[pbank_id].status_hash()
            };
            status_hash = combine_bank_hash(status_hash, group_id, physical_hash);
        }
        unsafe {
            with_trace_ptr(trace, || {
                generic::trace::bemu_btrace(
                    inst_id,
                    *execution_id as u64,
                    BTraceBank {
                        owner_hart_id: if generic::configs::is_shared_vbank(w0_vbank as u64) {
                            shared_memory.bank_owner_id(*execution_id, true) as u64
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
    state.profile.end_npu(funct7, profile_started, duration);

    Ok(result)
}

fn preflight(
    state: &Accelerator,
    memory: &mut GuestAccess<'_>,
    funct: u8,
    xs1: u64,
    xs2: u64,
) -> Result<(), generic::riscv::Trap> {
    use generic::riscv::Access;
    use instructions::decode::{rs1_b0, rs1_b2, DmaRows, Mvin2dGeometry};
    if funct == 44 {
        let bytes = instructions::decode::rs1_iter(xs1) as usize;
        let bank = rs1_b2(xs1) as usize;
        let base = generic::configs::virtual_bank_num();
        assert!(bank >= base && bank < base + 2 && bytes >= 24 && bytes % 4 == 0);
        return memory.check_range(xs2, bytes, Access::Load);
    }
    if matches!(funct, 18 | 19 | 36 | 37) {
        let write = matches!(funct, 18 | 19);
        let g = instructions::dma_strided::Geometry::decode(xs1, xs2, write, matches!(funct, 19 | 37));
        assert!(
            !generic::configs::is_shared_vbank(g.bank),
            "strided DMA cannot access shared banks"
        );
        {
            let private = state.private.lock().expect("private banks poisoned");
            let cfg = &private.bank_cfgs[g.bank as usize];
            assert!(
                cfg.allocated && g.group < cfg.cols,
                "strided DMA group is not allocated"
            );
            let bank = private
                .bank_map
                .resolve_group(g.bank as u32, g.group as u32)
                .expect("strided DMA bank has no physical mapping");
            assert!(
                g.first + ((g.rows >> g.block_log2) - 1) * g.bank_stride + (1u64 << g.block_log2) - 1
                    < private.banks[bank].len() as u64 / 16,
                "strided DMA exceeds bank"
            );
        }
        for i in 0..(g.rows >> g.block_log2) {
            memory.check_range(
                g.address + i * g.memory_stride,
                g.bytes << g.block_log2,
                if write { Access::Store } else { Access::Load },
            )?;
        }
        return Ok(());
    }
    if !matches!(funct, 16 | 33 | 34) {
        return Ok(());
    }
    let bank = if funct == 16 { rs1_b0(xs1) } else { rs1_b2(xs1) };
    assert!(
        !generic::configs::is_shared_vbank(bank),
        "DDR DMA cannot access shared banks"
    );
    let (groups, bank_rows) = {
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
