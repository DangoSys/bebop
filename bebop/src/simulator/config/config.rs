use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 主机类型配置部分
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HostTypeConfig {
  pub host_path: String,
  pub test_binary_path: String,
  #[serde(default)]
  pub host_args: Vec<String>,
  // gem5 特定配置
  #[serde(default)]
  pub gem5_mode: String, // "se" 或 "fs"
  #[serde(default)]
  pub se_binary_path: String, // SE模式下的test_binary_path
  #[serde(default)]
  pub fs_kernel_path: String, // FS模式下的内核路径
  #[serde(default)]
  pub fs_image_path: String, // FS模式下的磁盘镜像路径
}

/// 主机配置部分
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HostSection {
  pub host_type: String,
  #[serde(default)]
  pub spike: Option<HostTypeConfig>,
  #[serde(default)]
  pub gem5: Option<HostTypeConfig>,
}

impl Default for HostSection {
  fn default() -> Self {
    Self {
      host_type: "spike".to_string(),
      spike: None,
      gem5: None,
    }
  }
}

/// 模拟配置部分
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SimulationSection {
  #[serde(default = "default_arch_type")]
  pub arch_type: String,
  #[serde(default)]
  pub quiet: bool,
  #[serde(default = "default_step_mode")]
  pub step_mode: bool,
  #[serde(default)]
  pub trace_file: String,
}

fn default_arch_type() -> String {
  "buckyball".to_string()
}

fn default_step_mode() -> bool {
  false
}

impl Default for SimulationSection {
  fn default() -> Self {
    Self {
      arch_type: default_arch_type(),
      quiet: false,
      step_mode: default_step_mode(),
      trace_file: String::new(),
    }
  }
}

/// 统一的应用配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
  #[serde(default)]
  pub host: HostSection,
  #[serde(default)]
  pub simulation: SimulationSection,
}

impl Default for AppConfig {
  fn default() -> Self {
    Self {
      host: HostSection::default(),
      simulation: SimulationSection::default(),
    }
  }
}

/// 从default.toml加载默认配置
pub fn load_default_config() -> io::Result<AppConfig> {
  let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let config_path = manifest_dir
    .join("src")
    .join("simulator")
    .join("config")
    .join("default.toml");

  load_config_file(&config_path)
}

/// 从指定文件加载配置
pub fn load_config_file(path: &Path) -> io::Result<AppConfig> {
  let content = fs::read_to_string(path)
    .map_err(|e| io::Error::new(io::ErrorKind::NotFound, format!("无法读取配置文件 {:?}: {}", path, e)))?;

  toml::from_str::<AppConfig>(&content)
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("解析TOML配置失败: {}", e)))
}

/// 合并两个配置（后者覆盖前者）
pub fn merge_config(mut base: AppConfig, override_config: AppConfig) -> AppConfig {
  // 合并host部分
  if !override_config.host.host_type.is_empty() {
    base.host.host_type = override_config.host.host_type;
  }
  if override_config.host.spike.is_some() {
    base.host.spike = override_config.host.spike;
  }
  if override_config.host.gem5.is_some() {
    base.host.gem5 = override_config.host.gem5;
  }

  // 合并simulation部分
  if !override_config.simulation.arch_type.is_empty() {
    base.simulation.arch_type = override_config.simulation.arch_type;
  }
  if override_config.simulation.quiet {
    base.simulation.quiet = true;
  }
  if override_config.simulation.step_mode {
    base.simulation.step_mode = true;
  }
  if !override_config.simulation.trace_file.is_empty() {
    base.simulation.trace_file = override_config.simulation.trace_file;
  }

  base
}

/// 应用CLI参数覆写配置
pub fn apply_cli_overrides(
  config: &mut AppConfig,
  quiet: bool,
  step: bool,
  trace_file: Option<&str>,
  arch: Option<&str>,
  host_type: Option<&str>,
  test_binary: Option<&str>,
  se_binary: Option<&str>,
  fs_kernel: Option<&str>,
  fs_image: Option<&str>,
  gem5_mode: Option<&str>,
) {
  if quiet {
    config.simulation.quiet = true;
  }
  if step {
    config.simulation.step_mode = true;
  }
  if let Some(file) = trace_file {
    config.simulation.trace_file = file.to_string();
  }
  if let Some(arch_str) = arch {
    config.simulation.arch_type = arch_str.to_string();
  }
  if let Some(host_str) = host_type {
    config.host.host_type = host_str.to_string();
  }
  if let Some(test_binary_path) = test_binary {
    // 对当前主机类型的配置应用test_binary_path
    match config.host.host_type.to_lowercase().as_str() {
      "spike" => {
        if let Some(ref mut spike) = config.host.spike {
          spike.test_binary_path = test_binary_path.to_string();
        }
      },
      "gem5" => {
        if let Some(ref mut gem5) = config.host.gem5 {
          gem5.test_binary_path = test_binary_path.to_string();
        }
      },
      _ => {},
    }
  }
  if let Some(se_binary_path) = se_binary {
    // 应用se_binary_path到gem5配置
    if let Some(ref mut gem5) = config.host.gem5 {
      gem5.se_binary_path = se_binary_path.to_string();
    }
  }
  if let Some(fs_kernel_path) = fs_kernel {
    // 应用fs_kernel_path到gem5配置
    if let Some(ref mut gem5) = config.host.gem5 {
      gem5.fs_kernel_path = fs_kernel_path.to_string();
    }
  }
  if let Some(fs_image_path) = fs_image {
    // 应用fs_image_path到gem5配置
    if let Some(ref mut gem5) = config.host.gem5 {
      gem5.fs_image_path = fs_image_path.to_string();
    }
  }
  if let Some(mode) = gem5_mode {
    // 应用gem5_mode到gem5配置
    if let Some(ref mut gem5) = config.host.gem5 {
      gem5.gem5_mode = mode.to_string();
    }
  }
}

