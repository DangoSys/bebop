use super::cycle_trace::CycleTraceCollector;
use bebop_uart::UartTx;
use std::collections::HashMap;
use std::ffi::CString;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::FileExt;
use std::sync::mpsc::Sender;
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
    uart_rx_read_files: HashMap<u32, File>,
    uart_rx_write_files: HashMap<u32, File>,
    uart_rx_offsets: HashMap<u32, u64>,
    uart_rx_peek: HashMap<u32, u8>,
    uart_rx_probe_counts: HashMap<u32, u32>,
    uart_rx_last_probe: HashMap<u32, (u64, u64, bool)>,
    console_tx: Option<Sender<UartTx>>,
    cycle_trace: Option<CycleTraceCollector>,
    cycle_trace_error: Option<String>,
}

static STATE: OnceLock<Mutex<RuntimeState>> = OnceLock::new();

fn state() -> &'static Mutex<RuntimeState> {
    STATE.get_or_init(|| Mutex::new(RuntimeState::default()))
}

pub fn reset_runtime_state() {
    *state().lock().unwrap() = RuntimeState::default();
}

pub fn set_log_dir(log_dir: String) {
    std::fs::create_dir_all(&log_dir).unwrap_or_else(|e| panic!("failed to create log directory {log_dir}: {e}"));

    for entry in std::fs::read_dir(&log_dir).unwrap_or_else(|e| panic!("failed to scan log directory {log_dir}: {e}")) {
        let entry = entry.unwrap_or_else(|e| panic!("failed to read log directory entry in {log_dir}: {e}"));
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("console_rx_hart_") && name.ends_with(".bin") {
            std::fs::remove_file(entry.path())
                .unwrap_or_else(|e| panic!("failed to remove stale console RX file {}: {e}", entry.path().display()));
        }
    }

    state().lock().unwrap().log_dir = Some(log_dir);
}

pub fn set_console_tx(tx: Sender<UartTx>) {
    state().lock().unwrap().console_tx = Some(tx);
}

pub fn init_cycle_trace(log_dir: &std::path::Path) -> Result<(), String> {
    state().lock().unwrap().cycle_trace = Some(CycleTraceCollector::new(log_dir)?);
    Ok(())
}

pub fn finish_cycle_trace() -> Result<(), String> {
    let (collector, error) = {
        let mut guard = state().lock().unwrap();
        (guard.cycle_trace.take(), guard.cycle_trace_error.take())
    };
    if let Some(error) = error {
        return Err(error);
    }
    if let Some(collector) = collector {
        collector.finish()?;
    }
    Ok(())
}

pub fn push_uart_rx(hart_id: u32, byte: u8) {
    let mut guard = state().lock().unwrap();
    let file = uart_rx_write_file(&mut guard, hart_id);
    file.write_all(&[byte])
        .unwrap_or_else(|e| panic!("failed to append console RX byte for hart {hart_id}: {e}"));
    file.flush()
        .unwrap_or_else(|e| panic!("failed to flush console RX byte for hart {hart_id}: {e}"));
    append_console_debug(&guard, format_args!("rx push hart={hart_id} byte=0x{byte:02x}\n"));
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
            if let Some(collector) = guard.cycle_trace.as_mut() {
                if let Err(error) = collector.push_uart_byte(0, byte) {
                    guard.cycle_trace_error.get_or_insert(error);
                }
            }
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
    if let Some(collector) = guard.cycle_trace.as_mut() {
        if let Err(error) = collector.push_uart_byte(hart_id, byte) {
            guard.cycle_trace_error.get_or_insert(error);
        }
    }
    if let Some(tx) = &guard.console_tx {
        let _ = tx.send(UartTx { hart_id, byte });
    }

    // Print to stdout with hart_id prefix
    print!("[hart{}] {}", hart_id, byte as char);
    let _ = std::io::Write::flush(&mut std::io::stdout());
}

#[no_mangle]
pub extern "C" fn scu_uart_rx_valid(hart_id: u32) -> i32 {
    let mut guard = state().lock().unwrap();
    ensure_uart_rx_peek(&mut guard, hart_id).is_some() as i32
}

#[no_mangle]
pub extern "C" fn scu_uart_rx_sample(hart_id: u32, pop: u32, valid: *mut u32, data: *mut u32) {
    if valid.is_null() || data.is_null() {
        panic!("scu_uart_rx_sample received null output pointer");
    }

    let mut guard = state().lock().unwrap();
    let byte = if pop != 0 {
        let byte = ensure_uart_rx_peek(&mut guard, hart_id);
        if let Some(byte) = byte {
            guard.uart_rx_peek.remove(&hart_id);
            *guard.uart_rx_offsets.entry(hart_id).or_insert(0) += 1;
            append_console_debug(&guard, format_args!("rx pop hart={hart_id} byte=0x{byte:02x}\n"));
        }
        ensure_uart_rx_peek(&mut guard, hart_id)
    } else {
        ensure_uart_rx_peek(&mut guard, hart_id)
    };

    set_uart_rx_sample(valid, data, byte);
}

fn set_uart_rx_sample(valid: *mut u32, data: *mut u32, byte: Option<u8>) {
    // SAFETY: callers validate both pointers are non-null before passing them
    // here; the C side provides writable output slots for one u32 each.
    unsafe {
        *valid = byte.is_some() as u32;
        *data = byte.unwrap_or(0) as u32;
    }
}

