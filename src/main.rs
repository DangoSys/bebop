use clap::{Parser, Subcommand};
#[cfg(feature = "p2e")]
use snafu::FromString;
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
        #[arg(long, help = "Log directory (creates bdb.ndjson, stdout.log, stderr.log, and uart/)")]
        log_dir: std::path::PathBuf,
        #[arg(long, help = "Waveform directory (creates waveform.fst)")]
        fst_dir: std::path::PathBuf,
        #[arg(long, help = "Disable waveform dump")]
        no_wave: bool,
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
        #[arg(long, help = "Run with proxy kernel (Linux mode, starts in S-mode)")]
        pk: bool,
    },
    #[cfg(feature = "p2e")]
    /// Run the P2E FPGA flow.
    P2e {
        #[arg(long, help = "Build bitstream")]
        buildbitstream: bool,
        #[arg(long, help = "Run workload")]
        runworkload: bool,
        #[arg(
            long,
            help = "Design build directory (for buildbitstream: contains vvacDir and outputs; for runworkload: contains bitstream)"
        )]
        build_dir: Option<std::path::PathBuf>,
        #[arg(long, help = "Kernel image name to load (for runworkload)")]
        image: Option<std::path::PathBuf>,
        #[arg(long, help = "Bitstream file path (for runworkload)")]
        bitstream: Option<std::path::PathBuf>,
        #[arg(long, help = "Log directory (for runworkload only)")]
        log_dir: Option<std::path::PathBuf>,
        #[arg(long, help = "Enable waveform dump during runworkload")]
        wave: bool,
        #[arg(long, help = "Start waveform dump from this cycle")]
        wave_start: Option<u64>,
    },
}

fn dispatch(cli: Cli) -> Result<(), Whatever> {
    match cli.command {
        #[cfg(feature = "verilator")]
        Commands::Verilator {
            elf,
            log_dir,
            fst_dir,
            no_wave,
            itrace,
            mtrace,
            pmctrace,
            ctrace,
            banktrace,
        } => run_verilator(VerilatorCli {
            elf,
            log_dir,
            fst_dir,
            wave: !no_wave,
            itrace,
            mtrace,
            pmctrace,
            ctrace,
            banktrace,
        }),
        #[cfg(feature = "bemu")]
        Commands::Bemu { elf, log_dir, pk } => run_bemu(BemuCli { elf, log_dir, pk }),
        #[cfg(feature = "p2e")]
        Commands::P2e {
            buildbitstream,
            runworkload,
            build_dir,
            image,
            bitstream,
            log_dir,
            wave,
            wave_start,
        } => {
            if buildbitstream {
                let build_dir = build_dir.ok_or_else(|| {
                    Whatever::without_source("--build-dir is required for buildbitstream".to_string())
                })?;
                let builder = BitstreamBuilder::new(build_dir);
                builder.build().map_err(Whatever::without_source)?;

                Ok(())
            } else if runworkload {
                let image =
                    image.ok_or_else(|| Whatever::without_source("--image is required for runworkload".to_string()))?;
                let bitstream = bitstream
                    .ok_or_else(|| Whatever::without_source("--bitstream is required for runworkload".to_string()))?;
                let build_dir = build_dir.unwrap_or_else(|| std::path::PathBuf::from("./out"));
                let log = log_dir.unwrap_or_else(|| build_dir.join("log"));
                let wave = wave || wave_start.is_some();

                run_p2e(P2ECli {
                    image,
                    bitstream,
                    output: build_dir,
                    log,
                    wave,
                    wave_start,
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
