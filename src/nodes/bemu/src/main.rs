use std::path::PathBuf;
use snafu::Whatever;

use crate::config::parse_args;

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
    println!("elf: {}", self.elf.display());
    println!("log: {}", self.log);
    println!("isa: {}", self.isa);
    println!("procs: {}", self.procs);
    println!("mem_size: {}", self.mem_size);
    println!("batch: {}", self.batch);
    println!("help: {}", self.help);
    Ok(())
  }
}
