use crate::simulator::host::config::load_host_config;
use crate::simulator::sim::mode::SimConfig;
use log::info;
use std::io::Result;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

pub struct HostConfig {
  pub host: String,
  pub arg: Vec<String>,
}

impl HostConfig {
  pub fn from_sim_config(sim_config: &SimConfig) -> Self {
    // Get the workspace root (3 levels up from bebop/bebop/src)
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();

    let config_path = sim_config.host_config.as_ref().map(|p| {
      let path = PathBuf::from(p);
      if path.is_absolute() {
        path
      } else {
        workspace_root.join(p)
      }
    });

    let cfg = load_host_config(config_path.as_ref(), &sim_config.host_type)
      .expect("failed to load host config");

    // Use absolute path directly, or join with workspace_root if relative
    let host_path = {
      let path = PathBuf::from(&cfg.host_path);
      if path.is_absolute() {
        cfg.host_path
      } else {
        workspace_root
          .join(&cfg.host_path)
          .to_string_lossy()
          .to_string()
      }
    };

    // Use absolute path directly, or join with workspace_root if relative
    let test_binary_path = {
      let path = PathBuf::from(&cfg.test_binary_path);
      if path.is_absolute() {
        cfg.test_binary_path
      } else {
        workspace_root
          .join(&cfg.test_binary_path)
          .to_string_lossy()
          .to_string()
      }
    };

    // Filter out empty strings from host_args
    let mut arg: Vec<String> = cfg
      .host_args
      .into_iter()
      .filter(|s| !s.is_empty())
      .collect();
    arg.push(test_binary_path);

    Self {
      host: host_path,
      arg,
    }
  }
}

impl Default for HostConfig {
  fn default() -> Self {
    let sim_config = SimConfig {
      quiet: false,
      step_mode: crate::simulator::sim::mode::StepMode::Continuous,
      trace_file: None,
      arch_type: crate::simulator::sim::mode::ArchType::Buckyball,
      host_type: crate::simulator::sim::mode::HostType::Spike,
      host_config: None,
    };
    Self::from_sim_config(&sim_config)
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
        Ok(_status) => {
          // println!("host process exited with status: {}", _status);
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
