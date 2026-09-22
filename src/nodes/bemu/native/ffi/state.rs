use super::pk::PkVm;
use super::*;

pub struct SharedMemory {
    data: std::cell::UnsafeCell<Vec<u8>>,
    banks: std::cell::UnsafeCell<SharedBankState>,
    barrier: TileBarrier,
}

pub(super) struct SharedBankState {
    pub(super) storage: Vec<inst::instruction::PrivateBank>,
    pub(super) cfgs: Vec<BankConfig>,
    pub(super) map: BankMap,
    pub(super) virtual_bank_count: usize,
}

unsafe impl Send for SharedMemory {}
unsafe impl Sync for SharedMemory {}

impl SharedMemory {
    pub fn new(
        size: usize,
        core_count: usize,
        shared_physical_bank_count: usize,
        shared_bank_size: usize,
        virtual_bank_count: usize,
    ) -> Arc<Self> {
        Arc::new(Self {
            data: std::cell::UnsafeCell::new(vec![0; size]),
            banks: std::cell::UnsafeCell::new(SharedBankState {
                storage: (0..shared_physical_bank_count)
                    .map(|_| inst::instruction::PrivateBank::new(shared_bank_size, false))
                    .collect(),
                cfgs: vec![BankConfig::default(); core_count * virtual_bank_count],
                map: BankMap::new(shared_physical_bank_count),
                virtual_bank_count,
            }),
            barrier: TileBarrier::new(core_count),
        })
    }

    pub(super) fn as_slice(&self) -> &[u8] {
        unsafe { &*self.data.get() }
    }

    pub(super) fn as_mut_slice(&self) -> &mut [u8] {
        unsafe { &mut *self.data.get() }
    }

    pub(super) fn banks_mut(&self) -> &mut SharedBankState {
        unsafe { &mut *self.banks.get() }
    }

    pub fn wait_barrier(&self, hart_id: usize) {
        self.barrier.wait(hart_id);
    }

    pub fn abort_barrier(&self) {
        self.barrier.abort();
    }
}

struct TileBarrier {
    core_count: usize,
    state: Mutex<(u64, Vec<usize>, bool)>,
    ready: std::sync::Condvar,
}

impl TileBarrier {
    fn new(core_count: usize) -> Self {
        Self {
            core_count,
            state: Mutex::new((0, Vec::with_capacity(core_count), false)),
            ready: std::sync::Condvar::new(),
        }
    }

    fn wait(&self, hart_id: usize) {
        let mut state = self.state.lock().expect("BEMU barrier poisoned");
        let epoch = state.0;
        if state.2 {
            return;
        }
        if !state.1.contains(&hart_id) {
            state.1.push(hart_id);
        }
        if state.1.len() == self.core_count {
            state.0 = state.0.wrapping_add(1);
            state.1.clear();
            self.ready.notify_all();
            return;
        }
        while state.0 == epoch && !state.2 {
            state = self.ready.wait(state).expect("BEMU barrier poisoned");
        }
    }

    fn abort(&self) {
        let mut state = self.state.lock().expect("BEMU barrier poisoned");
        state.2 = true;
        self.ready.notify_all();
    }
}

pub(super) enum GuestMemory {
    Owned(Vec<u8>),
    Shared(Arc<SharedMemory>),
}

impl GuestMemory {
    pub(super) fn as_mut_ptr(&mut self) -> *mut u8 {
        match self {
            Self::Owned(memory) => memory.as_mut_ptr(),
            Self::Shared(memory) => memory.as_mut_slice().as_mut_ptr(),
        }
    }

    pub(super) fn len(&self) -> usize {
        match self {
            Self::Owned(memory) => memory.len(),
            Self::Shared(memory) => memory.as_slice().len(),
        }
    }
}

impl std::ops::Deref for GuestMemory {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(memory) => memory,
            Self::Shared(memory) => memory.as_slice(),
        }
    }
}

impl std::ops::DerefMut for GuestMemory {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Owned(memory) => memory,
            Self::Shared(memory) => memory.as_mut_slice(),
        }
    }
}

