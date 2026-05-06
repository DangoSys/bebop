use crate::ffi::{self, get_state};
use crate::ddr::DdrBackdoor;
use crate::scu::ScuController;

pub struct P2ESimulator;

impl P2ESimulator {
    pub fn new() -> Result<Self, String> {
        unsafe { ffi::p2e_init(); }
        log::info!("P2E Simulator created");
        Ok(Self)
    }

    pub fn load_image(&self, addr: u64, data: &[u8]) -> Result<(), String> {
        DdrBackdoor::load_image(addr, data)
    }

    pub fn reset(&self) -> Result<(), String> {
        let mut state = get_state().lock().unwrap();
        state.reset();
        log::info!("Simulator reset");
        Ok(())
    }

    pub fn step(&self, cycles: u32) -> Result<(), String> {
        unsafe {
            ffi::waitNCycles(cycles);
        }
        Ok(())
    }

    pub fn scu_write(&self, addr: u32, data: u32) -> Result<(), String> {
        ScuController::write(addr, data)
    }

    pub fn scu_read(&self, addr: u32) -> u32 {
        ScuController::read(addr)
    }

    pub fn run_until_exit(&self) -> Result<i32, String> {
        loop {
            self.step(100)?;

            let exit_flag = unsafe { ffi::check_sim_exit() };
            if exit_flag != 0 {
                let code = unsafe { ffi::get_exit_code() };
                log::info!("Simulation exited with code {}", code);
                return Ok(code);
            }
        }
    }

    pub fn check_exit(&self) -> bool {
        unsafe { ffi::check_sim_exit() != 0 }
    }

    pub fn get_exit_code(&self) -> i32 {
        unsafe { ffi::get_exit_code() }
    }
}
