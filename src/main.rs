mod emu;

use clap::{Parser, Subcommand};
use emu::config::{BANK_NUM, BANK_SIZE};
use emu::interface::{BemuSpikeInterface, SpikeCallbackParams};
use log::error;

#[derive(Parser)]
#[command(name = "bebop", about = "Bebop BEMU CLI")]
struct Cli {
    #[arg(short, long, default_value_t = false)]
    verbose: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Execute {
        #[arg(short, long)]
        funct: u32,
        #[arg(long, default_value_t = 0)]
        xs1: u64,
        #[arg(long, default_value_t = 0)]
        xs2: u64,
    },
    Info,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    if cli.verbose {
        log::set_max_level(log::LevelFilter::Debug);
    }
    let ret = match cli.command {
        Commands::Execute { funct, xs1, xs2 } => execute(funct, xs1, xs2, cli.verbose),
        Commands::Info => {
            show_info();
            Ok(())
        }
    };
    if let Err(e) = ret {
        error!("{e}");
        std::process::exit(1);
    }
}

fn execute(funct: u32, xs1: u64, xs2: u64, verbose: bool) -> Result<(), String> {
    let mut itf = BemuSpikeInterface::with_verbose(verbose);
    let res = itf
        .handle_custom_instruction(&SpikeCallbackParams::new(funct, xs1, xs2))
        .map_err(|e| format!("execute failed: {e}"))?;
    println!("result=0x{res:x}");
    Ok(())
}

fn show_info() {
    println!("bebop 0.1.0");
    println!("banks={BANK_NUM}, bank_size={}KB", BANK_SIZE / 1024);
    println!("supported funct: 23(mset) 24(mvin) 25(mvout) 32(matmul) 34(transpose)");
}
