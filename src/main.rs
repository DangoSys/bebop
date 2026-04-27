use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "bebop", about = "Bebop CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Run the verilator flow.
    Verilator {
        #[arg(value_name = "ELF")]
        elf: PathBuf,
        #[arg(value_name = "ARGS", trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn dispatch(cli: Cli) -> Result<(), Whatever> {
    match cli.command {
        Commands::Verilator { elf, plusargs } => run_verilator(VerilatorCli { elf, plusargs }),
    }
}

fn main() {
    let cli = Cli::parse();
    dispatch(cli)
}
