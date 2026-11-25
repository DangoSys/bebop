//! BLink: what a Ball and the accelerator that hosts it see of each other.

use crate::{
    banks::{BankConfig, BankMap, SharedBankContext, TrackedBanks},
    operators,
};

pub type BallStates = std::collections::BTreeMap<(u32, std::any::TypeId), Box<dyn std::any::Any + Send>>;

/// The Ball bus as an accelerator drives it: a Ball instruction, by the class of its Ball.
#[derive(Clone, Copy)]
pub struct BallBus {
    pub execute: fn(&str, u32, u64, u64, &mut ExecContext) -> u64,
    pub cycles: fn(&str, u32, u64, u64, &ExecContext) -> u64,
}

/// BLink in the emulator: the context an instruction of the accelerator executes in. A Ball
/// model uses its ids, its cycle, `banks` and the methods; the other fields are the state of
/// the accelerator that hosts it.
pub struct ExecContext<'a> {
    /// The Ball bus of the accelerator.
    pub balls: BallBus,
    /// The accelerator's own instruction set, which an expanding Ball issues into.
    pub execute: fn(u32, u64, u64, &mut ExecContext) -> Option<u64>,
    pub ball_states: &'a mut BallStates,
    pub ball_id: Option<u32>,
    pub hart_id: usize,
    pub inst_id: u64,
    /// Hart cycle at which the instruction issues.
    pub cycle: u64,
    pub memory: crate::guest::GuestAccess<'a>,
    pub operator: &'a mut Option<operators::Engine>,
    pub banks: TrackedBanks<'a>,
    pub cfgs: &'a mut [BankConfig],
    pub bank_map: &'a mut BankMap,
    pub shared: Option<SharedBankContext<'a>>,
    /// Virtual banks released by a CISC instruction after its bank hash is
    /// sampled. Keeping the mapping alive until then preserves the logical
    /// identity of every physical bank written by the instruction.
    pub deferred_bank_frees: &'a mut Vec<u32>,
    pub mmio_banks: &'a mut [Vec<u8>],
    pub barrier_hit: &'a mut bool,
}

impl ExecContext<'_> {
    pub fn state<T: Default + Send + 'static>(&mut self) -> std::sync::Arc<std::sync::Mutex<T>> {
        let key = (
            self.ball_id.expect("Ball state requires an issued Ball command"),
            std::any::TypeId::of::<T>(),
        );
        self.ball_states
            .entry(key)
            .or_insert_with(|| Box::new(std::sync::Arc::new(std::sync::Mutex::new(T::default()))))
            .downcast_ref::<std::sync::Arc<std::sync::Mutex<T>>>()
            .expect("Ball state type")
            .clone()
    }
    pub fn metadata_word(&self, address: usize) -> u64 {
        let mut data = [0; 8];
        let banks = self.mmio_banks.len();
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = self.mmio_banks[(address + i) % banks][(address + i) / banks];
        }
        u64::from_le_bytes(data)
    }
    pub fn write_metadata_word(&mut self, address: usize, value: u64) {
        let banks = self.mmio_banks.len();
        for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.mmio_banks[(address + i) % banks][(address + i) / banks] = byte;
        }
    }

    pub fn read_memory(&mut self, addr: u64) -> u8 {
        self.memory.read(addr)
    }

    pub fn config(&self, bank_id: u64) -> &BankConfig {
        let index = usize::try_from(bank_id).expect("bank id exceeds usize");
        if crate::configs::is_shared_vbank(bank_id) {
            let shared = self.shared.as_ref().expect("shared bank storage is unavailable");
            &shared.cfgs[shared.local_thread * shared.virtual_bank_count + index]
        } else {
            &self.cfgs[index]
        }
    }

    pub fn config_mut(&mut self, bank_id: u64) -> &mut BankConfig {
        let index = usize::try_from(bank_id).expect("bank id exceeds usize");
        if crate::configs::is_shared_vbank(bank_id) {
            let shared = self.shared.as_mut().expect("shared bank storage is unavailable");
            let thread = shared.local_thread;
            &mut shared.cfgs[thread * shared.virtual_bank_count + index]
        } else {
            &mut self.cfgs[index]
        }
    }

    pub fn physical_bank(&self, bank_id: u64, group: u64) -> usize {
        if crate::configs::is_shared_vbank(bank_id) {
            let shared = self.shared.as_ref().expect("shared bank storage is unavailable");
            let physical = shared
                .bank_map
                .resolve_hart_group(shared.local_thread, bank_id as u32, group as u32)
                .unwrap_or_else(|| panic!("shared vbank {bank_id} group {group} not mapped"));
            self.banks.banks.len() + physical
        } else {
            self.bank_map
                .resolve_group(bank_id as u32, group as u32)
                .unwrap_or_else(|| panic!("vbank {bank_id} group {group} not mapped"))
        }
    }

    pub fn reported_physical_bank(&self, bank_id: u64, encoded: usize) -> u32 {
        if crate::configs::is_shared_vbank(bank_id) {
            (encoded - self.banks.banks.len()) as u32
        } else {
            encoded as u32
        }
    }

    pub fn defer_bank_free(&mut self, bank_id: u64) {
        let bank_id = u32::try_from(bank_id).expect("deferred bank id exceeds u32");
        assert!(
            self.config(bank_id as u64).allocated,
            "deferred free: bank {bank_id} is not allocated"
        );
        assert!(
            !self.deferred_bank_frees.contains(&bank_id),
            "deferred free: duplicate bank {bank_id}"
        );
        self.deferred_bank_frees.push(bank_id);
    }
}

/// An expanding Ball has its host execute the instructions it expands into.
pub fn execute_known(funct: u32, xs1: u64, xs2: u64, ctx: &mut ExecContext) -> Option<u64> {
    (ctx.execute)(funct, xs1, xs2, ctx)
}

// The Ball instruction: bank operands, iteration count and write mode in rs1 (`stack/isa/bb_isa.h`).

#[inline]
pub fn rs1_b0(xs1: u64) -> u64 {
    xs1 & 0x3ff
}

#[inline]
pub fn rs1_b1(xs1: u64) -> u64 {
    (xs1 >> 10) & 0x3ff
}

/// `BB_BANK2` - the 9-bit architectural destination, bits [28:20].
#[inline]
pub fn rs1_b2(xs1: u64) -> u64 {
    (xs1 >> 20) & 0x1ff
}

/// `BB_READ_WR_OLD` - bit 29: the instruction reads the old destination.
#[inline]
pub fn rs1_read_old(xs1: u64) -> bool {
    (xs1 >> 29) & 1 != 0
}

/// `BB_ITER` - bits [62:30].
#[inline]
pub fn rs1_iter(xs1: u64) -> u64 {
    (xs1 >> 30) & ((1 << 33) - 1)
}

/// `BB_UPDATE_WR` - bit 63: the destination is updated in place.
#[inline]
pub fn rs1_update(xs1: u64) -> bool {
    xs1 >> 63 != 0
}

/// the bank field in the instruction is **vbank_id**; parse it to physical slot index before accessing `banks`.
#[inline]
pub fn pbank(ctx: &ExecContext, vbank: u64) -> usize {
    pbank_group(ctx, vbank, 0)
}

#[inline]
pub fn pbank_group(ctx: &ExecContext, vbank: u64, group: u64) -> usize {
    ctx.physical_bank(vbank, group)
}
