use log::info;
use std::io::Result;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

pub struct HostConfig {
  pub host: String,
  pub arg: Vec<String>,
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
      // .join("bb-tests/build/workloads/src/CTest/bebop/ctest_mvin_mvout_bebop_test_singlecore-baremetal")
      .join("bb-tests/output/workloads/src/CTest/gemmini/gemmini_transpose_singlecore-baremetal")
      .to_string_lossy()
      .to_string();

    Self {
      host: host_path,
      arg: vec!["--extension=bebop".to_string(), test_binary_path],
    }
  }
}

fn launch_host(config: &HostConfig) -> Result<Child> {
  info!("Launching host process...");
  info!("Host binary: {}", config.host);
  info!("Args: {:?}\n", config.arg);

  let mut cmd = Command::new(&config.host);
  for arg in &config.arg {
    cmd.arg(arg);
  }
  cmd.spawn()
}

pub fn launch_host_process(host_config: HostConfig) -> Result<(Option<Child>, Arc<AtomicBool>)> {
  let host_exit = Arc::new(AtomicBool::new(false));

  let mut host_process = match launch_host(&host_config) {
    Ok(child) => {
      // println!("host process started with PID: {}", child.id());
      Some(child)
    },
    Err(e) => {
      eprintln!("Warning: Failed to start host process: {}", e);
      eprintln!("You may need to start host manually.");
      None
    },
  };

  // Start a thread to monitor host process
  if let Some(child) = host_process.take() {
    let exit_flag = Arc::clone(&host_exit);
    host_process = Some(child);

    // Take the child process out to move into thread
    if let Some(mut child_process) = host_process.take() {
      thread::spawn(move || match child_process.wait() {
        Ok(status) => {
          // println!("host process exited with status: {}", status);
          exit_flag.store(true, Ordering::Relaxed);
        },
        Err(e) => {
          eprintln!("Error waiting for host process: {}", e);
          exit_flag.store(true, Ordering::Relaxed);
        },
      });
    }
  }

  Ok((host_process, host_exit))
}
