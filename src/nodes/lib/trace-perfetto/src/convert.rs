use crate::error::ConvertError;
use crate::handlers::handle_record;
use crate::options::ConvertOptions;
use crate::state::State;
use crate::util::{as_object, req_str, req_u64_flex};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub fn convert_ndjson_reader<R: BufRead>(
  reader: R,
  options: &ConvertOptions,
) -> Result<Value, ConvertError> {
  if options.tick_ns == 0 {
    return Err(ConvertError::InvalidLine {
      line: 0,
      msg: "tick_ns must be > 0".to_string(),
    });
  }

  let mut state = State::default();
  for (idx, line) in reader.lines().enumerate() {
    let line_no = idx + 1;
    let raw = line?;
    if raw.trim().is_empty() {
      return Err(ConvertError::InvalidLine {
        line: line_no,
        msg: "empty line is not allowed in NDJSON".to_string(),
      });
    }
    let v: Value = serde_json::from_str(&raw)?;
    let obj = as_object(&v, line_no)?;
    let typ = req_str(obj, "type", line_no)?;
    let clk = req_u64_flex(obj, "clk", line_no)?;
    let ts = clk
      .checked_mul(options.tick_ns)
      .ok_or_else(|| ConvertError::Overflow {
        line: line_no,
        msg: format!("timestamp overflow: clk={clk}, tick_ns={}", options.tick_ns),
      })?;
    handle_record(&mut state, typ, obj, line_no, ts)?;
  }

  ensure_closed(&state)?;
  Ok(json!({
    "displayTimeUnit": "ns",
    "traceEvents": state.events
  }))
}

pub fn convert_ndjson_writer<R: BufRead, W: Write>(
  reader: R,
  mut writer: W,
  options: &ConvertOptions,
) -> Result<(), ConvertError> {
  let out = convert_ndjson_reader(reader, options)?;
  serde_json::to_writer(&mut writer, &out)?;
  writer.write_all(b"\n")?;
  Ok(())
}

fn ensure_closed(state: &State) -> Result<(), ConvertError> {
  if let Some((rob_id, open)) = state.open_rob.iter().next() {
    return Err(ConvertError::InvalidLine {
      line: 0,
      msg: format!("itrace alloc is not closed: rob_id={rob_id}, domain_id={}", open.domain_id),
    });
  }
  if let Some(((ctr_id, _tag), name)) = state.open_ctr.iter().next() {
    return Err(ConvertError::InvalidLine {
      line: 0,
      msg: format!("ctrace start is not closed: ctr_id={ctr_id}, name={name}"),
    });
  }
  Ok(())
}
