// DPI-C callback implementations (called from RTL)

use crate::trace;

// DPI-C exports (called from RTL via extern "C")

#[no_mangle]
pub extern "C" fn dpi_bdb_set_clk(c: u64) {
    trace::set_rtl_clk(c);
}

fn u64_from_words(lo: u32, hi: u32) -> u64 {
    (u64::from(hi) << 32) | u64::from(lo)
}

#[no_mangle]
pub extern "C" fn dpi_itrace(
    is_issue: u32,
    rob_id: u32,
    domain_id: u32,
    funct: u32,
    pc_lo: u32,
    pc_hi: u32,
    _rs1_idx_lo: u32,
    _rs1_idx_hi: u32,
    _rs2_idx_lo: u32,
    _rs2_idx_hi: u32,
    rs1_data_lo: u32,
    rs1_data_hi: u32,
    rs2_data_lo: u32,
    rs2_data_hi: u32,
    bank_enable: u32,
) {
    trace::itrace(trace::ITraceEvent {
        is_issue: is_issue as u8,
        rob_id,
        domain_id,
        funct,
        pc: u64_from_words(pc_lo, pc_hi),
        rs1: u64_from_words(rs1_data_lo, rs1_data_hi),
        rs2: u64_from_words(rs2_data_lo, rs2_data_hi),
        bank_enable: bank_enable as u8,
    });
}

#[no_mangle]
pub extern "C" fn dpi_mtrace(
    is_write: u32,
    is_shared: u32,
    channel: u32,
    hart_id_lo: u32,
    hart_id_hi: u32,
    vbank_id: u32,
    pbank_id: u32,
    group_id: u32,
    addr: u32,
    data_lo_lo: u32,
    data_lo_hi: u32,
    data_hi_lo: u32,
    data_hi_hi: u32,
) {
    trace::mtrace(trace::MTraceEvent {
        is_write: is_write as u8,
        is_shared: is_shared as u8,
        channel,
        hart_id: u64_from_words(hart_id_lo, hart_id_hi),
        vbank_id,
        pbank_id,
        group_id,
        addr,
        data_lo: u64_from_words(data_lo_lo, data_lo_hi),
        data_hi: u64_from_words(data_hi_lo, data_hi_hi),
    });
}

#[no_mangle]
pub extern "C" fn dpi_bank_write_dispatch(rob_id: u32) {
    trace::bank_write_dispatch(rob_id);
}

#[no_mangle]
pub extern "C" fn dpi_bank_write_visible(rob_id: u32, vbank_id: u32, pbank_id: u32, group_id: u32) {
    trace::bank_write_visible(rob_id, vbank_id, pbank_id, group_id);
}

#[no_mangle]
pub extern "C" fn dpi_bank_write_end(rob_id: u32) {
    trace::bank_write_end(rob_id);
}

#[no_mangle]
pub extern "C" fn dpi_bank_instruction_cancel(rob_id: u32) {
    trace::bank_instruction_cancel(rob_id);
}

#[no_mangle]
pub extern "C" fn dpi_pmctrace(ball_id: u32, rob_id: u32, elapsed_lo: u32, elapsed_hi: u32) {
    trace::pmctrace_ball(ball_id, rob_id, u64_from_words(elapsed_lo, elapsed_hi));
}

#[no_mangle]
pub extern "C" fn dpi_mem_pmctrace(is_store: u32, rob_id: u32, elapsed_lo: u32, elapsed_hi: u32) {
    trace::pmctrace_mem(is_store as u8, rob_id, u64_from_words(elapsed_lo, elapsed_hi));
}

#[no_mangle]
pub extern "C" fn dpi_ctrace(
    subcmd: u32,
    ctr_id: u32,
    tag_lo: u32,
    tag_hi: u32,
    elapsed_lo: u32,
    elapsed_hi: u32,
    cycle_lo: u32,
    cycle_hi: u32,
) {
    trace::ctrace(
        subcmd as u8,
        ctr_id,
        u64_from_words(tag_lo, tag_hi),
        u64_from_words(elapsed_lo, elapsed_hi),
        u64_from_words(cycle_lo, cycle_hi),
    );
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
