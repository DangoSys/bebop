use std::ffi::CString;
use std::sync::{Mutex, OnceLock};

const SIM_EXIT_ADDR: u64 = 0x6000_0000;
const UART_BASE_ADDR: u64 = 0x6002_0000;
const UART_SIZE: u64 = 8;

#[repr(C)]
pub struct ICtbMgr {
    _private: [u8; 0],
}

#[cfg(vvac_linked)]
mod raw {
    use super::ICtbMgr;
    use std::os::raw::c_char;

    extern "C" {
        /// C wrapper: ctb_builder_create_wrapper()
        pub fn ctb_builder_create_wrapper() -> *mut ICtbMgr;

        /// C wrapper: ctb_init_wrapper(mgr, fpga_id, case_home, rtcfg_path)
        pub fn ctb_init_wrapper(
            ctb: *mut ICtbMgr,
            fpga_id: *const c_char,
            case_home: *const c_char,
            rtcfg_path: *const c_char,
        ) -> bool;

        /// C++: ctb::ctbMgr::quit()
        #[link_name = "_ZN3ctb6ctbMgr4quitEv"]
        pub fn ctb_quit(ctb: *mut ICtbMgr);

        /// C++: scu_0_hart_id() - exported from RTL
        pub fn scu_0_hart_id() -> u32;
    }
}

#[derive(Debug, Default)]
struct RuntimeState {
    initialized: bool,
    uart_log: Vec<u8>,
    exit_code: Option<i32>,
}

static STATE: OnceLock<Mutex<RuntimeState>> = OnceLock::new();

fn state() -> &'static Mutex<RuntimeState> {
    STATE.get_or_init(|| Mutex::new(RuntimeState::default()))
}

pub fn reset_runtime_state() {
    *state().lock().unwrap() = RuntimeState::default();
}

pub fn mark_initialized() {
    state().lock().unwrap().initialized = true;
}

pub fn check_exit() -> bool {
    state().lock().unwrap().exit_code.is_some()
}

pub fn exit_code() -> i32 {
    state().lock().unwrap().exit_code.unwrap_or(0)
}

pub fn uart_log() -> String {
    let guard = state().lock().unwrap();
    String::from_utf8_lossy(&guard.uart_log).to_string()
}

pub fn host_mmio_write(addr: u64, data: u64) -> i32 {
    let mut guard = state().lock().unwrap();

    if addr == SIM_EXIT_ADDR {
        guard.exit_code = Some((data & 0xffff_ffff) as i32);
        return 0;
    }

    if (UART_BASE_ADDR..UART_BASE_ADDR + UART_SIZE).contains(&addr) {
        if addr == UART_BASE_ADDR {
            let byte = (data & 0xff) as u8;
            guard.uart_log.push(byte);
            print!("{}", byte as char);
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        return 0;
    }

    0
}

// ============================================================================
// DPI-C functions exported by Rust and callable from generated RTL/C++.
// ============================================================================

#[no_mangle]
pub extern "C" fn scu_uart_write(_hart_id: u32, ch: u32, ack: *mut u8) {
    // 无条件写入文件，用于验证函数是否被调用
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/scu_uart_debug.log")
    {
        let _ = writeln!(f, "scu_uart_write called: hart_id={}, ch=0x{:x}", _hart_id, ch);
    }

    log::debug!("scu_uart_write: hart_id = {}, ch = 0x{:x}", _hart_id, ch);
    let _ = host_mmio_write(UART_BASE_ADDR, (ch & 0xff) as u64);
    // 给出响应，让 FPGA 继续运行
    if !ack.is_null() {
        unsafe {
            *ack = 1;
        }
    }
}

#[no_mangle]
pub extern "C" fn scu_sim_exit(_hart_id: u32, code: u32, ack: *mut u8) {
    log::debug!("scu_sim_exit: hart_id = {}, code = 0x{:x}", _hart_id, code);
    let _ = host_mmio_write(SIM_EXIT_ADDR, code as u64);
    // 给出响应
    if !ack.is_null() {
        unsafe {
            *ack = 1;
        }
    }
}

// ============================================================================
// Safe-ish Rust wrapper around the opaque VVAC CTB manager.
// ============================================================================

pub struct CtbManager {
    ctb: *mut ICtbMgr,
}

impl CtbManager {
    pub fn new() -> Result<Self, String> {
        #[cfg(not(vvac_linked))]
        {
            Err("VVAC is not linked; generate out/vvacDir/runtimeDir/lib/lib_arm/libvCtb.so and rebuild before running P2E".to_string())
        }

        #[cfg(vvac_linked)]
        {
            let ctb = unsafe { raw::ctb_builder_create_wrapper() };
            if ctb.is_null() {
                return Err("failed to create ICtbMgr".to_string());
            }
            Ok(Self { ctb })
        }
    }

    pub fn init(&self, fpga_id: &str, case_home: &str, rtcfg_path: &str) -> Result<(), String> {
        let fpga_id_c = CString::new(fpga_id).map_err(|e| e.to_string())?;
        let case_home_c = CString::new(case_home).map_err(|e| e.to_string())?;
        let rtcfg_path_c = CString::new(rtcfg_path).map_err(|e| e.to_string())?;

        #[cfg(vvac_linked)]
        let success = unsafe {
            raw::ctb_init_wrapper(
                self.ctb,
                fpga_id_c.as_ptr(),
                case_home_c.as_ptr(),
                rtcfg_path_c.as_ptr(),
            )
        };

        #[cfg(not(vvac_linked))]
        let success = {
            let _ = (fpga_id_c, case_home_c, rtcfg_path_c);
            false
        };

        if success {
            log::info!("CTB initialized successfully");
            Ok(())
        } else {
            Err("CTB initialization failed".to_string())
        }
    }

    pub fn quit(&self) {
        if !self.ctb.is_null() {
            #[cfg(vvac_linked)]
            unsafe {
                raw::ctb_quit(self.ctb);
            }
            log::info!("CTB quit");
        }
    }
}

impl Drop for CtbManager {
    fn drop(&mut self) {
        self.quit();
    }
}
