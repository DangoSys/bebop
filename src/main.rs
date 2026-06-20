use clap::{Parser, Subcommand};
#[cfg(any(feature = "p2e", all(feature = "bemu", feature = "verilator")))]
use snafu::FromString;
use snafu::Whatever;
use std::path::PathBuf;

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
    #[cfg(all(feature = "bemu", feature = "verilator"))]
    /// Run BEMU and Verilator with online bank hash packet comparison.
    BankHashDifftest {
        #[arg(long, value_name = "ELF")]
        elf: PathBuf,
        #[arg(long, value_name = "DIR")]
        out_dir: PathBuf,
        #[arg(long, help = "Run BEMU with proxy kernel (Linux mode, starts in S-mode)")]
        pk: bool,
        #[arg(long, help = "Enable Verilator waveform dump")]
        wave: bool,
    },
    #[cfg(feature = "verilator")]
    /// Run the verilator flow.
    Verilator {
        #[arg(long, value_name = "ELF")]
        elf: PathBuf,
        #[arg(long, help = "Log directory (creates bdb.ndjson, stdout.log, stderr.log, and uart/)")]
        log_dir: PathBuf,
        #[arg(long, help = "Waveform directory (creates waveform.fst)")]
        fst_dir: PathBuf,
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
        elf: PathBuf,
        #[arg(long, value_name = "DIR")]
        log_dir: Option<PathBuf>,
        #[arg(long, help = "Run with proxy kernel (Linux mode, starts in S-mode)")]
        pk: bool,
        #[arg(long, help = "Enable instruction trace")]
        itrace: bool,
        #[arg(long, help = "Enable memory trace")]
        mtrace: bool,
        #[arg(long, help = "Enable bank trace")]
        banktrace: bool,
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
        build_dir: Option<PathBuf>,
        #[arg(long, help = "Kernel image name to load (for runworkload)")]
        image: Option<PathBuf>,
        #[arg(long, help = "Bitstream file path (for runworkload)")]
        bitstream: Option<PathBuf>,
        #[arg(long, help = "Log directory (for runworkload only)")]
        log_dir: Option<PathBuf>,
        #[arg(long, help = "Use multi-FPGA hw_server connection without a location selector")]
        multi_fpga: bool,
        #[arg(long, help = "Enable waveform dump during runworkload")]
        wave: bool,
        #[arg(long, help = "Start waveform dump from this cycle")]
        wave_start: Option<u64>,
    },
}

fn dispatch(cli: Cli) -> Result<(), Whatever> {
    match cli.command {
        #[cfg(all(feature = "bemu", feature = "verilator"))]
        Commands::BankHashDifftest { elf, out_dir, pk, wave } => {
            run_bank_hash_difftest(BankHashDifftestCli { elf, out_dir, pk, wave })
        }
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
        Commands::Bemu {
            elf,
            log_dir,
            pk,
            itrace,
            mtrace,
            banktrace,
        } => run_bemu(BemuCli {
            elf,
            log_dir,
            pk,
            itrace,
            mtrace,
            banktrace,
        }),
        #[cfg(feature = "p2e")]
        Commands::P2e {
            buildbitstream,
            runworkload,
            build_dir,
            image,
            bitstream,
            log_dir,
            multi_fpga,
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
                let build_dir = build_dir.unwrap_or_else(|| PathBuf::from("./out"));
                let log = log_dir.unwrap_or_else(|| build_dir.join("log"));
                let wave = wave || wave_start.is_some();

                run_p2e(P2ECli {
                    image,
                    bitstream,
                    output: build_dir,
                    log,
                    multi_fpga,
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

#[cfg(all(feature = "bemu", feature = "verilator"))]
#[derive(Debug, Clone)]
struct BankHashDifftestCli {
    elf: PathBuf,
    out_dir: PathBuf,
    pk: bool,
    wave: bool,
}

#[cfg(all(feature = "bemu", feature = "verilator"))]
fn run_bank_hash_difftest(cli: BankHashDifftestCli) -> Result<(), Whatever> {
    let bemu_log_dir = cli.out_dir.join("bemu");
    let rtl_log_dir = cli.out_dir.join("rtl").join("log");
    let rtl_fst_dir = cli.out_dir.join("rtl").join("fst");
    let compare_output = cli.out_dir.join("bank_hash_compare.ndjson");

    std::fs::create_dir_all(&cli.out_dir)
        .map_err(|e| Whatever::without_source(format!("failed to create {}: {e}", cli.out_dir.display())))?;

    let packets = bebop_bank_hash::init_runtime_packet_channel();

    println!("Bank hash difftest output: {}", cli.out_dir.display());
    println!("Running BEMU...");
    let bemu_result = run_bemu(BemuCli {
        elf: cli.elf.clone(),
        log_dir: Some(bemu_log_dir),
        pk: cli.pk,
        itrace: true,
        mtrace: true,
        banktrace: true,
    });
    if let Err(e) = &bemu_result {
        eprintln!("BEMU failed before compare: {e}");
    }

    println!("Running Verilator...");
    let rtl_result = run_verilator(VerilatorCli {
        elf: cli.elf,
        log_dir: rtl_log_dir,
        fst_dir: rtl_fst_dir,
        wave: cli.wave,
        itrace: true,
        mtrace: true,
        pmctrace: false,
        ctrace: false,
        banktrace: true,
    });
    if let Err(e) = &rtl_result {
        eprintln!("RTL failed before compare: {e}");
    }

    bebop_bank_hash::shutdown_runtime_packet_channel();
    println!("Running online Bank Hash comparator...");
    let compare_result = bebop_bank_hash::run_online_compare_with_summary(packets, compare_output.clone());

    match &compare_result {
        Ok(summary) => {
            println!(
                "Bank hash difftest online summary: PASS={} MISMATCH={} MISSING_RTL={} MISSING_BEMU={} TOTAL={}",
                summary.pass,
                summary.mismatch,
                summary.missing_rtl,
                summary.missing_bemu,
                summary.total()
            );
        }
        Err(e) => {
            eprintln!("Bank Hash comparator failed: {e}");
        }
    }

    let mut failures = Vec::new();
    if let Err(e) = bemu_result {
        failures.push(format!("BEMU failed: {e}"));
    }
    if let Err(e) = rtl_result {
        failures.push(format!("RTL failed: {e}"));
    }
    match compare_result {
        Ok(summary)
            if summary.pass > 0 && summary.mismatch == 0 && summary.missing_rtl == 0 && summary.missing_bemu == 0 => {}
        Ok(_) => failures.push(format!("Bank Hash compare failed; see {}", compare_output.display())),
        Err(e) => failures.push(format!("Bank Hash compare could not run: {e}")),
    }

    if failures.is_empty() {
        println!("Bank hash difftest: PASS");
        return Ok(());
    }

    println!("Bank hash difftest: FAIL");
    Err(Whatever::without_source(failures.join("; ")))
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = dispatch(cli) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
