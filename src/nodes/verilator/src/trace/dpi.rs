// DPI-C callback implementations (called from RTL)

use crate::trace;

// DPI-C exports (called from RTL via extern "C")

#[no_mangle]
pub extern "C" fn dpi_bdb_set_clk(c: u64) {
    trace::set_rtl_clk(c);
}

#[no_mangle]
pub extern "C" fn dpi_itrace(
    is_issue: u8,
    rob_id: u32,
    domain_id: u32,
    funct: u32,
    pc: u64,
    rs1: u64,
    rs2: u64,
    bank_enable: u8,
) {
    trace::itrace(trace::ITraceEvent {
        is_issue,
        rob_id,
        domain_id,
        funct,
        pc,
        rs1,
        rs2,
        bank_enable,
    });
}

#[no_mangle]
pub extern "C" fn dpi_mtrace(
    is_write: u8,
    is_shared: u8,
    channel: u32,
    hart_id: u64,
    vbank_id: u32,
    pbank_id: u32,
    group_id: u32,
    addr: u32,
    data_lo: u64,
    data_hi: u64,
) {
    trace::mtrace(trace::MTraceEvent {
        is_write,
        is_shared,
        channel,
        hart_id,
        vbank_id,
        pbank_id,
        group_id,
        addr,
        data_lo,
        data_hi,
    });
}

#[no_mangle]
pub extern "C" fn dpi_pmctrace(ball_id: u32, rob_id: u32, elapsed: u64) {
    trace::pmctrace_ball(ball_id, rob_id, elapsed);
}

#[no_mangle]
pub extern "C" fn dpi_mem_pmctrace(is_store: u8, rob_id: u32, elapsed: u64) {
    trace::pmctrace_mem(is_store, rob_id, elapsed);
}

#[no_mangle]
pub extern "C" fn dpi_ctrace(subcmd: u8, ctr_id: u32, tag: u64, elapsed: u64, cycle: u64) {
    trace::ctrace(subcmd, ctr_id, tag, elapsed, cycle);
}

// ================================================================
// JTAG DPI-C function
// ================================================================

#[no_mangle]
pub extern "C" fn jtag_tick(
    _tck: *mut u8,
    _tms: *mut u8,
    _tdi: *mut u8,
    _tdo: *mut u8,
    _trstn: u8,
    _jtag_id: *mut u32,
) -> u8 {
    // Stub implementation - JTAG not currently used
    0
}
