use std::path::PathBuf;
use snafu::{ResultExt, Whatever};

use crate::config::parse_args;
use crate::ffi::run_spike;

#[derive(Debug, Clone)]
pub struct BemuCli {
  pub elf: PathBuf,
  pub args: Vec<String>,
}

pub fn run(cli: BemuCli) -> Result<(), Whatever> {
  let sim = BemuSim::config(cli)?;
  sim.run()
}

#[derive(Debug, Clone)]
struct BemuSim {
  elf: PathBuf,
  log: String,
  isa: String,
  procs: usize,
  mem_size: String,
  batch: bool,
  help: bool,
}

impl BemuSim {
  fn config(cli: BemuCli) -> Result<Self, Whatever> {
    let (log, isa, procs, mem_size, batch, help) = parse_args(cli.args)?;
    Ok(Self {
      elf: cli.elf,
      log,
      isa,
      procs,
      mem_size,
      batch,
      help,
    })
  }

  fn run(self) -> Result<(), Whatever> {
    let mem_mb: usize = self.mem_size.parse()
        .map_err(|_| format!("invalid mem_size: {}", self.mem_size))
        .whatever_context("failed to parse mem_size")?;

    let elf_path = self.elf.to_str()
        .ok_or_else(|| "invalid elf path".to_string())
        .whatever_context("failed to convert elf path")?;

    let log_path = if self.log.is_empty() {
        None
    } else {
        Some(self.log.as_str())
    };

    run_spike(&self.isa, self.procs, mem_mb, elf_path, log_path)
        .whatever_context("spike execution failed")
  }
}
