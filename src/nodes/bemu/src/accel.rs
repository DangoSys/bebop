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
use std::{path::Path, sync::Arc};

pub(crate) struct State {
    pub(crate) banks: Vec<inst::instruction::PrivateBank>,
    pub(crate) bank_cfgs: Vec<BankConfig>,
    pub(crate) bank_map: BankMap,
    pub(crate) shared_memory: Option<Arc<Tile>>,
    pub(crate) hart_id: usize,
    pub(crate) bank_scoreboard: inst::instruction::BankScoreboard,
    pub(crate) deferred_bank_frees: Vec<u32>,
    pub(crate) mmio_banks: Vec<Vec<u8>>,
    pub(crate) total_lat: u64,
    pub(crate) npu_inst_id: u64,
    pub(crate) trace: TraceState,
    pub(crate) profile: BemuProfile,
    pub(crate) barrier_hit: bool,
}

impl State {
    pub(crate) fn finish_bank_frees(&mut self) {
        for bank_id in self.deferred_bank_frees.drain(..) {
            if crate::config::is_shared_vbank(bank_id as u64) {
                let mut shared = self
                    .shared_memory
                    .as_ref()
                    .expect("shared banks are unavailable")
                    .banks
                    .lock()
                    .expect("shared banks poisoned");
                shared.map.delete_hart_vbank(self.hart_id, bank_id);
                let core = self.hart_id % (shared.cfgs.len() / shared.virtual_bank_count);
                let index = core * shared.virtual_bank_count + bank_id as usize;
                shared.cfgs[index] = BankConfig::default();
            } else {
                self.bank_map.delete_vbank(bank_id);
                self.bank_cfgs[bank_id as usize] = BankConfig::default();
            }
        }
    }

    pub(crate) fn new(
        log_dir: &Path,
        trace_config: TraceConfig,
        profile: bool,
        hart_id: usize,
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
        Ok(Self {
            banks: (0..bank_num())
                .map(|_| inst::instruction::PrivateBank::new(bank_size(), btrace))
                .collect(),
            bank_cfgs: vec![BankConfig::default(); virtual_bank_num()],
            bank_map: BankMap::new(bank_num()),
            shared_memory,
            hart_id,
            bank_scoreboard: inst::instruction::BankScoreboard::new(),
            deferred_bank_frees: Vec::new(),
            mmio_banks: vec![vec![0; mmio_bank_size()]; mmio_bank_num()],
            total_lat: 0,
            npu_inst_id: 0,
            trace: TraceState::new(log_dir, trace_config).map_err(|e| e.to_string())?,
            profile: BemuProfile::new(profile),
            barrier_hit: false,
        })
    }
}

pub(crate) fn execute(state: &mut State, memory: GuestAccess<'_>, funct7: u8, xs1: u64, xs2: u64, pc: u64) -> u64 {
    state.barrier_hit = false;
    let profile_started = state.profile.begin_npu();
    let lat = inst::decode::cycles_after_issue(funct7 as u32, xs1, xs2);
    state.total_lat += lat;
    state.trace.set_bemu_clk(state.total_lat);
    if !matches!(funct7, 0 | 1) {
        state.npu_inst_id = state.npu_inst_id.wrapping_add(1);
    }
    let inst_id = state.npu_inst_id;
    let trace = &mut state.trace as *mut TraceState;
    let enable = funct7 >> 4;
    let btrace = state.trace.btrace_enabled()
        && pc != 0
        && matches!(enable, 2..=4)
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

    let State {
        banks,
        bank_cfgs,
        bank_map,
        shared_memory,
        hart_id,
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
            let accesses_shared = !matches!(funct7, 0 | 1)
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
        let w0_vbank = ((xs1 >> 20) & 0x3ff) as u32;
        let mut status_hash = 0;
        let cols = if crate::config::is_shared_vbank(w0_vbank as u64) {
            let shared = shared_memory
                .as_ref()
                .expect("shared bank storage is unavailable")
                .banks
                .lock()
                .expect("shared banks poisoned");
            let core = *hart_id % (shared.cfgs.len() / shared.virtual_bank_count);
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
                    .resolve_hart_group(*hart_id, w0_vbank, group_id)
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
    state.finish_bank_frees();
    state.profile.end_npu(funct7, profile_started);

    result
}
