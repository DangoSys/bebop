// FFI bindings to minimal C++ Verilator wrapper

use std::os::raw::{c_char, c_int};

#[repr(C)]
pub struct VerilatorContext {
    _private: [u8; 0],
}

#[repr(C)]
pub struct VerilatorTop {
    _private: [u8; 0],
}

#[repr(C)]
pub struct VerilatorTrace {
    _private: [u8; 0],
}

extern "C" {
    // Verilator context management
    pub fn verilator_context_new() -> *mut VerilatorContext;
    pub fn verilator_context_free(ctx: *mut VerilatorContext);
    pub fn verilator_context_time_inc(ctx: *mut VerilatorContext, add: u64);
    pub fn verilator_context_time(ctx: *mut VerilatorContext) -> u64;
    pub fn verilator_context_command_args(ctx: *mut VerilatorContext, argc: c_int, argv: *const *const c_char);
    pub fn verilator_context_trace_ever_on(ctx: *mut VerilatorContext, on: bool);

    // Coverage
    pub fn verilator_context_coverage_write(ctx: *mut VerilatorContext);

    // Top module
    pub fn verilator_top_new(ctx: *mut VerilatorContext) -> *mut VerilatorTop;
    pub fn verilator_top_free(top: *mut VerilatorTop);
    pub fn verilator_top_eval(top: *mut VerilatorTop);
    pub fn verilator_top_trace(top: *mut VerilatorTop, tfp: *mut VerilatorTrace, levels: c_int);

    // Top module signals
    pub fn verilator_top_set_clock(top: *mut VerilatorTop, val: u8);
    pub fn verilator_top_set_reset(top: *mut VerilatorTop, val: u8);
    pub fn verilator_top_get_clock(top: *mut VerilatorTop) -> u8;
    pub fn verilator_top_get_reset(top: *mut VerilatorTop) -> u8;

    // SCU state query (DPI-C functions are called from RTL automatically)
    pub fn verilator_scu_has_exit() -> bool;
    pub fn verilator_scu_exit_code() -> i32;
    pub fn verilator_scu_reset();
    pub fn verilator_scu_push_uart_rx(hart_id: u32, byte: u32);
    pub fn verilator_scu_drain_uart_tx(buf: *mut u32, len: u32) -> u32;

    // FST trace
    pub fn verilator_trace_new() -> *mut VerilatorTrace;
    pub fn verilator_trace_free(tfp: *mut VerilatorTrace);
    pub fn verilator_trace_open(tfp: *mut VerilatorTrace, filename: *const c_char) -> bool;
    pub fn verilator_trace_dump(tfp: *mut VerilatorTrace, timeui: u64);
    pub fn verilator_trace_close(tfp: *mut VerilatorTrace);
}
