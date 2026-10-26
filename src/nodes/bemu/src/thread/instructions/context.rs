use crate::{
    ball::BallStates,
    thread::{
        banks::{BankConfig, BankMap, SharedBankContext, TrackedBanks},
        kernels,
    },
};

/// Execution context passed to all instructions

pub struct ExecContext<'a> {
    pub ball_states: &'a mut BallStates,
    pub ball_id: Option<u32>,
    pub hart_id: usize,
    pub inst_id: u64,
    pub(crate) memory: crate::core::memory::GuestAccess<'a>,
    pub(crate) kernel: &'a mut Option<kernels::Engine>,
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

/// Instruction trait - all instructions must implement this
pub trait Instruction {
    /// Instruction opcode (funct7 field)
    const FUNCT: u32;

    /// Execute the instruction, return result value
    fn exec(xs1: u64, xs2: u64, ctx: &mut ExecContext) -> u64;
    /// Local timing contract; callers must model overlap and contention separately.
    fn latency(xs1: u64, xs2: u64) -> u64;
}
