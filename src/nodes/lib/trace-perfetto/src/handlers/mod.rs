mod banktrace;
mod ctrace;
mod itrace;
mod mtrace;
mod pmctrace;

use crate::error::ConvertError;
use crate::state::State;
use serde_json::{Map, Value};

pub fn handle_record(
    state: &mut State,
    typ: &str,
    obj: &Map<String, Value>,
    line_no: usize,
    ts: u64,
) -> Result<(), ConvertError> {
    match typ {
        "itrace" => itrace::on_itrace(state, obj, line_no, ts),
        "mtrace" => mtrace::on_mtrace(state, obj, line_no, ts),
        "pmctrace" => pmctrace::on_pmctrace(state, obj, line_no, ts),
        "ctrace" => ctrace::on_ctrace(state, obj, line_no, ts),
        "banktrace" => banktrace::on_banktrace(state, obj, line_no, ts),
        other => Err(ConvertError::InvalidLine {
            line: line_no,
            msg: format!("unsupported trace type: {other}"),
        }),
    }
}
