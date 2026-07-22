use crate::state;

pub struct BankTraceEvent {
    pub event: &'static str,
    pub is_shared: u8,
    pub vbank_id: u32,
    pub pbank_id: u32,
    pub group_id: u32,
    pub addr: u32,
    pub data_lo: Option<u64>,
    pub data_hi: Option<u64>,
}

pub fn banktrace(event: BankTraceEvent) {
    if !state::banktrace_enabled() {
        return;
    }

    let clk = state::rtl_clk();
    let json = match (event.data_lo, event.data_hi) {
        (Some(data_lo), Some(data_hi)) => format!(
            r#"{{"type":"banktrace","clk":{},"event":"{}","bank_id":{},"row":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}","data":"0x{:016x}{:016x}"}}"#,
            clk,
            event.event,
            event.pbank_id,
            event.addr,
            event.is_shared,
            event.vbank_id,
            event.pbank_id,
            event.group_id,
            event.addr,
            data_hi,
            data_lo
        ),
        _ => format!(
            r#"{{"type":"banktrace","clk":{},"event":"{}","bank_id":{},"row":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}"}}"#,
            clk,
            event.event,
            event.pbank_id,
            event.addr,
            event.is_shared,
            event.vbank_id,
            event.pbank_id,
            event.group_id,
            event.addr
        ),
    };

    state::write_trace(&json);
}