#[no_mangle]
pub extern "C" fn scu_uart_peek(hart_id: u32) -> i32 {
    let mut guard = state().lock().unwrap();
    ensure_uart_rx_peek(&mut guard, hart_id).unwrap_or(0) as i32
}

#[no_mangle]
pub extern "C" fn scu_uart_pop(hart_id: u32) -> i32 {
    let mut guard = state().lock().unwrap();
    let Some(byte) = ensure_uart_rx_peek(&mut guard, hart_id) else {
        return 0;
    };
    guard.uart_rx_peek.remove(&hart_id);
    *guard.uart_rx_offsets.entry(hart_id).or_insert(0) += 1;
    append_console_debug(&guard, format_args!("rx pop hart={hart_id} byte=0x{byte:02x}\n"));
    byte as i32
}

fn uart_rx_path(guard: &RuntimeState, hart_id: u32) -> String {
    let log_dir = guard
        .log_dir
        .as_ref()
        .expect("log_dir must be set via set_log_dir() before console RX is used");
    format!("{log_dir}/console_rx_hart_{hart_id}.bin")
}

fn uart_rx_write_file(guard: &mut RuntimeState, hart_id: u32) -> &mut File {
    if !guard.uart_rx_write_files.contains_key(&hart_id) {
        let path = uart_rx_path(guard, hart_id);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap_or_else(|e| panic!("failed to open console RX file {path} for append: {e}"));
        guard.uart_rx_write_files.insert(hart_id, file);
    }
    guard.uart_rx_write_files.get_mut(&hart_id).unwrap()
}

fn uart_rx_read_file(guard: &mut RuntimeState, hart_id: u32) -> &File {
    if !guard.uart_rx_read_files.contains_key(&hart_id) {
        let path = uart_rx_path(guard, hart_id);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .unwrap_or_else(|e| panic!("failed to open console RX file {path} for read: {e}"));
        guard.uart_rx_read_files.insert(hart_id, file);
    }
    guard.uart_rx_read_files.get(&hart_id).unwrap()
}

fn ensure_uart_rx_peek(guard: &mut RuntimeState, hart_id: u32) -> Option<u8> {
    if let Some(byte) = guard.uart_rx_peek.get(&hart_id).copied() {
        probe_uart_rx(guard, hart_id, true, Some(byte));
        return Some(byte);
    }

    let offset = *guard.uart_rx_offsets.entry(hart_id).or_insert(0);
    let file = uart_rx_read_file(guard, hart_id);
    let file_len = file
        .metadata()
        .unwrap_or_else(|e| panic!("failed to stat console RX file for hart {hart_id}: {e}"))
        .len();
    let mut byte = [0_u8; 1];
    match file.read_at(&mut byte, offset) {
        Ok(1) => {
            guard.uart_rx_peek.insert(hart_id, byte[0]);
            probe_uart_rx(guard, hart_id, true, Some(byte[0]));
            Some(byte[0])
        }
        Ok(0) => {
            probe_uart_rx_empty(guard, hart_id, offset, file_len);
            None
        }
        Ok(n) => panic!("unexpected short console RX read for hart {hart_id}: {n} bytes"),
        Err(e) => panic!("failed to read console RX byte for hart {hart_id}: {e}"),
    }
}

fn probe_uart_rx(guard: &mut RuntimeState, hart_id: u32, valid: bool, byte: Option<u8>) {
    let offset = *guard.uart_rx_offsets.entry(hart_id).or_insert(0);
    let path = uart_rx_path(guard, hart_id);
    let file_len = std::fs::metadata(&path)
        .unwrap_or_else(|e| panic!("failed to stat console RX file {path}: {e}"))
        .len();
    let last = guard.uart_rx_last_probe.insert(hart_id, (offset, file_len, valid));
    let count = guard.uart_rx_probe_counts.entry(hart_id).or_insert(0);
    let should_log = *count < 32 || last != Some((offset, file_len, valid));
    *count += 1;
    if should_log {
        match byte {
            Some(byte) => append_console_debug(
                guard,
                format_args!("rx valid hart={hart_id} offset={offset} len={file_len} byte=0x{byte:02x}\n"),
            ),
            None => append_console_debug(
                guard,
                format_args!("rx valid hart={hart_id} offset={offset} len={file_len}\n"),
            ),
        }
    }
}

fn probe_uart_rx_empty(guard: &mut RuntimeState, hart_id: u32, offset: u64, file_len: u64) {
    let last = guard.uart_rx_last_probe.insert(hart_id, (offset, file_len, false));
    let count = guard.uart_rx_probe_counts.entry(hart_id).or_insert(0);
    let should_log = *count < 8 || file_len > 0 || last != Some((offset, file_len, false));
    *count += 1;
    if should_log {
        append_console_debug(
            guard,
            format_args!("rx empty hart={hart_id} offset={offset} len={file_len}\n"),
        );
    }
}

fn append_console_debug(guard: &RuntimeState, args: std::fmt::Arguments<'_>) {
    let log_dir = guard
        .log_dir
        .as_ref()
        .expect("log_dir must be set via set_log_dir() before console debug is used");
    let path = format!("{log_dir}/p2e-console-debug.log");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap_or_else(|e| panic!("failed to open console debug log {path}: {e}"));
    file.write_fmt(args)
        .unwrap_or_else(|e| panic!("failed to write console debug log {path}: {e}"));
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
