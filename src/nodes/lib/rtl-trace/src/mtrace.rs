use crate::banktrace::{banktrace, BankTraceEvent};
use crate::state;

pub struct MTraceEvent {
    pub is_write: u8,
    pub is_shared: u8,
    pub channel: u32,
    pub hart_id: u64,
    pub vbank_id: u32,
    pub pbank_id: u32,
    pub group_id: u32,
    pub addr: u32,
    pub data_lo: u64,
    pub data_hi: u64,
}

pub fn mtrace(event: MTraceEvent) {
    if event.is_write != 0 {
        banktrace(BankTraceEvent {
            event: "backdoor_write",
            is_shared: event.is_shared,
            vbank_id: event.vbank_id,
            pbank_id: event.pbank_id,
            group_id: event.group_id,
            addr: event.addr,
            data_lo: Some(event.data_lo),
            data_hi: Some(event.data_hi),
        });
    } else {
        banktrace(BankTraceEvent {
            event: "backdoor_read",
            is_shared: event.is_shared,
            vbank_id: event.vbank_id,
            pbank_id: event.pbank_id,
            group_id: event.group_id,
            addr: event.addr,
            data_lo: None,
            data_hi: None,
        });
    }

    if !state::mtrace_enabled() {
        return;
    }

    let clk = state::rtl_clk();
    let json = if event.is_write != 0 {
        format!(
            r#"{{"type":"mtrace","clk":{},"event":"write","channel":{},"hart_id":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}","data":"0x{:016x}{:016x}"}}"#,
            clk,
            event.channel,
            event.hart_id,
            event.is_shared,
            event.vbank_id,
            event.pbank_id,
            event.group_id,
            event.addr,
            event.data_hi,
            event.data_lo
        )
    } else {
        format!(
            r#"{{"type":"mtrace","clk":{},"event":"read","channel":{},"hart_id":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}"}}"#,
            clk,
            event.channel,
            event.hart_id,
            event.is_shared,
            event.vbank_id,
            event.pbank_id,
            event.group_id,
            event.addr
        )
    };

    state::write_trace(&json);
}