/// 验证配置
pub fn validate_config(config: &AppConfig) -> io::Result<()> {
  // 获取当前主机类型的配置
  let host_config = match config.host.host_type.to_lowercase().as_str() {
    "spike" => config.host.spike.as_ref(),
    "gem5" => config.host.gem5.as_ref(),
    other => {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unsupported host type: {}", other),
      ))
    },
  };

  let host_config = host_config.ok_or_else(|| {
    io::Error::new(
      io::ErrorKind::InvalidData,
      format!("missing host type '{}' configuration", config.host.host_type),
    )
  })?;

  // 验证test_binary_path不为空
  if config.host.host_type.to_lowercase().as_str() == "spike" {
    if host_config.test_binary_path.trim().is_empty() {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "test_binary_path cannot be empty, please specify it through the configuration file or CLI parameters".to_string(),
      ));
    }
  }

  // 验证host_path不为空
  if host_config.host_path.trim().is_empty() {
    return Err(io::Error::new(
      io::ErrorKind::InvalidData,
      "host_path cannot be empty".to_string(),
    ));
  }

  // 验证test_binary_path不为空
  if config.host.host_type.to_lowercase().as_str() == "gem5" {
    if host_config.gem5_mode.to_lowercase().as_str() == "se" {
      if host_config.se_binary_path.trim().is_empty() {
        return Err(io::Error::new(
          io::ErrorKind::InvalidData,
          "se_binary_path cannot be empty, please specify it through the configuration file or CLI parameters".to_string(),
        ));
      }
    }
    if host_config.gem5_mode.to_lowercase().as_str() == "fs" {
      if host_config.fs_kernel_path.trim().is_empty() || host_config.fs_image_path.trim().is_empty() {
        return Err(io::Error::new(
          io::ErrorKind::InvalidData,
          "fs_kernel_path and fs_image_path cannot be empty, please specify it through the configuration file or CLI parameters".to_string(),
        ));
      }
    }
  }

  // 验证arch_type有效
  match config.simulation.arch_type.to_lowercase().as_str() {
    "buckyball" | "gemmini" => {},
    other => {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unsupported arch type: {}", other),
      ))
    },
  }

  Ok(())
}

/// 补全相对路径（相对于bebop文件夹，即CARGO_MANIFEST_DIR）
pub fn resolve_paths(config: &mut AppConfig, bebop_root: &Path) -> io::Result<()> {
  // 处理spike配置
  if let Some(ref mut spike) = config.host.spike {
    spike.host_path = resolve_single_path(&spike.host_path, bebop_root)?;
    spike.test_binary_path = resolve_single_path(&spike.test_binary_path, bebop_root)?;
  }

  // 处理gem5配置
  if let Some(ref mut gem5) = config.host.gem5 {
    gem5.host_path = resolve_single_path(&gem5.host_path, bebop_root)?;
    gem5.test_binary_path = resolve_single_path(&gem5.test_binary_path, bebop_root)?;
    gem5.se_binary_path = resolve_single_path(&gem5.se_binary_path, bebop_root)?;
    gem5.fs_kernel_path = resolve_single_path(&gem5.fs_kernel_path, bebop_root)?;
    gem5.fs_image_path = resolve_single_path(&gem5.fs_image_path, bebop_root)?;
  }

  // 处理trace_file
  if !config.simulation.trace_file.is_empty() {
    config.simulation.trace_file = resolve_single_path(&config.simulation.trace_file, bebop_root)?;
  }

  Ok(())
}

/// 补全单个路径
fn resolve_single_path(path_str: &str, bebop_root: &Path) -> io::Result<String> {
  if path_str.is_empty() {
    return Ok(path_str.to_string());
  }

  let path = Path::new(path_str);

  // 如果已经是绝对路径，直接返回
  if path.is_absolute() {
    return Ok(path_str.to_string());
  }

  // 相对路径相对于bebop_root
  let absolute_path = bebop_root.join(path);

  Ok(absolute_path.to_string_lossy().to_string())
}

/// 加载并合并配置
///
/// 流程：
/// 1. 加载默认配置
/// 2. 如果提供了自定义配置文件，加载并合并
/// 3. 应用CLI参数覆写
/// 4. 补全相对路径
/// 5. 验证配置
pub fn load_and_merge_configs(
  custom_config_path: Option<&str>,
  bebop_root: &Path,
  quiet: bool,
  step: bool,
  trace_file: Option<&str>,
  arch: Option<&str>,
  host_type: Option<&str>,
  test_binary: Option<&str>,
  se_binary: Option<&str>,
  fs_kernel: Option<&str>,
  fs_image: Option<&str>,
  gem5_mode: Option<&str>,
) -> io::Result<AppConfig> {
  // 加载默认配置
  let mut config = load_default_config()?;

  // 如果提供了自定义配置文件，加载并合并
  if let Some(custom_path) = custom_config_path {
    let custom_path_buf = PathBuf::from(custom_path);
    let custom_path_abs = if custom_path_buf.is_absolute() {
      custom_path_buf
    } else {
      bebop_root.join(&custom_path_buf)
    };

    let custom_config = load_config_file(&custom_path_abs)?;
    config = merge_config(config, custom_config);
  }

  // 应用CLI参数覆写
  apply_cli_overrides(
    &mut config,
    quiet,
    step,
    trace_file,
    arch,
    host_type,
    test_binary,
    se_binary,
    fs_kernel,
    fs_image,
    gem5_mode,
  );

  // 补全相对路径
  resolve_paths(&mut config, bebop_root)?;

  // 验证配置
  validate_config(&config)?;

  Ok(config)
}
