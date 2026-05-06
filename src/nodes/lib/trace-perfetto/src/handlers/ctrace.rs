use crate::error::ConvertError;
use crate::ids::PID_CTRACE;
use crate::state::State;
use crate::util::{req_str, req_u64_flex};
use serde_json::{json, Map, Value};

pub fn on_ctrace(
  state: &mut State,
  obj: &Map<String, Value>,
  line_no: usize,
  ts: u64,
) -> Result<(), ConvertError> {
  let event = req_str(obj, "event", line_no)?;
  let ctr_id = req_u64_flex(obj, "ctr_id", line_no)?;
  match event {
    "ctr_start" => on_start(state, obj, line_no, ts, ctr_id),
    "ctr_stop" => on_stop(state, obj, line_no, ts, ctr_id),
    "ctr_read" => on_read(state, obj, line_no, ts, ctr_id),
    other => Err(ConvertError::InvalidLine {
      line: line_no,
      msg: format!("unsupported ctrace event: {other}"),
    }),
  }
}

fn on_start(
  state: &mut State,
  obj: &Map<String, Value>,
  line_no: usize,
  ts: u64,
  ctr_id: u64,
) -> Result<(), ConvertError> {
  let tag = req_u64_flex(obj, "tag", line_no)?;
  let key = (ctr_id, tag);
  let name = format!("ctr#{ctr_id}/tag=0x{tag:X}");
  if state.open_ctr.contains_key(&key) {
    return Err(ConvertError::InvalidLine {
      line: line_no,
      msg: format!("duplicated ctrace start for ctr_id={ctr_id}, tag=0x{tag:X}"),
    });
  }
  state.open_ctr.insert(key, name.clone());
  state.events.push(json!({
    "name": name, "cat": "ctrace", "ph": "B", "ts": ts, "pid": PID_CTRACE, "tid": ctr_id,
    "args": {
      "event": "ctr_start", "ctr_id": ctr_id, "tag": format!("0x{tag:X}"),
      "cycle": req_u64_flex(obj, "cycle", line_no)?
    }
  }));
  Ok(())
}

fn on_stop(
  state: &mut State,
  obj: &Map<String, Value>,
  line_no: usize,
  ts: u64,
  ctr_id: u64,
) -> Result<(), ConvertError> {
  let tag = req_u64_flex(obj, "tag", line_no)?;
  let key = (ctr_id, tag);
  let name = state.open_ctr.remove(&key).ok_or_else(|| ConvertError::InvalidLine {
    line: line_no,
    msg: format!("ctrace stop without start for ctr_id={ctr_id}, tag=0x{tag:X}"),
  })?;
  state.events.push(json!({
    "name": name, "cat": "ctrace", "ph": "E", "ts": ts, "pid": PID_CTRACE, "tid": ctr_id,
    "args": {
      "event": "ctr_stop", "ctr_id": ctr_id, "tag": format!("0x{tag:X}"),
      "elapsed": req_u64_flex(obj, "elapsed", line_no)?,
      "cycle": req_u64_flex(obj, "cycle", line_no)?
    }
  }));
  Ok(())
}

fn on_read(
  state: &mut State,
  obj: &Map<String, Value>,
  line_no: usize,
  ts: u64,
  ctr_id: u64,
) -> Result<(), ConvertError> {
  state.events.push(json!({
    "name": format!("ctr#{ctr_id}.current"),
    "cat": "ctrace",
    "ph": "C",
    "ts": ts,
    "pid": PID_CTRACE,
    "tid": ctr_id,
    "args": {
      "current": req_u64_flex(obj, "current", line_no)?,
      "cycle": req_u64_flex(obj, "cycle", line_no)?
    }
  }));
  Ok(())
}
