use crate::state;

#[derive(Clone, Copy, Debug)]
pub struct MTraceEvent {
    pub is_write: u8,
    pub is_shared: u8,
    pub channel: u32,
    pub hart_id: u64,
    pub rob_id: u32,
    pub inst_id: u64,
    pub vbank_id: u32,
    pub pbank_id: u32,
    pub group_id: u32,
    pub addr: u32,
    pub write_mask: u32,
    pub data_lo: u64,
    pub data_hi: u64,
}

pub fn mtrace(event: MTraceEvent) {
    if !state::mtrace_enabled() {
        return;
    }

    let clk = state::rtl_clk();
    let json = if event.is_write != 0 {
        format!(
            r#"{{"type":"mtrace","clk":{},"event":"write","channel":{},"hart_id":{},"rob_id":{},"inst_id":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}","write_mask":"0x{:04x}","data":"0x{:016x}{:016x}"}}"#,
            clk,
            event.channel,
            event.hart_id,
            event.rob_id,
            event.inst_id,
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
            r#"{{"type":"mtrace","clk":{},"event":"read","channel":{},"hart_id":{},"rob_id":{},"inst_id":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}"}}"#,
            clk,
            event.channel,
            event.hart_id,
            event.rob_id,
            event.inst_id,
            event.is_shared,
            event.vbank_id,
            event.pbank_id,
            event.group_id,
            event.addr
        )
    };

    state::write_trace(&json);
}
