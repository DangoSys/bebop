use std::path::PathBuf;
use snafu::Whatever;

use crate::config::parse_args;

#[derive(Debug, Clone)]
pub struct VerilatorCli {
  pub elf: PathBuf,
  pub args: Vec<String>,
}

pub fn run(cli: VerilatorCli) -> Result<(), Whatever> {
  let sim = VerilatorSim::config(cli)?;
  sim.run()
}

#[derive(Debug, Clone)]
struct VerilatorSim {
  elf: PathBuf,
  log: String,
  fst: String,
  trace: Option<String>,
  trace_mask: Option<String>,
  batch: bool,
  help: bool,
}

impl VerilatorSim {
  fn config(cli: VerilatorCli) -> Result<Self, Whatever> {
    let (log, fst, trace, trace_mask, batch, help) = parse_args(cli.args)?;
    Ok(Self {
      elf: cli.elf,
      log,
      fst,
      trace,
      trace_mask,
      batch,
      help,
    })
  }

  fn run(self) -> Result<(), Whatever> {
    println!("elf: {}", self.elf.display());
    println!("log: {}", self.log);
    println!("fst: {}", self.fst);
    println!("trace: {:?}", self.trace);
    println!("trace_mask: {:?}", self.trace_mask);
    println!("batch: {}", self.batch);
    println!("help: {}", self.help);
    Ok(())
  }
}

