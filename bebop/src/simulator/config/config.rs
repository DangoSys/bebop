use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Host type configuration section
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HostTypeConfig {
  pub host_path: String,
  pub test_binary_path: String,
  #[serde(default)]
  pub host_args: Vec<String>,
  // gem5 specific configuration
  #[serde(default)]
  pub gem5_mode: String, // "se" or "fs"
  #[serde(default)]
  pub se_binary_path: String, // test_binary_path in SE mode
  #[serde(default)]
  pub fs_kernel_path: String, // kernel path in FS mode
  #[serde(default)]
  pub fs_image_path: String, // disk image path in FS mode
}

/// Host configuration section
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

/// Simulation configuration section
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

/// Unified application configuration
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

/// Load default configuration from default.toml
pub fn load_default_config() -> io::Result<AppConfig> {
  let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let config_path = manifest_dir
    .join("src")
    .join("simulator")
    .join("config")
    .join("default.toml");

  load_config_file(&config_path)
}

/// Load configuration from specified file
pub fn load_config_file(path: &Path) -> io::Result<AppConfig> {
  let content = fs::read_to_string(path).map_err(|e| {
    io::Error::new(
      io::ErrorKind::NotFound,
      format!("Failed to read config file {:?}: {}", path, e),
    )
  })?;

  toml::from_str::<AppConfig>(&content).map_err(|e| {
    io::Error::new(
      io::ErrorKind::InvalidData,
      format!("Failed to parse TOML config: {}", e),
    )
  })
}

/// Merge two configurations (latter overrides former)
pub fn merge_config(mut base: AppConfig, override_config: AppConfig) -> AppConfig {
  // Merge host section
  if !override_config.host.host_type.is_empty() {
    base.host.host_type = override_config.host.host_type;
  }
  if override_config.host.spike.is_some() {
    base.host.spike = override_config.host.spike;
  }
  if override_config.host.gem5.is_some() {
    base.host.gem5 = override_config.host.gem5;
  }

  // Merge simulation section
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

/// Apply CLI parameter overrides to configuration
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
    // Apply test_binary_path to the configuration of current host type
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
    // Apply se_binary_path to gem5 configuration
    if let Some(ref mut gem5) = config.host.gem5 {
      gem5.se_binary_path = se_binary_path.to_string();
    }
  }
  if let Some(fs_kernel_path) = fs_kernel {
    // Apply fs_kernel_path to gem5 configuration
    if let Some(ref mut gem5) = config.host.gem5 {
      gem5.fs_kernel_path = fs_kernel_path.to_string();
    }
  }
  if let Some(fs_image_path) = fs_image {
    // Apply fs_image_path to gem5 configuration
    if let Some(ref mut gem5) = config.host.gem5 {
      gem5.fs_image_path = fs_image_path.to_string();
    }
  }
  if let Some(mode) = gem5_mode {
    // Apply gem5_mode to gem5 configuration
    if let Some(ref mut gem5) = config.host.gem5 {
      gem5.gem5_mode = mode.to_string();
    }
  }
}

/// Validate configuration
pub fn validate_config(config: &AppConfig) -> io::Result<()> {
  // Get configuration for current host type
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

  // Validate test_binary_path is not empty
  if config.host.host_type.to_lowercase().as_str() == "spike" {
    if host_config.test_binary_path.trim().is_empty() {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "test_binary_path cannot be empty, please specify it through the configuration file or CLI parameters"
          .to_string(),
      ));
    }
  }

  // Validate host_path is not empty
  if host_config.host_path.trim().is_empty() {
    return Err(io::Error::new(
      io::ErrorKind::InvalidData,
      "host_path cannot be empty".to_string(),
    ));
  }

  // Validate test_binary_path is not empty
  if config.host.host_type.to_lowercase().as_str() == "gem5" {
    if host_config.gem5_mode.to_lowercase().as_str() == "se" {
      if host_config.se_binary_path.trim().is_empty() {
        return Err(io::Error::new(
          io::ErrorKind::InvalidData,
          "se_binary_path cannot be empty, please specify it through the configuration file or CLI parameters"
            .to_string(),
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

  // Validate arch_type is valid
  match config.simulation.arch_type.to_lowercase().as_str() {
    "buckyball" | "gemmini" | "verilator" | "verilator-rtl" => {},
    other => {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unsupported arch type: {}", other),
      ))
    },
  }

  Ok(())
}

/// Resolve relative paths (relative to bebop folder, i.e., CARGO_MANIFEST_DIR)
pub fn resolve_paths(config: &mut AppConfig, bebop_root: &Path) -> io::Result<()> {
  // Process spike configuration
  if let Some(ref mut spike) = config.host.spike {
    spike.host_path = resolve_single_path(&spike.host_path, bebop_root)?;
    spike.test_binary_path = resolve_single_path(&spike.test_binary_path, bebop_root)?;
  }

  // Process gem5 configuration
  if let Some(ref mut gem5) = config.host.gem5 {
    gem5.host_path = resolve_single_path(&gem5.host_path, bebop_root)?;
    gem5.test_binary_path = resolve_single_path(&gem5.test_binary_path, bebop_root)?;
    gem5.se_binary_path = resolve_single_path(&gem5.se_binary_path, bebop_root)?;
    gem5.fs_kernel_path = resolve_single_path(&gem5.fs_kernel_path, bebop_root)?;
    gem5.fs_image_path = resolve_single_path(&gem5.fs_image_path, bebop_root)?;
  }

  // Process trace_file
  if !config.simulation.trace_file.is_empty() {
    config.simulation.trace_file = resolve_single_path(&config.simulation.trace_file, bebop_root)?;
  }

  Ok(())
}

/// Resolve a single path
fn resolve_single_path(path_str: &str, bebop_root: &Path) -> io::Result<String> {
  if path_str.is_empty() {
    return Ok(path_str.to_string());
  }

  let path = Path::new(path_str);

  // If already absolute path, return directly
  if path.is_absolute() {
    return Ok(path_str.to_string());
  }

  // Relative path is relative to bebop_root
  let absolute_path = bebop_root.join(path);

  Ok(absolute_path.to_string_lossy().to_string())
}

/// Load and merge configurations
///
/// Process:
/// 1. Load default configuration
/// 2. If custom config file is provided, load and merge it
/// 3. Apply CLI parameter overrides
/// 4. Resolve relative paths
/// 5. Validate configuration
pub fn load_configs(
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
  // Load default configuration
  let mut config = load_default_config()?;

  // If custom config file is provided, load and merge it
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

  // Apply CLI parameter overrides
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

  // Resolve relative paths
  resolve_paths(&mut config, bebop_root)?;

  // Validate configuration
  validate_config(&config)?;

  Ok(config)
}
