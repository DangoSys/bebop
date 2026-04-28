use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

// ============================================================================
// DPI-C 函数（RTL 导出，Rust 调用）
// ============================================================================

extern "C" {
    /// 等待 N 个时钟周期
    pub fn waitNCycles(n: u32);
}

// ============================================================================
// ICtbMgr C++ 接口（libvCtb.so）
// ============================================================================

#[repr(C)]
pub struct ICtbMgr {
    _private: [u8; 0],
}

extern "C" {
    /// 创建 ICtbMgr 实例
    /// C++: vvac::CtbBuilder::create()
    #[link_name = "_ZN4vvac10CtbBuilder6createEv"]
    pub fn ctb_builder_create() -> *mut ICtbMgr;

    /// 初始化 CTB
    /// C++: ICtbMgr::init(fpga_id, case_home, rtcfg_path)
    /// 返回: true=成功, false=失败
    #[link_name = "_ZN4vvac8ICtbMgr4initEPKcS2_S2_"]
    pub fn ctb_init(
        ctb: *mut ICtbMgr,
        fpga_id: *const c_char,
        case_home: *const c_char,
        rtcfg_path: *const c_char,
    ) -> bool;

    /// 退出 CTB
    /// C++: ICtbMgr::quit()
    #[link_name = "_ZN4vvac8ICtbMgr4quitEv"]
    pub fn ctb_quit(ctb: *mut ICtbMgr);
}

// ============================================================================
// DPI-C 导出函数（Rust 导出，RTL 调用）
// ============================================================================

/// SCU MMIO 写入
#[no_mangle]
pub extern "C" fn scu_mmio_write(addr: u32, data: u32) -> i32 {
    // 实现在 mmio/scu.rs 中
    0
}

/// SCU MMIO 读取
#[no_mangle]
pub extern "C" fn scu_mmio_read(addr: u32) -> u32 {
    // 实现在 mmio/scu.rs 中
    0
}

/// P2E 初始化
#[no_mangle]
pub extern "C" fn p2e_init() {
    log::info!("P2E DPI-C initialized");
}

/// DDR backdoor 写入（通过 TCL，这里只是占位）
#[no_mangle]
pub extern "C" fn p2e_ddr_backdoor_write(addr: u64, data: *const u8, len: usize) {
    log::warn!("DDR backdoor write via DPI-C is not supported, use TCL memory command");
}

// ============================================================================
// Rust 封装
// ============================================================================

pub struct CtbManager {
    ctb: *mut ICtbMgr,
}

impl CtbManager {
    pub fn new() -> Result<Self, String> {
        let ctb = unsafe { ctb_builder_create() };
        if ctb.is_null() {
            return Err("Failed to create ICtbMgr".to_string());
        }
        Ok(Self { ctb })
    }

    pub fn init(&self, fpga_id: &str, case_home: &str, rtcfg_path: &str) -> Result<(), String> {
        let fpga_id_c = CString::new(fpga_id).map_err(|e| e.to_string())?;
        let case_home_c = CString::new(case_home).map_err(|e| e.to_string())?;
        let rtcfg_path_c = CString::new(rtcfg_path).map_err(|e| e.to_string())?;

        let success = unsafe {
            ctb_init(
                self.ctb,
                fpga_id_c.as_ptr(),
                case_home_c.as_ptr(),
                rtcfg_path_c.as_ptr(),
            )
        };

        if success {
            log::info!("CTB initialized successfully");
            Ok(())
        } else {
            Err("CTB initialization failed".to_string())
        }
    }

    pub fn quit(&self) {
        unsafe {
            ctb_quit(self.ctb);
        }
        log::info!("CTB quit");
    }
}

impl Drop for CtbManager {
    fn drop(&mut self) {
        self.quit();
    }
