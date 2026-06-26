use crate::error::ConvertError;
use crate::ids::PID_PMCTRACE;
use crate::state::State;
use crate::util::req_str;
use crate::util::req_u64_flex;
use serde_json::{json, Map, Value};

pub fn on_pmctrace(state: &mut State, obj: &Map<String, Value>, line_no: usize, ts: u64) -> Result<(), ConvertError> {
    let event = req_str(obj, "event", line_no)?;
    match event {
        "ball" => {
            let ball_id = req_u64_flex(obj, "ball_id", line_no)?;
            state.events.push(json!({
              "name": "pmctrace.ball", "cat": "pmctrace", "ph": "i", "s": "t", "ts": ts,
              "pid": PID_PMCTRACE, "tid": ball_id,
              "args": {
                "event": event, "ball_id": ball_id,
                "rob_id": req_u64_flex(obj, "rob_id", line_no)?,
                "elapsed": req_u64_flex(obj, "elapsed", line_no)?
              }
            }));
            Ok(())
        }
        "load" | "store" => {
            state.events.push(json!({
              "name": format!("pmctrace.{event}"), "cat": "pmctrace", "ph": "i", "s": "t", "ts": ts,
              "pid": PID_PMCTRACE, "tid": 0,
              "args": {
                "event": event,
                "rob_id": req_u64_flex(obj, "rob_id", line_no)?,
                "elapsed": req_u64_flex(obj, "elapsed", line_no)?
              }
            }));
            Ok(())
        }
        other => Err(ConvertError::InvalidLine {
            line: line_no,
            msg: format!("unsupported pmctrace event: {other}"),
        }),
    }
}
