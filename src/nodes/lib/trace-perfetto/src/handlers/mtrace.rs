use crate::error::ConvertError;
use crate::ids::PID_MTRACE;
use crate::state::State;
use crate::util::{req_str, req_u64_flex};
use serde_json::{json, Map, Value};

pub fn on_mtrace(state: &mut State, obj: &Map<String, Value>, line_no: usize, ts: u64) -> Result<(), ConvertError> {
    let event = req_str(obj, "event", line_no)?;
    let channel = req_u64_flex(obj, "channel", line_no)?;
    let hart_id = req_u64_flex(obj, "hart_id", line_no)?;
    let is_shared = req_u64_flex(obj, "is_shared", line_no)?;
    let vbank_id = req_u64_flex(obj, "vbank_id", line_no)?;
    let pbank_id = req_u64_flex(obj, "pbank_id", line_no)?;
    let group_id = req_u64_flex(obj, "group_id", line_no)?;
    let addr = req_str(obj, "addr", line_no)?;

    match event {
        "read" => {
            state.events.push(json!({
              "name": "mtrace.read", "cat": "mtrace", "ph": "i", "s": "t", "ts": ts,
              "pid": PID_MTRACE, "tid": channel,
              "args": {
                "event": event, "channel": channel, "hart_id": hart_id, "is_shared": is_shared,
                "vbank_id": vbank_id, "pbank_id": pbank_id, "group_id": group_id, "addr": addr
              }
            }));
            Ok(())
        }
        "write" => {
            state.events.push(json!({
              "name": "mtrace.write", "cat": "mtrace", "ph": "i", "s": "t", "ts": ts,
              "pid": PID_MTRACE, "tid": channel,
              "args": {
                "event": event, "channel": channel, "hart_id": hart_id, "is_shared": is_shared,
                "vbank_id": vbank_id, "pbank_id": pbank_id, "group_id": group_id, "addr": addr,
                "data": req_str(obj, "data", line_no)?
              }
            }));
            Ok(())
        }
        other => Err(ConvertError::InvalidLine {
            line: line_no,
            msg: format!("unsupported mtrace event: {other}"),
        }),
    }
}
