use super::trace::with_current_trace;

pub struct MTraceEvent {
    pub is_write: bool,
    pub is_shared: bool,
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

pub fn mtrace(event: MTraceEvent) {
    with_current_trace(|trace| {
        let event_name = if event.is_write { "write" } else { "read" };
        let data = event
            .is_write
            .then(|| {
                format!(
                    r#","write_mask":"0x{:04x}","data":"0x{:016x}{:016x}""#,
                    event.write_mask, event.data_hi, event.data_lo
                )
            })
            .unwrap_or_default();
        let json = format!(
            r#"{{"type":"mtrace","clk":{},"event":"{}","channel":{},"hart_id":{},"rob_id":{},"is_shared":{},"vbank_id":{},"pbank_id":{},"group_id":{},"addr":"0x{:08x}"{}}}"#,
            trace.bemu_clk(),
            event_name,
            event.channel,
            event.hart_id,
            event.rob_id,
            u8::from(event.is_shared),
            event.vbank_id,
            event.pbank_id,
            event.group_id,
            event.addr,
            data
        );

        trace.write_mtrace(&json);
    });
}
