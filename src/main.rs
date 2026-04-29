use clap::{Parser, Subcommand};
use snafu::Whatever;

#[cfg(feature = "verilator")]
use bebop_verilator::{run as run_verilator, VerilatorCli};

#[cfg(feature = "bemu")]
use bebop_bemu::{run as run_bemu, BemuCli};

#[cfg(feature = "p2e")]
use bebop_p2e::{run as run_p2e, P2ECli};

#[derive(Debug, Parser)]
#[command(name = "bebop", about = "Bebop CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[cfg(feature = "verilator")]
    /// Run the verilator flow.
    Verilator {
        #[arg(value_name = "ELF")]
        elf: std::path::PathBuf,
        #[arg(
            value_name = "ARGS",
            trailing_var_arg = true,
            allow_hyphen_values = true
        )]
        args: Vec<String>,
    },
    #[cfg(feature = "bemu")]
    /// Run the bemu emulator.
    Bemu {
        #[arg(value_name = "ELF")]
        elf: std::path::PathBuf,
        #[arg(
            value_name = "ARGS",
            trailing_var_arg = true,
            allow_hyphen_values = true
        )]
        args: Vec<String>,
    },
    #[cfg(feature = "p2e")]
    /// Run the P2E FPGA flow.
    P2e {
        #[arg(long, help = "Build the P2E bitstream")]
        buildbitstream: bool,
        #[arg(long, help = "Run the workload through VVAC CTB")]
        runworkload: bool,
    },
}

fn dispatch(cli: Cli) -> Result<(), Whatever> {
    match cli.command {
        #[cfg(feature = "verilator")]
        Commands::Verilator { elf, args } => run_verilator(VerilatorCli { elf, args }),
        #[cfg(feature = "bemu")]
        Commands::Bemu { elf, args } => run_bemu(BemuCli { elf, args }),
        #[cfg(feature = "p2e")]
        Commands::P2e {
            buildbitstream,
            runworkload,
        } => run_p2e(P2ECli {
            buildbitstream,
            runworkload,
        }),
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = dispatch(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
