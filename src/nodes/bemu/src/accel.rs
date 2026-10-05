use crate::inst::{FUNCT7_MSET, FUNCT7_MVIN_MMIO};
use crate::trace::with_trace_ptr;
use crate::{
    bank::*,
    inst,
    root::{mmu::GuestAccess, shared_memory, tile::Tile},
    trace::{TraceConfig, TraceState},
};
use bebop_bank_hash::{combine_bank_hash, BTraceBank};
use bebop_bemu_profile::BemuProfile;
use std::{path::Path, sync::{Arc, Mutex}};

pub(crate) struct PrivateState {
    pub(crate) banks: Vec<inst::instruction::PrivateBank>,
    pub(crate) bank_cfgs: Vec<BankConfig>,
    pub(crate) bank_map: BankMap,
    pub(crate) row_bytes: usize,
}

pub(crate) struct State {
    private: Arc<Mutex<PrivateState>>,
    pub(crate) rvv: Option<rvv::Engine>,
    pub(crate) shared_memory: Option<Arc<Tile>>,
    pub(crate) hart_id: usize,
    pub(crate) endpoint_index: Option<usize>,
    pub(crate) bank_scoreboard: inst::instruction::BankScoreboard,
    pub(crate) deferred_bank_frees: Vec<u32>,
    pub(crate) mmio_banks: Vec<Vec<u8>>,
    pub(crate) npu_inst_id: u64,
    pub(crate) trace: TraceState,
    pub(crate) profile: BemuProfile,
    pub(crate) barrier_hit: bool,
}

impl State {
    pub(crate) fn finish_bank_frees(&mut self) {
        let mut private = self.private.lock().expect("private banks poisoned");
        for bank_id in self.deferred_bank_frees.drain(..) {
            if crate::config::is_shared_vbank(bank_id as u64) {
                let mut shared = self
                    .shared_memory
                    .as_ref()
                    .expect("shared banks are unavailable")
                    .banks
                    .lock()
                    .expect("shared banks poisoned");
                shared.map.delete_hart_vbank(self.endpoint_index.expect("shared bank requires compute core"), bank_id);
                let core = self.endpoint_index.expect("shared bank requires compute core");
                let index = core * shared.virtual_bank_count + bank_id as usize;
                shared.cfgs[index] = BankConfig::default();
            } else {
                private.bank_map.delete_vbank(bank_id);
                private.bank_cfgs[bank_id as usize] = BankConfig::default();
            }
        }
    }

    pub(crate) fn new(
        log_dir: &Path,
        trace_config: TraceConfig,
        profile: bool,
        hart_id: usize,
        endpoint_index: Option<usize>,
        shared_memory: Option<Arc<Tile>>,
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
                .map(|_| inst::instruction::PrivateBank::new(bank_size(), btrace))
                .collect(),
            bank_cfgs: vec![BankConfig::default(); virtual_bank_num()],
            bank_map: BankMap::new(bank_num()),
            row_bytes: crate::config::bank_row_bytes(),
        }));
        if let Some(tile) = &shared_memory {
            if let Some(core) = endpoint_index {
                let previous = tile.private_endpoints.lock().expect("private endpoints poisoned")
                    .insert(core, Arc::clone(&private));
                assert!(previous.is_none(), "duplicate core bank endpoint");
            }
        }
        Ok(Self {
            private,
            rvv: crate::config::rvv().map(|config| {
                rvv::Engine::new(
                    config.v_len as usize,
                    config.e_len as usize,
                    config.i_buf_words as usize * 4,
                )
            }),
            shared_memory,
            hart_id,
            endpoint_index,
            bank_scoreboard: inst::instruction::BankScoreboard::new(),
            deferred_bank_frees: Vec::new(),
            mmio_banks: vec![vec![0; mmio_bank_size()]; mmio_bank_num()],
            npu_inst_id: 0,
            trace: TraceState::new(log_dir, trace_config).map_err(|e| e.to_string())?,
            profile: BemuProfile::new(profile),
            barrier_hit: false,
        })
    }
}

