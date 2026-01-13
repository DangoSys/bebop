use crate::simulator::sim::mode::HostType;
use serde::Deserialize;
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct HostTomlSection {
  host_path: String,
  test_binary_path: String,
  host_args: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ConfigToml {
  spike: Option<HostTomlSection>,
  gem5: Option<HostTomlSection>,
}

#[derive(Debug, Clone)]
pub struct HostConfigData {
  pub host_path: String,
  pub test_binary_path: String,
  pub host_args: Vec<String>,
}

pub fn load_host_config(
  config_path: Option<&PathBuf>,
  host_type: &HostType,
) -> io::Result<HostConfigData> {
  let config_path = match config_path {
    Some(path) => path.clone(),
    None => {
      // CARGO_MANIFEST_DIR points to bebop/bebop
      let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
      manifest_dir
        .join("src")
        .join("simulator")
        .join("host")
        .join("host.toml")
    },
  };

  let content = fs::read_to_string(&config_path)?;

  let raw: ConfigToml = toml::from_str(&content).map_err(|e| {
    io::Error::new(
      io::ErrorKind::InvalidData,
      format!("parse host.toml failed: {}", e),
    )
  })?;

  let config = match host_type {
    HostType::Spike => raw
      .spike
      .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "spike config not found"))?,
    HostType::Gem5 => raw
      .gem5
      .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "gem5 config not found"))?,
  };

  Ok(HostConfigData {
    host_path: config.host_path,
    test_binary_path: config.test_binary_path,
    host_args: config.host_args,
  })
}
