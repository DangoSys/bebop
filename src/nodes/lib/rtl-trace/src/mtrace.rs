use crate::banktrace::{banktrace, BankTraceEvent};
use crate::state;

#[derive(Clone, Copy, Debug)]
pub struct MTraceEvent {
    pub is_write: u8,
    pub is_shared: u8,
    pub channel: u32,
    pub hart_id: u64,
    pub rob_id: u32,
    pub vbank_id: u32,
    pub pbank_id: u32,
    pub group_id: u32,
    pub addr: u32,
    pub write_mask: u32,
    pub data_lo: u64,
    pub data_hi: u64,
}

pub struct MTraceIssueEvent {
    pub is_shared: u8,
    pub hart_id: u64,
    pub rob_id: u32,
    pub vbank_id: u32,
    pub group_id: u32,
}

pub fn mtrace_issue(event: MTraceIssueEvent) {
    crate::bank_digest::record_write_issue(&event);
    if state::mtrace_enabled() {
        state::write_trace(&format!(
            r#"{{"type":"mtrace","clk":{},"event":"write_issue","hart_id":{},"rob_id":{},"is_shared":{},"vbank_id":{},"group_id":{}}}"#,
            state::rtl_clk(),
            event.hart_id,
            event.rob_id,
            event.is_shared,
            event.vbank_id,
            event.group_id
        ));
    }
}

pub fn mtrace(event: MTraceEvent) {
    crate::bank_digest::record_memory(&event);

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
            r#"{{"type":"mtrace","clk":{},"event":"write","channel":{},"hart_id":{},"rob_id":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}","write_mask":"0x{:04x}","data":"0x{:016x}{:016x}"}}"#,
            clk,
            event.channel,
            event.hart_id,
            event.rob_id,
            event.is_shared,
            event.vbank_id,
            event.pbank_id,
            event.group_id,
            event.addr,
            event.write_mask,
            event.data_hi,
            event.data_lo
        )
    } else {
        format!(
            r#"{{"type":"mtrace","clk":{},"event":"read","channel":{},"hart_id":{},"rob_id":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}"}}"#,
            clk,
            event.channel,
            event.hart_id,
            event.rob_id,
            event.is_shared,
            event.vbank_id,
            event.pbank_id,
            event.group_id,
            event.addr
        )
    };

    state::write_trace(&json);
}
