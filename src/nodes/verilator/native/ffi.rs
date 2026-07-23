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

    // Top module
    pub fn verilator_top_new(ctx: *mut VerilatorContext) -> *mut VerilatorTop;
    pub fn verilator_top_free(top: *mut VerilatorTop);
    pub fn verilator_top_eval(top: *mut VerilatorTop);
    pub fn verilator_top_trace(top: *mut VerilatorTop, tfp: *mut VerilatorTrace, levels: c_int);
    pub fn verilator_private_bank_count() -> u32;
    pub fn verilator_private_bank_bytes(top: *mut VerilatorTop) -> u32;
    pub fn verilator_read_private_bank(top: *mut VerilatorTop, bank_id: u32, out: *mut u8, out_len: u32) -> bool;
    pub fn verilator_hash_private_bank(top: *mut VerilatorTop, bank_id: u32, out_hash: *mut u64) -> bool;
    pub fn verilator_flip_private_bank_bit(top: *mut VerilatorTop, bank_id: u32, byte_offset: u32, bit: u8) -> bool;
    pub fn verilator_resolve_private_bank_mask(top: *mut VerilatorTop, vbank_id: u32, pbank_mask: *mut u32) -> bool;
    pub fn verilator_read_rob_bank_access(
        top: *mut VerilatorTop,
        rob_id: u32,
        rd0_valid: *mut bool,
        rd0_vbank_id: *mut u32,
        rd1_valid: *mut bool,
        rd1_vbank_id: *mut u32,
        wr_valid: *mut bool,
        wr_vbank_id: *mut u32,
    ) -> bool;

    // Top module signals
    pub fn verilator_top_set_clock(top: *mut VerilatorTop, val: u8);
    pub fn verilator_top_set_reset(top: *mut VerilatorTop, val: u8);

    // SCU state query (DPI-C functions are called from RTL automatically)
    pub fn verilator_scu_has_exit() -> bool;
    pub fn verilator_scu_exit_code() -> i32;
    pub fn verilator_scu_push_uart_rx(hart_id: u32, byte: u32);
    pub fn verilator_scu_drain_uart_tx(buf: *mut u32, len: u32) -> u32;

    // FST trace
    pub fn verilator_trace_new() -> *mut VerilatorTrace;
    pub fn verilator_trace_free(tfp: *mut VerilatorTrace);
    pub fn verilator_trace_open(tfp: *mut VerilatorTrace, filename: *const c_char) -> bool;
    pub fn verilator_trace_dump(tfp: *mut VerilatorTrace, timeui: u64);
    pub fn verilator_trace_close(tfp: *mut VerilatorTrace);
}
