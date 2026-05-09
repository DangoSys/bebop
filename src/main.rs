use clap::{Parser, Subcommand};
use snafu::Whatever;

#[cfg(feature = "verilator")]
use bebop_verilator::{run as run_verilator, VerilatorCli};

#[cfg(feature = "bemu")]
use bebop_bemu::{run as run_bemu, BemuCli};

#[cfg(feature = "p2e")]
use bebop_p2e::{run as run_p2e, BitstreamBuilder, P2ECli};

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
        #[arg(long, value_name = "ELF")]
        elf: std::path::PathBuf,
        #[arg(long, help = "Log directory (creates bdb.ndjson and stdout.log)")]
        log_dir: std::path::PathBuf,
        #[arg(long, help = "Waveform directory (creates waveform.fst)")]
        fst_dir: std::path::PathBuf,
        #[arg(long, help = "Enable instruction trace")]
        itrace: bool,
        #[arg(long, help = "Enable memory trace")]
        mtrace: bool,
        #[arg(long, help = "Enable performance counter trace")]
        pmctrace: bool,
        #[arg(long, help = "Enable cycle counter trace")]
        ctrace: bool,
        #[arg(long, help = "Enable bank trace")]
        banktrace: bool,
    },
    #[cfg(feature = "bemu")]
    /// Run the bemu emulator.
    Bemu {
        #[arg(long, value_name = "ELF")]
        elf: std::path::PathBuf,
        #[arg(long, value_name = "DIR")]
        log_dir: Option<std::path::PathBuf>,
    },
    #[cfg(feature = "p2e")]
    /// Run the P2E FPGA flow.
    P2e {
        #[arg(long, help = "Build bitstream")]
        buildbitstream: bool,
        #[arg(long, help = "Run workload")]
        runworkload: bool,
        #[arg(long, help = "Kernel image to load (for runworkload)")]
        image: Option<std::path::PathBuf>,
        #[arg(long, help = "Verilog source directory (for buildbitstream)")]
        vsrc_dir: Option<std::path::PathBuf>,
        #[arg(long, help = "Output directory", default_value = "./out")]
        output_dir: std::path::PathBuf,
        #[arg(long, help = "Log directory", default_value = "./log")]
        log_dir: std::path::PathBuf,
    },
}

fn dispatch(cli: Cli) -> Result<(), Whatever> {
    match cli.command {
        #[cfg(feature = "verilator")]
        Commands::Verilator {
            elf,
            log_dir,
            fst_dir,
            itrace,
            mtrace,
            pmctrace,
            ctrace,
            banktrace,
        } => run_verilator(VerilatorCli {
            elf,
            log_dir,
            fst_dir,
            itrace,
            mtrace,
            pmctrace,
            ctrace,
            banktrace,
        }),
        #[cfg(feature = "bemu")]
        Commands::Bemu { elf, log_dir } => run_bemu(BemuCli { elf, log_dir }),
        #[cfg(feature = "p2e")]
        Commands::P2e {
            buildbitstream,
            runworkload,
            image,
            vsrc_dir,
            output_dir,
            log_dir,
        } => {
            if buildbitstream {
                let vsrc_dir = vsrc_dir
                    .ok_or_else(|| Whatever::without_source("--vsrc-dir is required for buildbitstream".to_string()))?;

                // Build bitstream using BitstreamBuilder
                let builder = BitstreamBuilder::new(vsrc_dir, output_dir);
                builder.build().map_err(|e| Whatever::without_source(e))?;

                Ok(())
            } else if runworkload {
                let image =
                    image.ok_or_else(|| Whatever::without_source("--image is required for runworkload".to_string()))?;
                run_p2e(P2ECli {
                    image,
                    output: output_dir,
                    log: log_dir,
                })
            } else {
                Err(Whatever::without_source(
                    "Must specify either --buildbitstream or --runworkload".to_string(),
                ))
            }
        }
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = dispatch(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
