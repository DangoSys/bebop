use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum ConvertError {
  Io(std::io::Error),
  Json(serde_json::Error),
  InvalidLine { line: usize, msg: String },
  Overflow { line: usize, msg: String },
}

impl Display for ConvertError {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    match self {
      ConvertError::Io(e) => write!(f, "io error: {e}"),
      ConvertError::Json(e) => write!(f, "json error: {e}"),
      ConvertError::InvalidLine { line, msg } => write!(f, "line {line}: {msg}"),
      ConvertError::Overflow { line, msg } => write!(f, "line {line}: {msg}"),
    }
  }
}

impl std::error::Error for ConvertError {}

impl From<std::io::Error> for ConvertError {
  fn from(value: std::io::Error) -> Self {
    ConvertError::Io(value)
  }
}

impl From<serde_json::Error> for ConvertError {
  fn from(value: serde_json::Error) -> Self {
    ConvertError::Json(value)
  }
}