pub(crate) fn execute(state: &mut State, mut memory: GuestAccess<'_>, funct7: u8, xs1: u64, xs2: u64, pc: u64) -> Result<u64, rvsim::Trap> {
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
        && (funct7 == 15 || matches!(enable, 2..=4))
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

    if funct7 == 13 {
        let tile = state.shared_memory.as_ref().expect("mvover requires a Tile");
        tile.mvover(xs1, xs2).unwrap_or_else(|error| panic!("mvover: {error}"));
        state.profile.end_npu(funct7, profile_started);
        return Ok(0);
    }
    let mut private = state.private.lock().expect("private banks poisoned");
    let PrivateState { banks, bank_cfgs, bank_map, .. } = &mut *private;
    let State {
        rvv,
        shared_memory,
        hart_id,
        endpoint_index,
        bank_scoreboard,
        deferred_bank_frees,
        mmio_banks,
        barrier_hit,
        trace: _,
        ..
    } = state;

    let result = unsafe {
        with_trace_ptr(trace, || {
            let shared_range = crate::config::shared_vbank_base()..crate::config::virtual_bank_num();
            let accesses_shared = funct7 >= 16
                && [0, 10, 20].into_iter().any(|shift| {
                    let bank = ((xs1 >> shift) & 0x3ff) as usize;
                    bank > crate::config::private_vbank_upper_bound() && shared_range.contains(&bank)
                });
            let mut shared_state = shared_memory
                .as_ref()
                .filter(|_| accesses_shared)
                .map(|memory| memory.banks.lock().expect("shared banks poisoned"));
            let (tracked_banks, shared) = match shared_state.as_deref_mut() {
                Some(shared_memory::State {
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
                        local_core: endpoint_index.expect("shared bank requires compute core"),
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
                rvv,
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
        let w0_vbank = if funct7 == 15 {
            result as u32
        } else {
            ((xs1 >> 20) & 0x3ff) as u32
        };
        let mut status_hash = 0;
        let cols = if crate::config::is_shared_vbank(w0_vbank as u64) {
            let shared = shared_memory
                .as_ref()
                .expect("shared bank storage is unavailable")
                .banks
                .lock()
                .expect("shared banks poisoned");
            let core = endpoint_index.expect("shared bank requires compute core");
            shared.cfgs[core * shared.virtual_bank_count + w0_vbank as usize].cols
        } else {
            bank_cfgs[w0_vbank as usize].cols
        };
        for group_id in 0..cols as u32 {
            let physical_hash = if crate::config::is_shared_vbank(w0_vbank as u64) {
                let mut shared = shared_memory
                    .as_ref()
                    .expect("shared bank storage is unavailable")
                    .banks
                    .lock()
                    .expect("shared banks poisoned");
                let pbank_id = shared
                    .map
                    .resolve_hart_group(endpoint_index.expect("shared bank requires compute core"), w0_vbank, group_id)
                    .unwrap_or_else(|| panic!("unmapped shared vbank {w0_vbank} group {group_id}"));
                shared.storage[pbank_id].status_hash()
            } else {
                let pbank_id = bank_map
                    .resolve_group(w0_vbank, group_id)
                    .unwrap_or_else(|| panic!("unmapped vbank {w0_vbank} group {group_id}"));
                banks[pbank_id].status_hash()
            };
            status_hash = combine_bank_hash(status_hash, group_id, physical_hash);
        }
        unsafe {
            with_trace_ptr(trace, || {
                crate::trace::bemu_btrace(
                    inst_id,
                    *hart_id as u64,
                    BTraceBank {
                        owner_hart_id: if crate::config::is_shared_vbank(w0_vbank as u64) {
                            shared_memory.as_ref().expect("shared bank storage is unavailable")
                                .bank_owner_hart(*hart_id, true) as u64
                        } else { *hart_id as u64 },
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

fn preflight(state: &State, memory: &mut GuestAccess<'_>, funct: u8, xs1: u64, xs2: u64) -> Result<(), rvsim::Trap> {
    use inst::decode::{DmaRows, Mvin2dGeometry, rs1_b0, rs1_b2};
    use rvsim::Access;
    if funct == 12 {
        let bytes = xs1 as u32 as usize;
        assert!(xs1 >> 32 < 2 && bytes >= 24 && bytes % 4 == 0);
        return memory.check_range(xs2, bytes, Access::Load);
    }
    if !matches!(funct, 16 | 33 | 34) { return Ok(()); }
    let bank = if funct == 16 { rs1_b0(xs1) } else { rs1_b2(xs1) };
    let groups = if crate::config::is_shared_vbank(bank) {
        let tile = state.shared_memory.as_ref().expect("shared banks unavailable");
        let shared = tile.banks.lock().expect("shared banks poisoned");
        let cfg = &shared.cfgs[state.endpoint_index.expect("shared bank requires compute core") * shared.virtual_bank_count + bank as usize];
        assert!(cfg.allocated, "DMA bank not allocated");
        cfg.cols.max(1)
    } else {
        let private = state.private.lock().expect("private banks poisoned");
        let cfg = &private.bank_cfgs[bank as usize];
        assert!(cfg.allocated, "DMA bank not allocated");
        cfg.cols.max(1)
    };
    if funct == 34 {
        let geometry = Mvin2dGeometry::decode(xs1, xs2);
        for row in 0..geometry.height {
            for column in 0..geometry.width {
                memory.check_range(geometry.source(row, column), geometry.valid_bytes as usize, Access::Load)?;
            }
        }
    } else {
        let geometry = DmaRows::decode(xs1, xs2, groups);
        assert!(geometry.depth <= crate::bank::bank_size() as u64 / 16, "DMA exceeds bank");
        let access = if funct == 16 { Access::Store } else { Access::Load };
        if geometry.stride == 1 {
            memory.check_range(geometry.address, (geometry.depth * groups * 16) as usize, access)?;
        } else {
            for row in 0..geometry.depth {
                memory.check_range(geometry.source(row, 0), (groups * 16) as usize, access)?;
            }
        }
    }
    Ok(())
}
