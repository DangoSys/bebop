use std::io::Result;
use std::process::{Child, Command};
use crate::log_info;

pub struct HostConfig {
  pub host: String,
  pub test_binary: String,
}

impl Default for HostConfig {
  fn default() -> Self {
    // Get the workspace root (3 levels up from bebop/bebop/src)
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();

    let host_path = workspace_root
      .join("bebop/host/spike/riscv-isa-sim/install/bin/spike")
      .to_string_lossy()
      .to_string();

    let test_binary_path = workspace_root
      .join("bb-tests/build/workloads/src/CTest/toy/ctest_mvin_mvout_test_singlecore-baremetal")
      .to_string_lossy()
      .to_string();

    Self {
      host: host_path,
      test_binary: test_binary_path,
    }
  }
}

pub fn launch_host(config: &HostConfig) -> Result<Child> {
  log_info!("Launching host process...");
  log_info!("Host binary: {}", config.host);
  log_info!("Test binary: {}", config.test_binary);

  Command::new(&config.host)
    .arg("--extension=bebop")
    // .arg("--log-commits")
    .arg(&config.test_binary)
    .spawn()
}
