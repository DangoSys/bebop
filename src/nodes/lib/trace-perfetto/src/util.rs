use crate::error::ConvertError;
use serde_json::{Map, Value};

pub fn as_object<'a>(
  v: &'a Value,
  line_no: usize,
) -> Result<&'a Map<String, Value>, ConvertError> {
  v.as_object().ok_or_else(|| ConvertError::InvalidLine {
    line: line_no,
    msg: "record must be a JSON object".to_string(),
  })
}

pub fn req_str<'a>(
  obj: &'a Map<String, Value>,
  key: &str,
  line_no: usize,
) -> Result<&'a str, ConvertError> {
  obj.get(key)
    .ok_or_else(|| ConvertError::InvalidLine {
      line: line_no,
      msg: format!("missing required key: {key}"),
    })?
    .as_str()
    .ok_or_else(|| ConvertError::InvalidLine {
      line: line_no,
      msg: format!("key '{key}' must be string"),
    })
}

pub fn req_u64_flex(
  obj: &Map<String, Value>,
  key: &str,
  line_no: usize,
) -> Result<u64, ConvertError> {
  let v = obj.get(key).ok_or_else(|| ConvertError::InvalidLine {
    line: line_no,
    msg: format!("missing required key: {key}"),
  })?;
  match v {
    Value::Number(n) => n.as_u64().ok_or_else(|| ConvertError::InvalidLine {
      line: line_no,
      msg: format!("key '{key}' must be unsigned integer"),
    }),
    Value::String(s) => parse_u64_string(s, key, line_no),
    _ => Err(ConvertError::InvalidLine {
      line: line_no,
      msg: format!("key '{key}' must be unsigned integer or numeric string"),
    }),
  }
}

fn parse_u64_string(s: &str, key: &str, line_no: usize) -> Result<u64, ConvertError> {
  if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
    u64::from_str_radix(rest, 16).map_err(|e| ConvertError::InvalidLine {
      line: line_no,
      msg: format!("key '{key}' has invalid hex value '{s}': {e}"),
    })
  } else {
    s.parse::<u64>().map_err(|e| ConvertError::InvalidLine {
      line: line_no,
      msg: format!("key '{key}' has invalid integer value '{s}': {e}"),
    })
  }
}
