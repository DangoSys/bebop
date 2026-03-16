/// BEMU C API 导出
/// 
/// 这些函数导出为 C 兼容接口，供 Spike (C++) 调用
/// 使用 #[no_mangle] 确保函数名不被 Rust 编译器修饰

use super::spike_interface::{BemuSpikeInterface, SpikeCallbackParams, SpikeCallbacks};
use log::{error, info};

/// C API: 创建 BEMU 接口
#[no_mangle]
pub unsafe extern "C" fn bemu_create_interface(verbose: bool) -> *mut BemuSpikeInterface {
    info!("Creating BEMU Spike interface (verbose={})", verbose);
    let interface = Box::new(BemuSpikeInterface::with_verbose(verbose));
    Box::into_raw(interface)
}

/// C API: 释放 BEMU 接口
#[no_mangle]
pub unsafe extern "C" fn bemu_free_interface(interface: *mut BemuSpikeInterface) {
    if !interface.is_null() {
        info!("Freeing BEMU Spike interface");
        let _ = Box::from_raw(interface);
    }
}

/// C API: 处理自定义指令
#[no_mangle]
pub unsafe extern "C" fn bemu_handle_custom(
    interface: *mut BemuSpikeInterface,
    funct: u32,
    xs1: u64,
    xs2: u64,
    result: *mut u64,
) -> i32 {
    if interface.is_null() || result.is_null() {
        error!("Null pointer passed to bemu_handle_custom");
        return -1;
    }
    
    let interface = &mut *interface;
    let params = SpikeCallbackParams::new(funct, xs1, xs2);
    
    match interface.handle_custom_instruction(&params) {
        Ok(res) => {
            *result = res;
            0 // 成功
        }
        Err(e) => {
            error!("Spike callback error: {:?}", e);
            -2 // 错误码
        }
    }
}

/// C API: 同步内存
#[no_mangle]
pub unsafe extern "C" fn bemu_sync_memory(
    interface: *mut BemuSpikeInterface,
    addr: u64,
    data: *const u8,
    size: usize,
) -> i32 {
    if interface.is_null() || data.is_null() {
        error!("Null pointer passed to bemu_sync_memory");
        return -1;
    }
    
    let interface = &mut *interface;
    let slice = std::slice::from_raw_parts(data, size);
    
    match interface.sync_memory(addr, slice) {
        Ok(()) => 0,
        Err(e) => {
            error!("Memory sync error: {:?}", e);
            -3
        }
    }
}

/// C API: 读取内存
#[no_mangle]
pub unsafe extern "C" fn bemu_read_memory(
    interface: *mut BemuSpikeInterface,
    addr: u64,
    data: *mut u8,
    size: usize,
) -> i32 {
    if interface.is_null() || data.is_null() {
        error!("Null pointer passed to bemu_read_memory");
        return -1;
    }
    
    let interface = &*interface;
    
    match interface.read_memory(addr, size) {
        Ok(bytes) => {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len().min(size));
            0
        }
        Err(e) => {
            error!("Memory read error: {:?}", e);
            -3
        }
    }
}

/// C API: 获取版本
#[no_mangle]
pub unsafe extern "C" fn bemu_get_version() -> *const i8 {
    const VERSION: &str = "0.1.0\0";
    VERSION.as_ptr() as *const i8
}

/// C API: 获取指令计数
#[no_mangle]
pub unsafe extern "C" fn bemu_get_instruction_count(interface: *mut BemuSpikeInterface) -> u64 {
    if interface.is_null() {
        return 0;
    }
    let interface = &*interface;
    interface.get_stats().instructions_executed
}

/// C API: 重置统计
#[no_mangle]
pub unsafe extern "C" fn bemu_reset_stats(interface: *mut BemuSpikeInterface) {
    if !interface.is_null() {
        let interface = &mut *interface;
        interface.reset_stats();
    }
}
