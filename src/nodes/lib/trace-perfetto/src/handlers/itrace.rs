use crate::error::ConvertError;
use crate::ids::PID_ITRACE;
use crate::state::{OpenRob, State};
use crate::util::{req_str, req_u64_flex};
use serde_json::{json, Map, Value};

pub fn on_itrace(state: &mut State, obj: &Map<String, Value>, line_no: usize, ts: u64) -> Result<(), ConvertError> {
    let event = req_str(obj, "event", line_no)?;
    let rob_id = req_u64_flex(obj, "rob_id", line_no)?;
    let domain_id = req_u64_flex(obj, "domain_id", line_no)?;
    let funct = req_str(obj, "funct", line_no)?;
    let bank_enable = req_u64_flex(obj, "bank_enable", line_no)?;
    let bank = req_str(obj, "bank", line_no)?;
    let pc = req_str(obj, "pc", line_no)?;
    let name = format!("rob#{rob_id}");

    match event {
        "alloc" => on_alloc(
            state,
            obj,
            line_no,
            ts,
            rob_id,
            domain_id,
            funct,
            bank_enable,
            bank,
            pc,
            name,
        ),
        "issue" => on_issue(state, obj, line_no, ts, rob_id, domain_id, funct, bank_enable, bank, pc),
        "complete" => on_complete(state, line_no, ts, rob_id, domain_id, funct, bank_enable, bank, pc),
        other => Err(ConvertError::InvalidLine {
            line: line_no,
            msg: format!("unsupported itrace event: {other}"),
        }),
    }
}

fn on_alloc(
    state: &mut State,
    obj: &Map<String, Value>,
    line_no: usize,
    ts: u64,
    rob_id: u64,
    domain_id: u64,
    funct: &str,
    bank_enable: u64,
    bank: &str,
    pc: &str,
    name: String,
) -> Result<(), ConvertError> {
    if state.open_rob.contains_key(&rob_id) {
        return Err(ConvertError::InvalidLine {
            line: line_no,
            msg: format!("duplicated itrace alloc for rob_id={rob_id}"),
        });
    }
    state.open_rob.insert(
        rob_id,
        OpenRob {
            domain_id,
            name: name.clone(),
        },
    );
    state.events.push(json!({
      "name": name, "cat": "itrace", "ph": "B", "ts": ts, "pid": PID_ITRACE, "tid": domain_id,
      "args": {
        "rob_id": rob_id, "domain_id": domain_id, "funct": funct, "bank_enable": bank_enable,
        "bank": bank, "pc": pc, "rs1": req_str(obj, "rs1", line_no)?, "rs2": req_str(obj, "rs2", line_no)?
      }
    }));
    Ok(())
}

fn on_issue(
    state: &mut State,
    obj: &Map<String, Value>,
    line_no: usize,
    ts: u64,
    rob_id: u64,
    domain_id: u64,
    funct: &str,
    bank_enable: u64,
    bank: &str,
    pc: &str,
) -> Result<(), ConvertError> {
    if !state.open_rob.contains_key(&rob_id) {
        return Err(ConvertError::InvalidLine {
            line: line_no,
            msg: format!("itrace issue without alloc, rob_id={rob_id}"),
        });
    }
    state.events.push(json!({
      "name": "issue", "cat": "itrace", "ph": "i", "s": "t", "ts": ts,
      "pid": PID_ITRACE, "tid": domain_id,
      "args": {
        "rob_id": rob_id, "domain_id": domain_id, "funct": funct, "bank_enable": bank_enable,
        "bank": bank, "pc": pc, "rs1": req_str(obj, "rs1", line_no)?, "rs2": req_str(obj, "rs2", line_no)?
      }
    }));
    Ok(())
}

fn on_complete(
    state: &mut State,
    line_no: usize,
    ts: u64,
    rob_id: u64,
    domain_id: u64,
    funct: &str,
    bank_enable: u64,
    bank: &str,
    pc: &str,
) -> Result<(), ConvertError> {
    let open = state
        .open_rob
        .remove(&rob_id)
        .ok_or_else(|| ConvertError::InvalidLine {
            line: line_no,
            msg: format!("itrace complete without alloc, rob_id={rob_id}"),
        })?;
    if open.domain_id != domain_id {
        return Err(ConvertError::InvalidLine {
            line: line_no,
            msg: format!(
                "itrace complete domain mismatch for rob_id={rob_id}: alloc={} complete={}",
                open.domain_id, domain_id
            ),
        });
    }
    state.events.push(json!({
    "name": open.name, "cat": "itrace", "ph": "E", "ts": ts, "pid": PID_ITRACE, "tid": domain_id,
    "args": { "rob_id": rob_id, "domain_id": domain_id, "funct": funct, "bank_enable": bank_enable, "bank": bank, "pc": pc }
  }));
    Ok(())
}