pub(super) struct EmuState {
    pub(super) memory: GuestMemory,
    pub(super) banks: Vec<inst::instruction::PrivateBank>,
    pub(super) bank_cfgs: Vec<BankConfig>,
    pub(super) bank_map: BankMap,
    pub(super) shared_memory: Option<Arc<SharedMemory>>,
    pub(super) hart_id: usize,
    pub(super) bank_scoreboard: inst::instruction::BankScoreboard,
    pub(super) deferred_bank_frees: Vec<u32>,
    pub(super) mmio_banks: Vec<Vec<u8>>,
    pub(super) total_lat: u64,
    pub(super) npu_inst_id: u64,
    pub(super) matrix_instruction_count: u64,
    pub(super) uart: Uart,
    pub(super) clint: Clint,
    pub(super) plic: Plic,
    pub(super) syscall: SyscallState,
    pub(super) pk_vm: Option<PkVm>,
    pub(super) trace: TraceState,
    pub(super) profile: BemuProfile,
    pub(super) barrier_hit: bool,
}

impl EmuState {
    pub(super) fn new(
        log_dir: &Path,
        trace_config: TraceConfig,
        profile: bool,
        hart_id: usize,
        shared_memory: Option<Arc<SharedMemory>>,
    ) -> Result<Self, String> {
        // 1GB Here is important, for baremetal mode, when we set this to 4GB,
        // it will running for a long time.
        const MEM_SIZE: usize = 3 * (1 << 30);
        let btrace = trace_config.btrace;
        if btrace {
            if let Some(shared) = &shared_memory {
                for bank in &mut shared.banks_mut().storage {
                    bank.enable_hash();
                }
            }
        }
        Ok(Self {
            // memory is maintained by bemu not spike
            memory: shared_memory
                .clone()
                .map_or_else(|| GuestMemory::Owned(vec![0; MEM_SIZE]), GuestMemory::Shared),
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
            matrix_instruction_count: 0,
            uart: Uart::new(),
            clint: Clint::new(),
            plic: Plic::default(),
            syscall: SyscallState::new(),
            pk_vm: None,
            trace: TraceState::new(log_dir, trace_config).map_err(|e| e.to_string())?,
            profile: BemuProfile::new(profile),
            barrier_hit: false,
        })
    }

    pub(super) fn reset_accel(&mut self) {
        for b in &mut self.banks {
            b.reset();
        }
        self.bank_cfgs.fill(BankConfig::default());
        self.bank_map = BankMap::new(bank_num());
        self.bank_scoreboard.reset();
        self.deferred_bank_frees.clear();
        for bank in &mut self.mmio_banks {
            bank.fill(0);
        }
        self.total_lat = 0;
        self.npu_inst_id = 0;
        self.matrix_instruction_count = 0;
    }

    // rushB never exposes guest DRAM. DMA commands use the host pointers
    // supplied by the native lowering, so allocating BEMU's 1 GiB guest RAM
    // would only add startup cost.
    pub(super) fn new_host() -> Self {
        Self {
            memory: GuestMemory::Owned(Vec::new()),
            banks: (0..bank_num())
                .map(|_| inst::instruction::PrivateBank::new(bank_size(), false))
                .collect(),
            bank_cfgs: vec![BankConfig::default(); virtual_bank_num()],
            bank_map: BankMap::new(bank_num()),
            shared_memory: None,
            hart_id: 0,
            bank_scoreboard: inst::instruction::BankScoreboard::new(),
            deferred_bank_frees: Vec::new(),
            mmio_banks: vec![vec![0; mmio_bank_size()]; mmio_bank_num()],
            total_lat: 0,
            npu_inst_id: 0,
            matrix_instruction_count: 0,
            uart: Uart::new(),
            clint: Clint::new(),
            plic: Plic::default(),
            syscall: SyscallState::new(),
            pk_vm: None,
            trace: TraceState::default(),
            profile: BemuProfile::new(false),
            barrier_hit: false,
        }
    }
}
