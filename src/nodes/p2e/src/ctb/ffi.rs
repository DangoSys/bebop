use std::collections::HashMap;
use std::ffi::CString;
use std::fs::File;
use std::io::Write;
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

        /// C wrapper: ctb_quit_wrapper(mgr)
        pub fn ctb_quit_wrapper(ctb: *mut ICtbMgr);

        /// C++: scu_0_hart_id() - exported from RTL
        pub fn scu_0_hart_id() -> u32;
    }
}

#[derive(Debug, Default)]
struct RuntimeState {
    initialized: bool,
    uart_log: Vec<u8>,
    exit_code: Option<i32>,
    uart_files: HashMap<u32, File>,
    log_dir: Option<String>,
}

static STATE: OnceLock<Mutex<RuntimeState>> = OnceLock::new();

fn state() -> &'static Mutex<RuntimeState> {
    STATE.get_or_init(|| Mutex::new(RuntimeState::default()))
}

pub fn reset_runtime_state() {
    *state().lock().unwrap() = RuntimeState::default();
}

pub fn set_log_dir(log_dir: String) {
    state().lock().unwrap().log_dir = Some(log_dir);
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
        // Drop the lock before file I/O to avoid holding it across syscalls
        drop(guard);
        // Create exit flag file for TCL to detect and stop its run loop
        // Path is relative to case_home (vdbg's CWD)
        let _ = std::fs::write("sim_exit.flag", format!("{}", data & 0xffff_ffff));
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

//===-----------------------------------------------------------------===//
// DPI-C functions exported by Rust and callable from generated RTL/C++.
//===-----------------------------------------------------------------===//

#[no_mangle]
pub extern "C" fn scu_uart_write(hart_id: u32, ch: u32) {
    let mut guard = state().lock().unwrap();

    // Get or create file handle for this hart
    if !guard.uart_files.contains_key(&hart_id) {
        let log_dir = guard
            .log_dir
            .clone()
            .expect("log_dir must be set via set_log_dir() before scu_uart_write is called");
        let log_path = format!("{}/uart_hart_{}.log", log_dir, hart_id);

        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&log_path)
        {
            guard.uart_files.insert(hart_id, file);
        }
    }

    // Write to per-hart file
    if let Some(file) = guard.uart_files.get_mut(&hart_id) {
        let byte = (ch & 0xff) as u8;
        let _ = file.write_all(&[byte]);
        let _ = file.flush();
    }

    // Also write to global uart_log for backward compatibility
    let byte = (ch & 0xff) as u8;
    guard.uart_log.push(byte);

    // Print to stdout with hart_id prefix
    print!("[hart{}] {}", hart_id, byte as char);
    let _ = std::io::Write::flush(&mut std::io::stdout());
}

#[no_mangle]
pub extern "C" fn scu_sim_exit(hart_id: u32, code: u32) {
    // CRITICAL: This function may be called during ctb_mgr->init()
    // Add defensive checks to prevent crashes

    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("/tmp/scu_uart_{hart_id}.log"))
    {
        let _ = writeln!(f, "[DPI-C] scu_sim_exit called: hart_id={}, code=0x{:x}", hart_id, code);
    }

    // Try to write to exit address, but don't crash if it fails
    // host_mmio_write handles both exit_code state and sim_exit.flag creation
    let _ = host_mmio_write(SIM_EXIT_ADDR, code as u64);
}

//===-----------------------------------------------------------------===//
// Safe-ish Rust wrapper around the opaque VVAC CTB manager.
//===-----------------------------------------------------------------===//

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
            // SAFETY: FFI call to VVAC C++ wrapper; returns opaque ICtbMgr pointer or null.
            // Null-checked before wrapping in Self. Ownership transferred to CtbManager.
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
        // SAFETY: FFI call to VVAC C++ wrapper; self.ctb is valid (set in new(), freed in Drop);
        // CString args outlive the FFI call. Returns bool indicating success.
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
            // SAFETY: self.ctb is valid (set in new(), freed in Drop); ctb_quit_wrapper
            // is the proper cleanup function for ICtbMgr.
            unsafe {
                raw::ctb_quit_wrapper(self.ctb);
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
