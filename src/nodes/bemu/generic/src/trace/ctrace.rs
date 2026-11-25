use super::trace::with_current_trace;

/// One TraceBall counter record, in the format of the RTL `dpi_ctrace` hook
/// (bebop-rtl-trace), so the same tools read both simulators.
pub fn ctrace(subcmd: u32, ctr_id: u32, tag: u64, elapsed: u64, cycle: u64) {
    with_current_trace(|trace| {
        if !trace.ctrace {
            return;
        }
        let json = match subcmd {
            0 => format!(
                r#"{{"type":"ctrace","clk":{},"event":"ctr_start","ctr_id":{},"tag":"0x{:X}","cycle":{}}}"#,
                cycle, ctr_id, tag, cycle
            ),
            1 => format!(
                r#"{{"type":"ctrace","clk":{},"event":"ctr_stop","ctr_id":{},"tag":"0x{:X}","elapsed":{},"cycle":{}}}"#,
                cycle, ctr_id, tag, elapsed, cycle
            ),
            2 => format!(
                r#"{{"type":"ctrace","clk":{},"event":"ctr_read","ctr_id":{},"current":{},"cycle":{}}}"#,
                cycle, ctr_id, elapsed, cycle
            ),
            _ => panic!("ctrace: illegal subcmd {subcmd}"),
        };
        trace.write_ctrace(&json);
    });
}
