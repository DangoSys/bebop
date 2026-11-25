use super::instructions::{self, FUNCT7_MSET, FUNCT7_MVIN_MMIO};
use bebop_bank_hash::{combine_bank_hash, BTraceBank};
use bebop_bemu_profile::BemuProfile;
use generic::{
    banks::*,
    blink::{BallBus, BallStates},
    guest::GuestAccess,
    operators,
    tasks::{Event, Record},
    trace::{with_trace_ptr, TraceConfig, TraceState},
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

struct PrivateState {
    banks: Vec<generic::banks::PrivateBank>,
    bank_cfgs: Vec<BankConfig>,
    bank_map: BankMap,
}

/// The Ball accelerator of a hardware thread: its banks, its Balls and the RVV operator engine
/// of its GP domain. It executes without its time, which the thread's timer keeps.
pub struct Accelerator {
    private: Arc<Mutex<PrivateState>>,
    pub(crate) operator: Option<operators::Engine>,
    pub(crate) execution_id: usize,
    pub(crate) writes: generic::banks::BankWrites,
    pub(crate) deferred_bank_frees: Vec<u32>,
    metadata: Vec<Vec<u8>>,
    ball_states: BallStates,
    balls: BallBus,
    pub(crate) npu_inst_id: u64,
    pub(crate) trace: TraceState,
    pub(crate) profile: BemuProfile,
}

impl Accelerator {
    pub(crate) fn finish_bank_frees(&mut self) {
        let mut private = self.private.lock().expect("private banks poisoned");
        for bank_id in self.deferred_bank_frees.drain(..) {
            private.bank_map.delete_vbank(bank_id);
            private.bank_cfgs[bank_id as usize] = BankConfig::default();
        }
    }

    pub fn new(
        log_dir: &Path,
        trace_config: TraceConfig,
        profile: bool,
        execution_id: usize,
        balls: BallBus,
    ) -> Result<Self, String> {
        let btrace = trace_config.btrace;
        let private = Arc::new(Mutex::new(PrivateState {
            banks: (0..bank_num())
                .map(|_| generic::banks::PrivateBank::new(bank_size(), btrace))
                .collect(),
            bank_cfgs: vec![BankConfig::default(); virtual_bank_num()],
            bank_map: BankMap::new(bank_num()),
        }));
        Ok(Self {
            private,
            operator: generic::configs::gpdomain().map(|config| {
                operators::Engine::new(
                    &config,
                    bank_size(),
                    profile.then(|| log_dir.join("gp-instructions.csv")),
                )
            }),
            execution_id,
            writes: generic::banks::BankWrites::new(),
            deferred_bank_frees: Vec::new(),
            metadata: vec![vec![0; mmio_bank_size()]; mmio_bank_num()],
            ball_states: BallStates::new(),
            balls,
            npu_inst_id: 0,
            trace: TraceState::new(log_dir, trace_config).map_err(|e| e.to_string())?,
            profile: BemuProfile::new(profile),
        })
    }
}

/// Executes a custom instruction of thread `index` and records it for the timing unit of the
/// hart, `ant` cycles of the thread's CPU after its previous record.
pub(crate) fn execute(
    state: &mut Accelerator,
    mut memory: GuestAccess<'_>,
    funct7: u8,
    xs1: u64,
    xs2: u64,
    pc: u64,
    index: usize,
    ant: u64,
) -> Result<u64, generic::riscv::Trap> {
    preflight(state, &mut memory, funct7, xs1, xs2)?;
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
    assert!(funct7 != 1, "Thread programs cannot issue inter-thread barriers");
    let tasks = &memory.owner.tasks;
    let profile_started = state.profile.begin_npu();
    let mut private = state.private.lock().expect("private banks poisoned");
    let PrivateState {
        banks,
        bank_cfgs,
        bank_map,
        ..
    } = &mut *private;
    let Accelerator {
        operator,
        execution_id,
        writes,
        deferred_bank_frees,
        metadata,
        ball_states,
        balls,
        trace: _,
        ..
    } = state;

    let (result, duration, waited) = unsafe {
        with_trace_ptr(trace, || {
            let tracked_banks = generic::banks::TrackedBanks::new(banks, btrace.then_some(&*writes), inst_id);
            let mut ctx = instructions::context::ExecContext {
                balls: *balls,
                execute: instructions::execute_known,
                hart_id: *execution_id,
                inst_id,
                // The thread executes ahead of its time: the timing unit of the hart knows
                // the cycle the instruction issues at.
                cycle: 0,
                memory,
                operator,
                banks: tracked_banks,
                cfgs: bank_cfgs,
                bank_map,
                shared: None,
                deferred_bank_frees,
                mmio_banks: metadata,
                ball_states: ball_states,
                ball_id: generic::configs::ball_domain::instance_for_funct(funct7 as u32),
                barrier_hit: &mut false,
            };

            // Operator timing follows its executed instructions and memory
            // operands. Other operations expose their latency before execution.
            let operator_result = matches!(funct7, 44 | 79).then(|| {
                instructions::decode::execute_known(funct7 as u32, xs1, xs2, &mut ctx)
                    .unwrap_or_else(|| panic!("unknown funct7: {}", funct7))
            });
            let record = Record {
                ant,
                event: Event::Custom {
                    funct7,
                    xs1,
                    xs2,
                    ball: ctx.ball_id,
                    cycles: instructions::decode::cycles_after_issue(funct7 as u32, xs1, xs2, &ctx),
                    transfer: instructions::decode::transfer_after_issue(funct7 as u32, xs1, xs2, &ctx),
                },
            };
            // A trace or a profile reports the cycles of the instruction: only then does the
            // thread wait for the timing unit to replay it.
            let mut duration = 0;
            let mut waited = std::time::Duration::ZERO;
            if itrace || profile_started.is_some() {
                let waiting = std::time::Instant::now();
                if let Some((cycle, modeled_cycles)) = tasks.sync(index, record) {
                    duration = modeled_cycles;
                    generic::trace::itrace(generic::trace::ITraceEvent {
                        cycle,
                        modeled_cycles: Some(modeled_cycles),
                        funct: funct7 as u32,
                        pc,
                        operator_entry: (funct7 == 79).then(|| ctx.operator.as_ref().unwrap().entry),
                        rs1: xs1,
                        rs2: xs2,
                    });
                }
                waited = waiting.elapsed();
            } else {
                tasks.record(index, record);
            }

            let result = operator_result.unwrap_or_else(|| {
                instructions::decode::execute_known(funct7 as u32, xs1, xs2, &mut ctx)
                    .unwrap_or_else(|| panic!("unknown funct7: {}", funct7))
            });
            (result, duration, waited)
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
        let cols = bank_cfgs[w0_vbank as usize].cols;
        for group_id in 0..cols as u32 {
            let physical_hash = {
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
                        owner_hart_id: *execution_id as u64,
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
    // The host time of the instruction excludes its wait for the timing unit.
    state
        .profile
        .end_npu(funct7, profile_started.map(|started| started + waited), duration);

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
