use crate::simulator::config::config::AppConfig;
use log::info;
use std::io::Result;
use std::path::Path;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

pub struct HostConfig {
  pub host: String,
  pub arg: Vec<String>,
}

impl HostConfig {
  /// 从AppConfig创建HostConfig（新方法）
  pub fn from_app_config(app_config: &AppConfig) -> Result<Self> {
    // 获取当前主机类型的配置
    let host_type = app_config.host.host_type.to_lowercase();
    let host_type_config = match host_type.as_str() {
      "spike" => app_config.host.spike.as_ref(),
      "gem5" => app_config.host.gem5.as_ref(),
      other => {
        return Err(std::io::Error::new(
          std::io::ErrorKind::InvalidInput,
          format!("不支持的主机类型: {}", other),
        ));
      },
    };

    let host_type_config = host_type_config.ok_or_else(|| {
      std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("配置中缺少主机类型 '{}' 的配置", host_type),
      )
    })?;

    // 根据主机类型构建参数
    let arg = if host_type == "gem5" {
      // gem5的情况：根据gem5_mode构建不同的命令行
      let mode = host_type_config.gem5_mode.to_lowercase();
      let mut args = Vec::new();
      let gem5_dir = Path::new(host_type_config.host_path.as_str()).parent().unwrap().to_string_lossy().to_string();
      let se_script_path = Path::new(gem5_dir.as_str()).join("../../../riscv-se.py").to_path_buf().to_string_lossy().to_string();
      let fs_script_path = Path::new(gem5_dir.as_str()).join("../../../riscv-fs-custom-kernel.py").to_path_buf().to_string_lossy().to_string();
      
      match mode.as_str() {
        "se" => {
          // SE模式: ./build/RISCV/gem5.opt ../riscv-se.py --test-binary <test_binary_path>
          args.push(se_script_path);
          args.push("--test-binary".to_string());
          args.push(host_type_config.se_binary_path.clone());
        },
        "fs" => {
          // FS模式: ./build/RISCV/gem5.opt ../riscv-fs-custom-kernel.py --custom-kernel <fs_kernel_path> --custom-disk-image <fs_image_path>
          args.push(fs_script_path);
          args.push("--custom-kernel".to_string());
          args.push(host_type_config.fs_kernel_path.clone());
          args.push("--custom-disk-image".to_string());
          args.push(host_type_config.fs_image_path.clone());
        },
        other => {
          return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("不支持的gem5模式: {}", other),
          ));
        },
      }
      args
    } else {
      // Spike或其他主机的情况：使用host_args和test_binary_path
      let mut args: Vec<String> = host_type_config
        .host_args
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| s.clone())
        .collect();
      args.push(host_type_config.test_binary_path.clone());
      args
    };

    Ok(Self {
      host: host_type_config.host_path.clone(),
      arg,
    })
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
