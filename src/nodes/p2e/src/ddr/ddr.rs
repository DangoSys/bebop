use crate::ffi::{get_state, P2EState};

pub struct DdrBackdoor;

impl DdrBackdoor {
    pub fn load_image(addr: u64, data: &[u8]) -> Result<(), String> {
        let mut state = get_state().lock().unwrap();

        const DRAM_BASE: u64 = 0x80000000;
        if addr < DRAM_BASE {
            return Err(format!("Invalid DDR address: 0x{:x}", addr));
        }

        let offset = (addr - DRAM_BASE) as usize;
        if offset + data.len() > state.ddr_memory.len() {
            return Err("DDR write out of bounds".to_string());
        }

        state.ddr_memory[offset..offset + data.len()].copy_from_slice(data);
        log::info!("Loaded {} bytes to DDR at 0x{:x}", data.len(), addr);

        Ok(())
    }

    pub fn read_memory(addr: u64, len: usize) -> Result<Vec<u8>, String> {
        let state = get_state().lock().unwrap();

        const DRAM_BASE: u64 = 0x80000000;
        if addr < DRAM_BASE {
            return Err(format!("Invalid DDR address: 0x{:x}", addr));
        }

        let offset = (addr - DRAM_BASE) as usize;
        if offset + len > state.ddr_memory.len() {
            return Err("DDR read out of bounds".to_string());
        }

        Ok(state.ddr_memory[offset..offset + len].to_vec())
    }
}
