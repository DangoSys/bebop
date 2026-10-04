use super::trace::with_current_trace;

pub struct ITraceEvent {
    pub funct: u32,
    pub pc: u64,
    pub rs1: u64,
    pub rs2: u64,
}

pub fn itrace(event: ITraceEvent) {
    with_current_trace(|trace| {
        if !trace.itrace {
            return;
        }
        let json = format!(
            r#"{{"type":"itrace","event_index":{},"event":"complete","funct":"0x{:02x}","pc":"0x{:016x}","rs1":"0x{:016x}","rs2":"0x{:016x}"}}"#,
            trace.event_index(),
            event.funct,
            event.pc,
            event.rs1,
            event.rs2
        );

        trace.write_itrace(&json);
    });
}
