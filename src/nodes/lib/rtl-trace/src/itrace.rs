use crate::state;

pub struct ITraceEvent {
    pub hart_id: u64,
    pub is_issue: u8,
    pub rob_id: u32,
    pub inst_id: u64,
    pub domain_id: u32,
    pub funct: u32,
    pub pc: u64,
    pub rs1: u64,
    pub rs2: u64,
}

pub fn itrace(event: ITraceEvent) {
    if !state::itrace_enabled() {
        return;
    }

    let clk = state::rtl_clk();
    let event_name = match event.is_issue {
        2 => "alloc",
        1 => "issue",
        _ => "complete",
    };

    let json = if event.is_issue >= 1 {
        format!(
            r#"{{"type":"itrace","clk":{},"event":"{}","hart_id":{},"rob_id":{},"inst_id":{},"domain_id":{},"funct":"0x{:02x}","pc":"0x{:016x}","rs1":"0x{:016x}","rs2":"0x{:016x}"}}"#,
            clk,
            event_name,
            event.hart_id,
            event.rob_id,
            event.inst_id,
            event.domain_id,
            event.funct,
            event.pc,
            event.rs1,
            event.rs2
        )
    } else {
        format!(
            r#"{{"type":"itrace","clk":{},"event":"{}","hart_id":{},"rob_id":{},"inst_id":{},"domain_id":{},"funct":"0x{:02x}","pc":"0x{:016x}"}}"#,
            clk, event_name, event.hart_id, event.rob_id, event.inst_id, event.domain_id, event.funct, event.pc
        )
    };

    state::write_trace(&json);
}
