use std::sync::Mutex;
use std::os::raw::c_char;
use once_cell::sync::Lazy;
use bebop_elf::load_elf;

use crate::bank::{BankConfig, BankMap, BANK_NUM, BANK_SIZE};
use crate::inst;

const DRAM_BASE: u64 = 0x80000000;

static EMU_STATE: Lazy<Mutex<EmuState>> = Lazy::new(|| {
    Mutex::new(EmuState::new())
});

struct EmuState {
    memory: Vec<u8>,
    banks: Vec<Vec<u8>>,
    bank_cfgs: Vec<BankConfig>,
    bank_map: BankMap,
}

impl EmuState {
    fn new() -> Self {
        const MEM_SIZE: usize = 1 << 30; // 1GB
        Self {
            memory: vec![0; MEM_SIZE],
            banks: vec![vec![0; BANK_SIZE]; BANK_NUM],
            bank_cfgs: vec![BankConfig::default(); BANK_NUM],
            bank_map: BankMap::new(BANK_NUM),
        }
    }

    fn reset(&mut self) {
        self.memory.fill(0);
        for b in &mut self.banks {
            b.fill(0);
        }
        self.bank_cfgs.fill(BankConfig::default());
        self.bank_map = BankMap::new(BANK_NUM);
    }
}

#[no_mangle]
pub extern "C" fn buckyball_init() {
    let _guard = EMU_STATE.lock().unwrap();
}

#[no_mangle]
pub extern "C" fn buckyball_reset() {
    let mut state = EMU_STATE.lock().unwrap();
    state.reset();
}

#[no_mangle]
pub extern "C" fn buckyball_exec(funct7: u8, xs1: u64, xs2: u64) -> u64 {
    let mut state = EMU_STATE.lock().unwrap();
    let EmuState { memory, banks, bank_cfgs, bank_map } = &mut *state;

    inst::decode::execute_known(
        funct7 as u32,
        xs1,
        xs2,
        memory,
        banks,
        bank_cfgs,
        bank_map,
    ).unwrap_or_else(|| {
        panic!("unknown funct7: {}", funct7)
    })
}

extern "C" {
    fn spike_run_raw(
        isa: *const c_char,
        procs: usize,
        mem_ptr: *mut u8,
        mem_size: usize,
        entry: u64,
        log_path: *const c_char,
    ) -> i32;
}

pub fn run_spike(
    isa: &str,
    procs: usize,
    mem_mb: usize,
    elf_path: &str,
    log_path: Option<&str>,
) -> Result<(), String> {
    use std::ffi::CString;

    let mut state = EMU_STATE.lock().unwrap();

    // Load ELF into memory (Rust implementation)
    let entry = load_elf(elf_path, &mut state.memory, DRAM_BASE)?;

    let isa_c = CString::new(isa).map_err(|e| e.to_string())?;
    let log_c = log_path
        .map(|s| CString::new(s).map_err(|e| e.to_string()))
        .transpose()?;

    let ret = unsafe {
        spike_run_raw(
            isa_c.as_ptr(),
            procs,
            state.memory.as_mut_ptr(),
            state.memory.len(),
            entry,
            log_c.as_ref().map(|c| c.as_ptr()).unwrap_or(std::ptr::null()),
        )
    };

    if ret == 0 {
        Ok(())
    } else {
        Err(format!("spike exited with code {}", ret))
    }
}
