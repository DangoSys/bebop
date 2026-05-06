use crate::error::ConvertError;
use crate::ids::PID_BANKTRACE;
use crate::state::State;
use crate::util::{req_str, req_u64_flex};
use serde_json::{json, Map, Value};

pub fn on_banktrace(
  state: &mut State,
  obj: &Map<String, Value>,
  line_no: usize,
  ts: u64,
) -> Result<(), ConvertError> {
  let event = req_str(obj, "event", line_no)?;
  let bank_id = req_u64_flex(obj, "bank_id", line_no)?;
  match event {
    "backdoor_read" | "backdoor_write" => {
      state.events.push(json!({
        "name": format!("banktrace.{event}"), "cat": "banktrace",
        "ph": "i", "s": "t", "ts": ts, "pid": PID_BANKTRACE, "tid": bank_id,
        "args": {
          "event": event,
          "bank_id": bank_id,
          "row": req_u64_flex(obj, "row", line_no)?,
          "data": req_str(obj, "data", line_no)?
        }
      }));
      Ok(())
    }
    other => Err(ConvertError::InvalidLine {
      line: line_no,
      msg: format!("unsupported banktrace event: {other}"),
    }),
  }
}
